//! 条目元信息富集——rexxar 结构化详情 → subjects 跨用户缓存。
//! 设计与验收场景见《条目元信息富集方案.md》。

use anyhow::{Context, Result};
use rand::Rng;
use serde_json::{Map, Value, json};
use sqlx::{Row, SqlitePool};

use crate::crawler::Category;

use super::config::ServerConfig;
use super::covers::cover_proxy_url;
use super::error::{AppError, AppResult};
use super::fetcher::{DoubanFetcher, FetchError};
use super::service::ms_to_rfc3339;
use super::state::AppState;

/// rexxar 校验请求来源：Referer 必须指向 m.douban.com，UA 需为移动端
pub const REXXAR_REFERER: &str = "https://m.douban.com/";
pub const MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) \
     AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1";

/// 单条目富集最大尝试次数（含首次），对齐维度抓取的 MAX_ATTEMPTS
const MAX_ATTEMPTS: i64 = 5;

pub fn rexxar_url(category: &str, subject_id: &str) -> String {
    format!("https://m.douban.com/rexxar/api/v2/{category}/{subject_id}")
}

// --------------------------------------------------------------------------- //
// rexxar 响应 → subjects 行（映射契约见方案 §2.2）
// --------------------------------------------------------------------------- //

/// 映射产物：subjects 表除主键与状态列之外的完整载荷
pub struct SubjectRow {
    pub title: String,
    pub original_title: String,
    pub year: Option<i64>,
    pub aka_json: String,
    pub url: String,
    pub cover_url: String,
    pub rating_value: Option<f64>,
    pub rating_count: Option<i64>,
    pub meta_json: String,
    pub raw_json: String,
}

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// 取数组中的字符串元素（豆瓣数组偶含非字符串项，逐个过滤）
fn arr(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// 对象数组的 name 字段（directors/actors/singer 均为 [{name: …}]）
fn arr_name(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// book/music 无独立 year 字段，取 pubdate[0] 前四位
fn year_from_pubdate(v: &Value) -> Option<i64> {
    arr(v, "pubdate")
        .first()
        .and_then(|d| d.get(..4)?.parse().ok())
}

fn opt_i64(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_i64)
}

pub fn map_rexxar(category: Category, v: &Value) -> SubjectRow {
    let mut meta = Map::new();
    match category {
        Category::Movie => {
            meta.insert("directors".into(), json!(arr_name(v, "directors")));
            meta.insert("actors".into(), json!(arr_name(v, "actors")));
            meta.insert("genres".into(), json!(arr(v, "genres")));
            meta.insert("countries".into(), json!(arr(v, "countries")));
            meta.insert("languages".into(), json!(arr(v, "languages")));
            meta.insert("durations".into(), json!(arr(v, "durations")));
            meta.insert("pubdate".into(), json!(arr(v, "pubdate")));
            meta.insert("is_tv".into(), json!(v.get("is_tv").and_then(Value::as_bool)));
            meta.insert(
                "episodes_count".into(),
                json!(opt_i64(v, "episodes_count")),
            );
            meta.insert("intro".into(), json!(s(v, "intro")));
        }
        Category::Book => {
            // rexxar 语义权威：author 与 translator 分列（intro 文本无法区分）
            meta.insert("authors".into(), json!(arr(v, "author")));
            meta.insert("translators".into(), json!(arr(v, "translator")));
            meta.insert("press".into(), json!(arr(v, "press")));
            meta.insert("pubdate".into(), json!(arr(v, "pubdate")));
            meta.insert("pages".into(), json!(arr(v, "pages")));
            meta.insert("price".into(), json!(arr(v, "price")));
            meta.insert("subtitle".into(), json!(s(v, "subtitle")));
            meta.insert(
                "series".into(),
                json!(v
                    .pointer("/book_series/title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()),
            );
            meta.insert("intro".into(), json!(s(v, "intro")));
        }
        Category::Music => {
            meta.insert("singers".into(), json!(arr_name(v, "singer")));
            meta.insert("genres".into(), json!(arr(v, "genres")));
            meta.insert("media".into(), json!(arr(v, "media")));
            meta.insert("discs".into(), json!(arr(v, "discs")));
            meta.insert("pubdate".into(), json!(arr(v, "pubdate")));
            meta.insert("publisher".into(), json!(arr(v, "publisher")));
            meta.insert("version".into(), json!(arr(v, "version")));
            meta.insert(
                "songs".into(),
                json!(arr_name(v, "songs")),
            );
            meta.insert("intro".into(), json!(s(v, "intro")));
        }
    }

    // movie 的 year 是字符串（"1994"）或数字，两种都收
    let year = match category {
        Category::Movie => v.get("year").and_then(|y| {
            y.as_i64().or_else(|| y.as_str().and_then(|s| s.parse().ok()))
        }),
        _ => year_from_pubdate(v),
    };

    SubjectRow {
        title: s(v, "title"),
        original_title: s(v, "original_title"),
        year,
        aka_json: json!(arr(v, "aka")).to_string(),
        url: s(v, "url"),
        cover_url: s(v, "cover_url"),
        rating_value: v.pointer("/rating/value").and_then(Value::as_f64),
        rating_count: v.pointer("/rating/count").and_then(Value::as_i64),
        meta_json: json!(meta).to_string(),
        raw_json: v.to_string(),
    }
}

// --------------------------------------------------------------------------- //
// 入队：集合差 + INSERT OR IGNORE（跨用户去重的核心，方案 §5.1）
// --------------------------------------------------------------------------- //

/// 把该用户该类别下"从未富集 / 已过期 / 上次失败"的条目排入富集队列。
/// 返回本次新排入的 job 数（已被其他用户排过或数据仍新鲜的条目为 0 增量）。
pub async fn enqueue_missing_subjects(
    pool: &SqlitePool,
    uid: &str,
    category: &str,
    ttl_secs: i64,
) -> Result<u64> {
    let now = ServerConfig::now_ms();
    let stale_before = now - ttl_secs * 1000;
    let res = sqlx::query(
        "INSERT OR IGNORE INTO subject_jobs(category, subject_id, created_ms, updated_ms)
         SELECT DISTINCT r.category, r.subject_id, ?, ?
         FROM records r
         LEFT JOIN subjects s
                ON s.category = r.category AND s.subject_id = r.subject_id
         WHERE r.uid = ? AND r.category = ?
           AND ( s.category IS NULL
              OR (s.status = 'ok' AND s.fetched_ms < ?)
              OR s.status IN ('blocked', 'error') )",
    )
    .bind(now)
    .bind(now)
    .bind(uid)
    .bind(category)
    .bind(stale_before)
    .execute(pool)
    .await
    .context("富集队列入队失败")?;
    Ok(res.rows_affected())
}

/// 存量回填：对 records 里出现的每个 (uid, category) 逐个跑集合差（幂等）。
/// 返回 (新排入 job 数, 覆盖的 uid×类别对数)。
pub async fn backfill(pool: &SqlitePool, ttl_secs: i64) -> Result<(u64, usize)> {
    let pairs = sqlx::query("SELECT DISTINCT uid, category FROM records ORDER BY uid, category")
        .fetch_all(pool)
        .await
        .context("回填读取 records 失败")?;
    let mut queued = 0u64;
    for row in &pairs {
        let uid: String = row.get("uid");
        let category: String = row.get("category");
        queued += enqueue_missing_subjects(pool, &uid, &category, ttl_secs).await?;
    }
    Ok((queued, pairs.len()))
}

/// 强制刷新单个条目：无视 TTL 重新排队（priority=0 优先执行）
pub async fn force_refresh(pool: &SqlitePool, category: &str, subject_id: &str) -> Result<()> {
    let now = ServerConfig::now_ms();
    sqlx::query(
        "INSERT INTO subject_jobs (category, subject_id, state, priority, created_ms, updated_ms)
         VALUES (?, ?, 'queued', 0, ?, ?)
         ON CONFLICT(category, subject_id) DO UPDATE SET
            state = 'queued', attempts = 0, next_run_ms = NULL,
            priority = 0, updated_ms = excluded.updated_ms",
    )
    .bind(category)
    .bind(subject_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .context("条目刷新入队失败")?;
    Ok(())
}

// --------------------------------------------------------------------------- //
// worker 执行（与 dimensions 同构的取任务-执行-退避，方案 §6）
// --------------------------------------------------------------------------- //

struct SubjectJob {
    category: String,
    subject_id: String,
    attempts: i64,
}

/// 处理一个到期富集 job；返回是否处理了（供 worker 循环与测试复用）。
/// 维度队列优先，本方法只在 worker 空闲时段被调用。
pub async fn process_next_job<F: DoubanFetcher>(state: &AppState, fetcher: &F) -> Result<bool> {
    if !state.config.subject_enrich {
        return Ok(false);
    }
    let Some(job) = pick_due_job(&state.pool).await? else {
        return Ok(false);
    };
    let now = ServerConfig::now_ms();
    // 标记 running：崩溃后残留的 running 行也会被 pick_due_job 重新拾起
    sqlx::query(
        "UPDATE subject_jobs SET state='running', updated_ms=? WHERE category=? AND subject_id=?",
    )
    .bind(now)
    .bind(&job.category)
    .bind(&job.subject_id)
    .execute(&state.pool)
    .await?;

    match fetcher.fetch_subject(&job.category, &job.subject_id).await {
        Ok(value) => {
            state.record_crawl_success();
            // 全局节流：与页面抓取共用令牌桶语义，取两类间隔的较大值
            let interval = state
                .config
                .crawl_min_interval_secs
                .max(state.config.subject_min_interval_secs)
                + rand::thread_rng().gen_range(0.0..1.0);
            tokio::time::sleep(std::time::Duration::from_secs_f64(interval)).await;

            let category = Category::from_key(&job.category)
                .with_context(|| format!("未知 category: {}", job.category))?;
            let row = map_rexxar(category, &value);
            upsert_subject_row(&state.pool, &job.category, &job.subject_id, &row).await?;
            sqlx::query(
                "UPDATE subject_jobs SET state='ok', attempts=0, next_run_ms=NULL,
                        last_error=NULL, updated_ms=?
                 WHERE category=? AND subject_id=?",
            )
            .bind(ServerConfig::now_ms())
            .bind(&job.category)
            .bind(&job.subject_id)
            .execute(&state.pool)
            .await?;
            tracing::info!(
                subject = format!("{}/{}", job.category, job.subject_id),
                title = %row.title,
                "条目元信息富集完成"
            );
        }
        Err(e) => handle_job_error(state, &job, e).await?,
    }
    Ok(true)
}

async fn pick_due_job(pool: &SqlitePool) -> Result<Option<SubjectJob>> {
    let row = sqlx::query(
        "SELECT category, subject_id, attempts FROM subject_jobs
         WHERE state IN ('queued','running')
           AND (next_run_ms IS NULL OR next_run_ms <= ?)
         ORDER BY priority ASC, updated_ms ASC, rowid ASC
         LIMIT 1",
    )
    .bind(ServerConfig::now_ms())
    .fetch_optional(pool)
    .await
    .context("查询到期富集 job 失败")?;
    Ok(row.map(|r| SubjectJob {
        category: r.get("category"),
        subject_id: r.get("subject_id"),
        attempts: r.get("attempts"),
    }))
}

/// 失败处理对齐维度抓取：404 永久终态 not_found；其余线性退避，
/// 超限终态 error；可重试失败计入全局熔断。
async fn handle_job_error(state: &AppState, job: &SubjectJob, e: FetchError) -> Result<()> {
    let now = ServerConfig::now_ms();
    tracing::warn!(
        subject = format!("{}/{}", job.category, job.subject_id),
        attempt = job.attempts + 1,
        error = %e,
        "条目富集失败"
    );
    let permanent = !e.is_transient();
    let over_limit = job.attempts + 1 >= MAX_ATTEMPTS;
    let (job_state, next_run) = if permanent {
        ("not_found", None) // 404：条目已被豆瓣删除，终态
    } else if over_limit {
        ("error", None) // 重试耗尽终态
    } else {
        (
            "queued",
            Some(now + (job.attempts + 1) * 20_000 + rand::thread_rng().gen_range(0..5_000)),
        )
    };

    sqlx::query(
        "UPDATE subject_jobs SET state=?, attempts=?, next_run_ms=?, last_error=?, updated_ms=?
         WHERE category=? AND subject_id=?",
    )
    .bind(job_state)
    .bind(job.attempts + 1)
    .bind(next_run)
    .bind(format!("{}: {e}", e.code()))
    .bind(now)
    .bind(&job.category)
    .bind(&job.subject_id)
    .execute(&state.pool)
    .await?;
    if !permanent {
        let tripped = state.record_crawl_failure();
        if tripped {
            tracing::info!("熔断已触发，worker 将暂停抓取直至冷却结束");
        }
    }
    Ok(())
}

async fn upsert_subject_row(
    pool: &SqlitePool,
    category: &str,
    subject_id: &str,
    row: &SubjectRow,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO subjects (category, subject_id, title, original_title, year, aka_json,
                               url, cover_url, rating_value, rating_count, meta_json, raw_json,
                               status, attempts, next_retry_ms, fetched_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'ok', 0, NULL, ?)
         ON CONFLICT(category, subject_id) DO UPDATE SET
            title = excluded.title, original_title = excluded.original_title,
            year = excluded.year, aka_json = excluded.aka_json,
            url = excluded.url, cover_url = excluded.cover_url,
            rating_value = excluded.rating_value, rating_count = excluded.rating_count,
            meta_json = excluded.meta_json, raw_json = excluded.raw_json,
            status = 'ok', attempts = 0, next_retry_ms = NULL,
            fetched_ms = excluded.fetched_ms",
    )
    .bind(category)
    .bind(subject_id)
    .bind(&row.title)
    .bind(&row.original_title)
    .bind(row.year)
    .bind(&row.aka_json)
    .bind(&row.url)
    .bind(&row.cover_url)
    .bind(row.rating_value)
    .bind(row.rating_count)
    .bind(&row.meta_json)
    .bind(&row.raw_json)
    .bind(ServerConfig::now_ms())
    .execute(pool)
    .await
    .context("条目元信息入库失败")?;
    Ok(())
}

// --------------------------------------------------------------------------- //
// 读侧：API 视图
// --------------------------------------------------------------------------- //

/// subjects 行 → 对外 JSON（结构见方案 §7.1）
fn subject_view(category: &str, subject_id: &str, r: &sqlx::sqlite::SqliteRow) -> Value {
    let meta_json: String = r.get("meta_json");
    let aka_json: String = r.get("aka_json");
    let fetched_ms: i64 = r.get("fetched_ms");
    let rating = json!({
        "value": r.try_get::<Option<f64>, _>("rating_value").ok().flatten(),
        "count": r.try_get::<Option<i64>, _>("rating_count").ok().flatten(),
    });
    let mut view = json!({
        "category": category,
        "subject_id": subject_id,
        "title": r.get::<String, _>("title"),
        "original_title": r.get::<String, _>("original_title"),
        "aka": serde_json::from_str::<Value>(&aka_json).unwrap_or(json!([])),
        "year": r.try_get::<Option<i64>, _>("year").ok().flatten(),
        "rating": rating,
        // 与 records.cover 同理：豆瓣图域有 Referer 防盗链，出口改写走本地代理
        "cover": cover_proxy_url(&r.get::<String, _>("cover_url")),
        "status": r.get::<String, _>("status"),
        "fetched_at": ms_to_rfc3339(fetched_ms),
    });
    // 类别专属节点：键随 category（movie/book/music）
    view[category] = serde_json::from_str::<Value>(&meta_json).unwrap_or(json!({}));
    view
}

pub async fn get_subject_view(state: &AppState, category: &str, subject_id: &str) -> AppResult<Value> {
    let row = sqlx::query(
        "SELECT title, original_title, year, aka_json, cover_url, rating_value,
                rating_count, meta_json, status, fetched_ms
         FROM subjects WHERE category = ? AND subject_id = ?",
    )
    .bind(category)
    .bind(subject_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::SubjectNotFound(format!("{category}/{subject_id}")))?;
    Ok(subject_view(category, subject_id, &row))
}

/// marks?expand=subject 的批量读：按传入顺序对齐返回（未富集为 None）
pub async fn load_subject_views(
    state: &AppState,
    category: &str,
    subject_ids: &[String],
) -> Result<Vec<Option<Value>>> {
    if subject_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; subject_ids.len()].join(",");
    let sql = format!(
        "SELECT subject_id, title, original_title, year, aka_json, cover_url,
                rating_value, rating_count, meta_json, status, fetched_ms
         FROM subjects WHERE category = ? AND subject_id IN ({placeholders})"
    );
    let mut q = sqlx::query(&sql).bind(category);
    for id in subject_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(&state.pool).await.context("批量读取条目元信息失败")?;
    let by_id: std::collections::HashMap<String, Value> = rows
        .iter()
        .map(|r| {
            let id: String = r.get("subject_id");
            (id.clone(), subject_view(category, &id, r))
        })
        .collect();
    Ok(subject_ids
        .iter()
        .map(|id| by_id.get(id).cloned())
        .collect())
}

/// 全局富集进度计数（sync result 汇总用）
pub async fn enrichment_counts(pool: &SqlitePool) -> Result<(i64, i64)> {
    let enriched: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM subjects WHERE status = 'ok'")
            .fetch_one(pool)
            .await
            .context("统计已富集条目失败")?;
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM subject_jobs WHERE state IN ('queued','running')",
    )
    .fetch_one(pool)
    .await
    .context("统计待富集条目失败")?;
    Ok((enriched, pending))
}

// --------------------------------------------------------------------------- //
// 映射黄金用例：锁定三类 rexxar 真实响应的字段映射（fixture 为 2026-10-28 实抓）
// --------------------------------------------------------------------------- //

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(name: &str) -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn map_rexxar_movie_fixture() {
        let row = map_rexxar(Category::Movie, &fixture("rexxar_movie.json"));
        assert_eq!(row.title, "肖申克的救赎");
        assert_eq!(row.original_title, "The Shawshank Redemption");
        assert_eq!(row.year, Some(1994));
        assert_eq!(row.rating_value, Some(9.7));
        assert!(row.rating_count.unwrap_or(0) > 3_000_000);
        let meta: Value = serde_json::from_str(&row.meta_json).unwrap();
        assert_eq!(meta["directors"], json!(["弗兰克·德拉邦特"]));
        assert_eq!(meta["actors"][0], json!("蒂姆·罗宾斯"));
        assert_eq!(meta["genres"], json!(["剧情", "犯罪"]));
        assert_eq!(meta["countries"], json!(["美国"]));
        assert_eq!(meta["languages"], json!(["英语"]));
        assert_eq!(meta["durations"], json!(["142分钟"]));
        assert_eq!(meta["is_tv"], json!(false));
        let aka: Value = serde_json::from_str(&row.aka_json).unwrap();
        assert!(aka.as_array().unwrap().iter().any(|v| v == "月黑高飞(港)"));
        assert!(row.raw_json.contains("Shawshank"));
    }

    #[test]
    fn map_rexxar_book_fixture() {
        let row = map_rexxar(Category::Book, &fixture("rexxar_book.json"));
        assert_eq!(row.title, "浮生六记");
        // book 无独立 year 字段，取 pubdate[0] 前四位
        assert_eq!(row.year, Some(2026));
        assert_eq!(row.rating_value, Some(9.1));
        let meta: Value = serde_json::from_str(&row.meta_json).unwrap();
        // author 与 translator 分列——intro 纯文本无法做到的语义区分
        assert_eq!(meta["authors"], json!(["沈复"]));
        assert_eq!(meta["translators"], json!(["羽戈"]));
        assert_eq!(meta["press"], json!(["广西师范大学出版社"]));
        assert_eq!(meta["pubdate"], json!(["2026-2"]));
        assert_eq!(meta["price"], json!(["56.00元"]));
        assert_eq!(meta["pages"], json!(["328"]));
        assert_eq!(meta["series"], json!("人间三梦：古典文库系列"));
    }

    #[test]
    fn map_rexxar_music_fixture() {
        let row = map_rexxar(Category::Music, &fixture("rexxar_music.json"));
        assert_eq!(row.title, "歌之女");
        assert_eq!(row.year, Some(1995));
        let meta: Value = serde_json::from_str(&row.meta_json).unwrap();
        assert_eq!(meta["singers"], json!(["梅艷芳", "Anita Mui"]));
        assert_eq!(meta["genres"], json!(["流行"]));
        assert_eq!(meta["media"], json!(["CD"]));
        assert_eq!(meta["discs"], json!(["1"]));
    }
}
