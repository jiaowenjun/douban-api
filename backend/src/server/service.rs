//! 任务/数据的服务层——创建任务、查询进度、记录入库与查询、缓存读取、过期清理。

use rand::Rng;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};

use crate::crawler::{Category, Record, Status, parse_uid};

use super::config::ServerConfig;
use super::covers::cover_proxy_url;
use super::error::{AppError, AppResult};
use super::fetcher::DoubanFetcher;
use super::state::AppState;

pub const TERMINAL_STATUSES: [&str; 3] = ["completed", "partial", "failed"];

// --------------------------------------------------------------------------- //
// 记录入库（worker 与测试共用）
// --------------------------------------------------------------------------- //

/// 批量写维度记录；主键 (uid, category, dim_status, subject_id) 冲突即忽略，
/// 天然完成翻页偏移/标记新增场景下的去重。
pub async fn upsert_records(
    pool: &SqlitePool,
    uid: &str,
    category: &str,
    dim_status: &str,
    items: &[Record],
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for r in items {
        sqlx::query(
            "INSERT OR IGNORE INTO records
                    (uid, category, dim_status, subject_id, title, alt_title, my_rating,
                     date, comment, subject_url, cover, comment_id, intro)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uid)
        .bind(category)
        .bind(dim_status)
        .bind(&r.subject_id)
        .bind(&r.title)
        .bind(&r.alt_title)
        .bind(r.my_rating)
        .bind(&r.date)
        .bind(&r.comment)
        .bind(&r.subject_url)
        .bind(&r.cover)
        .bind(&r.comment_id)
        .bind(&r.intro)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// refresh 语义：清空该维度旧数据并重置为待全量重抓。
/// 手动 refresh 任务与订阅全量同步共用（调用前提：维度行已存在且非 queued/running，
/// 避免与进行中的分页抓取竞态删数据）。
async fn refresh_dimension(
    pool: &SqlitePool,
    uid: &str,
    category: &str,
    dim_status: &str,
    now: i64,
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM records WHERE uid=? AND category=? AND dim_status=?")
        .bind(uid)
        .bind(category)
        .bind(dim_status)
        .execute(pool)
        .await?;
    sqlx::query(
        "UPDATE dimensions SET state='queued', fetched_count=0,
                total=NULL, next_start=0, fetched_ms=NULL, attempts=0,
                next_sync_ms=NULL, last_error=NULL, updated_ms=?
         WHERE uid=? AND category=? AND dim_status=?",
    )
    .bind(now)
    .bind(uid)
    .bind(category)
    .bind(dim_status)
    .execute(pool)
    .await?;
    Ok(())
}

// --------------------------------------------------------------------------- //
// 用户资料（收藏页顶部资料块随抓取顺带入库）
// --------------------------------------------------------------------------- //

/// 抓取 worker 每页顺带调用：资料块解析成功即 upsert，
/// 空字段不覆盖已有值（两种页面结构渲染细节不同，避免互相清空）。
pub async fn upsert_user_profile(
    pool: &SqlitePool,
    uid: &str,
    profile: &crate::crawler::UserProfile,
) -> anyhow::Result<()> {
    if profile.username.is_empty() && profile.avatar.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO users (uid, username, avatar, updated_ms)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(uid) DO UPDATE SET
            username = CASE WHEN excluded.username != '' THEN excluded.username ELSE users.username END,
            avatar   = CASE WHEN excluded.avatar != '' THEN excluded.avatar ELSE users.avatar END,
            updated_ms = excluded.updated_ms",
    )
    .bind(uid)
    .bind(&profile.username)
    .bind(&profile.avatar)
    .bind(ServerConfig::now_ms())
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Serialize)]
pub struct UserProfileView {
    pub uid: String,
    pub username: String,
    /// 已改写为本地封面代理端点（豆瓣头像域有 Referer 防盗链，浏览器直连 418）
    pub avatar: String,
}

pub async fn get_user_profile(state: &AppState, uid: &str) -> AppResult<UserProfileView> {
    let row = sqlx::query("SELECT username, avatar FROM users WHERE uid = ?")
        .bind(uid)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::UserNotFound(uid.to_string()))?;
    Ok(UserProfileView {
        uid: uid.to_string(),
        username: row.get("username"),
        avatar: cover_proxy_url(&row.get::<String, _>("avatar")),
    })
}

// --------------------------------------------------------------------------- //
// 视图模型
// --------------------------------------------------------------------------- //

#[derive(Serialize)]
pub struct TaskView {
    pub task_id: String,
    pub user: String,
    pub status: String,
    pub request: Value,
    pub progress: Vec<ProgressEntry>,
    pub items_count: i64,
    pub error: Option<Value>,
    /// 任务总进度百分比（0-100）；completed 恒为 100，未终态封顶 99，无法判定为 null
    pub percent: Option<u32>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub expires_at: Option<String>,
    pub result_url: String,
}

#[derive(Serialize)]
pub struct ProgressEntry {
    pub category: String,
    pub status: String,
    pub state: String,
    pub fetched: i64,
    pub total: Option<i64>,
    /// 维度进度百分比（0-100）；done 恒为 100，error/未知总数为 null，进行中封顶 99
    pub percent: Option<u32>,
    /// 下一个待抓分页偏移（= 已抓页数 × 15），queued 尚未开始时为 0
    pub next_start: i64,
    /// 正在抓第几页（从 1 起，0 表示尚未开始）；pages_total 已知时可组合显示"第 X/Y 页"
    pub page: i64,
    /// 总页数（由 total 向上取整；总数未知为 null）
    pub pages_total: Option<i64>,
}

/// 维度百分比：done 恒 100；未知总数或 total=0 且未完成为 null；进行中封顶 99
fn dim_percent(state: &str, fetched: i64, total: Option<i64>) -> Option<u32> {
    match state {
        "done" => Some(100),
        "error" => None,
        _ => {
            let t = total?;
            if t <= 0 {
                return None;
            }
            let pct = (fetched.max(0) as f64 / t as f64 * 100.0).floor() as u32;
            Some(pct.min(99))
        }
    }
}

/// 任务总百分比：completed 恒 100；否则按已知 total 的维度求和计算，封顶 99。
/// 有维度尚未知 total 时其贡献不计入分母，因此结果偏乐观，封顶规则保证不误报完成。
fn overall_percent(status: &str, entries: &[(i64, Option<i64>)]) -> Option<u32> {
    if status == "completed" {
        return Some(100);
    }
    let mut fetched_sum = 0i64;
    let mut total_sum = 0i64;
    for (fetched, total) in entries {
        if let Some(t) = total {
            fetched_sum += *fetched;
            total_sum += t;
        }
    }
    if total_sum <= 0 {
        return None;
    }
    let pct = (fetched_sum as f64 / total_sum as f64 * 100.0).floor() as u32;
    Some(pct.min(99))
}

#[derive(Serialize)]
pub struct TaskSummary {
    pub task_id: String,
    pub user: String,
    pub status: String,
    pub items_count: i64,
    pub created_at: String,
    pub url: String,
    /// manual = 用户手动创建；auto = 订阅全量同步自动创建
    pub source: String,
}

pub(crate) fn ms_to_rfc3339(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

fn new_task_id() -> String {
    let now = ServerConfig::now_ms() as u64;
    let rand6: u32 = rand::thread_rng().gen_range(0x100000..=0xFFFFFF);
    format!("sync_{now:012x}{rand6:06x}")
}

// --------------------------------------------------------------------------- //
// 创建任务
// --------------------------------------------------------------------------- //

#[derive(Debug, Clone)]
pub struct CreateTaskInput {
    pub user: String,
    pub categories: Vec<Category>,
    pub statuses: Vec<Status>,
    pub refresh: bool,
}

pub struct CreateTaskOutcome {
    pub view: TaskView,
    pub cached_dimensions: usize,
    pub created_dimensions: usize,
}

pub async fn create_task(state: &AppState, input: CreateTaskInput) -> AppResult<CreateTaskOutcome> {
    let uid = parse_uid(&input.user)
        .ok_or_else(|| AppError::InvalidArgument(format!("非法豆瓣用户 ID: {:?}", input.user)))?;

    // 去重并保持顺序
    let mut pairs: Vec<(Category, Status)> = Vec::new();
    for &c in &input.categories {
        for &s in &input.statuses {
            if !pairs.contains(&(c, s)) {
                pairs.push((c, s));
            }
        }
    }

    let now = ServerConfig::now_ms();
    let ttl_ms = state.config.cache_ttl_secs * 1000;

    // 队列上限检查
    let active: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM dimensions WHERE state IN ('queued','running')")
            .fetch_one(&state.pool)
            .await?;
    let active = active + pairs.len() as i64;
    if active > state.config.queue_limit {
        return Err(AppError::TooManyTasks);
    }

    let task_id = new_task_id();
    let request_json = serde_json::to_string(&json!({
        "categories": input.categories.iter().map(|c| c.key()).collect::<Vec<_>>(),
        "statuses": input.statuses.iter().map(|s| s.key()).collect::<Vec<_>>(),
        "refresh": input.refresh,
    }))
    .map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query(
        "INSERT INTO tasks (task_id, user, request_json, status, created_ms)
         VALUES (?, ?, ?, 'queued', ?)",
    )
    .bind(&task_id)
    .bind(&uid)
    .bind(&request_json)
    .bind(now)
    .execute(&state.pool)
    .await?;

    let mut cached = 0usize;
    let mut created = 0usize;
    let mut need_wake = false;

    for (cat, st) in pairs {
        let dim: Option<(String, i64, Option<i64>)> = sqlx::query_as(
            "SELECT state, fetched_count, fetched_ms FROM dimensions
             WHERE uid = ? AND category = ? AND dim_status = ?",
        )
        .bind(&uid)
        .bind(cat.key())
        .bind(st.key())
        .fetch_optional(&state.pool)
        .await?;

        async fn insert_task_dim(
            pool: &SqlitePool,
            task_id: &str,
            uid: &str,
            cat_key: &str,
            st_key: &str,
            sql_state: &str,
            items: i64,
        ) -> AppResult<()> {
            sqlx::query(
                "INSERT INTO task_dims (task_id, uid, category, dim_status, state, items)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(task_id)
            .bind(uid)
            .bind(cat_key)
            .bind(st_key)
            .bind(sql_state)
            .bind(items)
            .execute(pool)
            .await?;
            Ok(())
        }

        match dim {
            // 缓存命中
            Some((state_str, fetched_count, fetched_ms))
                if !input.refresh
                    && state_str == "ok"
                    && fetched_ms.is_some_and(|t| t + ttl_ms > now) =>
            {
                insert_task_dim(
                    &state.pool,
                    &task_id,
                    &uid,
                    cat.key(),
                    st.key(),
                    "done",
                    fetched_count,
                )
                .await?;
                cached += 1;
            }
            // single-flight：已有运行中/排队的抓取，并入
            Some((state_str, _, _)) if state_str == "queued" || state_str == "running" => {
                insert_task_dim(
                    &state.pool,
                    &task_id,
                    &uid,
                    cat.key(),
                    st.key(),
                    "waiting",
                    0,
                )
                .await?;
                created += 1;
            }
            // 需要新抓取（idle / 过期缓存 / refresh / 上次失败）
            dim_state => {
                let is_fresh_ok = dim_state.as_ref().is_some_and(|(s, _, _)| s == "ok");
                if is_fresh_ok {
                    // refresh：清空旧数据重新全量抓
                    refresh_dimension(&state.pool, &uid, cat.key(), st.key(), now).await?;
                } else {
                    // idle/error：清错误与计数后入队，保留已有 records/next_start 续爬
                    sqlx::query(
                        "INSERT INTO dimensions (uid, category, dim_status, state, updated_ms)
                         VALUES (?, ?, ?, 'queued', ?)
                         ON CONFLICT(uid, category, dim_status) DO UPDATE SET
                            state='queued', attempts=0, next_sync_ms=NULL, last_error=NULL,
                            updated_ms=excluded.updated_ms",
                    )
                    .bind(&uid)
                    .bind(cat.key())
                    .bind(st.key())
                    .bind(now)
                    .execute(&state.pool)
                    .await?;
                }
                insert_task_dim(
                    &state.pool,
                    &task_id,
                    &uid,
                    cat.key(),
                    st.key(),
                    "waiting",
                    0,
                )
                .await?;
                created += 1;
                need_wake = true;
            }
        }
    }

    if need_wake {
        state.wake_worker();
    }

    // 全部维度命中缓存 → 直接终态
    let waiting: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM task_dims WHERE task_id = ? AND state != 'done'")
            .bind(&task_id)
            .fetch_one(&state.pool)
            .await?;
    if waiting == 0 {
        finish_task(
            &state.pool,
            &task_id,
            "completed",
            now,
            task_retention_ms(&state.config),
        )
        .await?;
    }

    let view = get_task(state, &task_id).await?;
    Ok(CreateTaskOutcome {
        view,
        cached_dimensions: cached,
        created_dimensions: created,
    })
}

// --------------------------------------------------------------------------- //
// 查询
// --------------------------------------------------------------------------- //

pub async fn get_task(state: &AppState, task_id: &str) -> AppResult<TaskView> {
    let task = sqlx::query(
        "SELECT task_id, user, request_json, status, items_count, error_json,
                created_ms, started_ms, finished_ms, expires_ms
         FROM tasks WHERE task_id = ?",
    )
    .bind(task_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::TaskNotFound(task_id.to_string()))?;
    let row = task;
    let status: String = row.get("status");

    let progress_rows = sqlx::query(
        "SELECT td.category, td.dim_status, td.state AS td_state, td.items,
                d.state AS dim_state, d.fetched_count, d.total, d.next_start
         FROM task_dims td
         LEFT JOIN dimensions d
           ON d.uid = td.uid AND d.category = td.category AND d.dim_status = td.dim_status
         WHERE td.task_id = ?
         ORDER BY td.rowid",
    )
    .bind(task_id)
    .fetch_all(&state.pool)
    .await?;

    let progress: Vec<ProgressEntry> = progress_rows
        .into_iter()
        .map(|r| {
            let td_state: String = r.get("td_state");
            let dim_state: Option<String> = r.get("dim_state");
            let state = match td_state.as_str() {
                "done" => "done".to_string(),
                "error" => "error".to_string(),
                _ => dim_state.unwrap_or_else(|| "queued".to_string()), // waiting → 维度实际状态
            };
            let fetched = r.get::<Option<i64>, _>("fetched_count").unwrap_or(0);
            let total: Option<i64> = r.get("total");
            let percent = dim_percent(&state, fetched, total);
            let next_start = r.get::<Option<i64>, _>("next_start").unwrap_or(0).max(0);
            let page_size = crate::crawler::PAGE_SIZE as i64;
            let page = next_start / page_size;
            let pages_total = total.map(|t| (t + page_size - 1) / page_size);
            ProgressEntry {
                category: r.get("category"),
                status: r.get("dim_status"),
                state,
                fetched,
                total,
                percent,
                next_start,
                page,
                pages_total,
            }
        })
        .collect();
    let percent = overall_percent(
        &status,
        &progress
            .iter()
            .map(|p| (p.fetched, p.total))
            .collect::<Vec<_>>(),
    );

    let error_json: Option<String> = row.get("error_json");
    Ok(TaskView {
        task_id: row.get("task_id"),
        user: row.get("user"),
        status,
        request: serde_json::from_str(&row.get::<String, _>("request_json")).unwrap_or(json!({})),
        progress,
        items_count: row.get("items_count"),
        error: error_json.and_then(|s| serde_json::from_str(&s).ok()),
        percent,
        created_at: ms_to_rfc3339(row.get("created_ms")),
        started_at: row.get::<Option<i64>, _>("started_ms").map(ms_to_rfc3339),
        finished_at: row.get::<Option<i64>, _>("finished_ms").map(ms_to_rfc3339),
        expires_at: row.get::<Option<i64>, _>("expires_ms").map(ms_to_rfc3339),
        result_url: format!("/api/v1/syncs/{task_id}/result"),
    })
}

pub struct ListFilter {
    pub user: Option<String>,
    pub status: Option<String>,
    /// None = 全部；Some("manual")/Some("auto") = 按来源过滤（默认列表只看手动任务）
    pub source: Option<String>,
    pub limit: i64,
    pub cursor_rowid: Option<i64>,
}

pub async fn list_tasks(
    state: &AppState,
    f: ListFilter,
) -> AppResult<(Vec<TaskSummary>, Option<i64>)> {
    let rows = sqlx::query(
        "SELECT rowid, task_id, user, status, items_count, created_ms, source FROM tasks
         WHERE (? IS NULL OR user = ?)
           AND (? IS NULL OR status = ?)
           AND (? IS NULL OR source = ?)
           AND (? IS NULL OR rowid < ?)
         ORDER BY rowid DESC
         LIMIT ?",
    )
    .bind(&f.user)
    .bind(&f.user)
    .bind(&f.status)
    .bind(&f.status)
    .bind(&f.source)
    .bind(&f.source)
    .bind(f.cursor_rowid)
    .bind(f.cursor_rowid)
    .bind(f.limit + 1)
    .fetch_all(&state.pool)
    .await?;

    let has_more = rows.len() as i64 > f.limit;
    let mut next_cursor = None;
    let mut summaries = Vec::with_capacity(rows.len().saturating_sub(1));
    for r in rows {
        let rowid: i64 = r.get("rowid");
        if summaries.len() as i64 >= f.limit {
            break;
        }
        let task_id: String = r.get("task_id");
        summaries.push(TaskSummary {
            url: format!("/api/v1/syncs/{task_id}"),
            task_id,
            user: r.get("user"),
            status: r.get("status"),
            items_count: r.get("items_count"),
            created_at: ms_to_rfc3339(r.get("created_ms")),
            source: r.get("source"),
        });
        next_cursor = Some(rowid);
    }
    if !has_more {
        next_cursor = None;
    }
    Ok((summaries, next_cursor))
}

pub async fn delete_task(state: &AppState, task_id: &str) -> AppResult<()> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM tasks WHERE task_id = ?")
        .bind(task_id)
        .fetch_optional(&state.pool)
        .await?
        .flatten();
    if exists.is_none() {
        return Err(AppError::TaskNotFound(task_id.to_string()));
    }
    // 只删任务记录；records 表的维度数据与任务生命周期解耦，长期保留
    sqlx::query("DELETE FROM tasks WHERE task_id = ?")
        .bind(task_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

// --------------------------------------------------------------------------- //
// 任务结果（按维度汇总，数据本体经 /users/{uid}/marks 浏览）
// --------------------------------------------------------------------------- //

#[derive(Serialize)]
pub struct ResultDimView {
    pub category: String,
    pub status: String,
    pub items: i64,
    pub with_comment: i64,
}

#[derive(Serialize)]
pub struct ResultView {
    pub task_id: String,
    pub user: String,
    pub summary: Value,
    pub dimensions: Vec<ResultDimView>,
    pub expires_at: Option<String>,
}

pub async fn task_result(state: &AppState, task_id: &str) -> AppResult<ResultView> {
    let task = sqlx::query("SELECT user, status, expires_ms FROM tasks WHERE task_id = ?")
        .bind(task_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::TaskNotFound(task_id.to_string()))?;
    let status: String = task.get("status");
    if !TERMINAL_STATUSES.contains(&status.as_str()) {
        return Err(AppError::TaskNotFinished(status));
    }
    let user: String = task.get("user");

    // 汇总（含短评数：读 records 表统计）
    let dim_rows = sqlx::query(
        "SELECT category, dim_status, items FROM task_dims
         WHERE task_id = ? AND state = 'done' ORDER BY rowid",
    )
    .bind(task_id)
    .fetch_all(&state.pool)
    .await?;

    let mut total = 0i64;
    let mut with_comment = 0i64;
    let mut by: serde_json::Map<String, Value> = serde_json::Map::new();
    let mut dimensions = Vec::with_capacity(dim_rows.len());
    for r in dim_rows {
        let category: String = r.get("category");
        let dim_status: String = r.get("dim_status");
        let items: i64 = r.get("items");
        let dim_comments: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM records
             WHERE uid = ? AND category = ? AND dim_status = ? AND comment != ''",
        )
        .bind(&user)
        .bind(&category)
        .bind(&dim_status)
        .fetch_one(&state.pool)
        .await?;
        total += items;
        with_comment += dim_comments;
        let label = Category::from_key(&category)
            .map(|c| format!("{}·{}", c.key(), c.status_label(status_of(&dim_status))))
            .unwrap_or_else(|| format!("{category}·{dim_status}"));
        by.insert(label, json!(items));
        dimensions.push(ResultDimView {
            category,
            status: dim_status,
            items,
            with_comment: dim_comments,
        });
    }

    // 全局富集进度（条目元信息跨用户共享，不按任务归属，仅附带给调用方观察）
    let counts = super::subject::enrichment_counts(&state.pool).await?;
    let (subjects_enriched, subjects_pending) = (counts.enriched, counts.pending);
    Ok(ResultView {
        task_id: task_id.to_string(),
        user,
        summary: json!({
            "total": total,
            "with_comment": with_comment,
            "by_category_status": by,
            "subjects_enriched": subjects_enriched,
            "subjects_pending": subjects_pending,
        }),
        dimensions,
        expires_at: task
            .try_get::<Option<i64>, _>("expires_ms")
            .ok()
            .flatten()
            .map(ms_to_rfc3339),
    })
}

fn status_of(key: &str) -> Status {
    Status::from_key(key).unwrap_or(Status::Collect)
}

// --------------------------------------------------------------------------- //
// 缓存直读
// --------------------------------------------------------------------------- //

pub struct MarksPage {
    pub user: String,
    pub category: Category,
    pub status: Status,
    pub cached_at: String,
    pub fresh: bool,
    pub total: usize,
    pub page: i64,
    pub page_size: i64,
    pub items: Vec<Record>,
}

/// 维度缓存检查：行缺失或非 ok → DimensionNotCached；返回 fetched_ms
pub(super) async fn ensure_dimension_cached(
    state: &AppState,
    uid: &str,
    category: Category,
    status: Status,
) -> AppResult<Option<i64>> {
    let row = sqlx::query(
        "SELECT state, fetched_ms FROM dimensions
         WHERE uid = ? AND category = ? AND dim_status = ?",
    )
    .bind(uid)
    .bind(category.key())
    .bind(status.key())
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Err(AppError::DimensionNotCached);
    };
    if row.get::<String, _>("state") != "ok" {
        return Err(AppError::DimensionNotCached);
    }
    Ok(row.get("fetched_ms"))
}

pub async fn read_marks(
    state: &AppState,
    uid: &str,
    category: Category,
    status: Status,
    page: i64,
    page_size: i64,
) -> AppResult<MarksPage> {
    let fetched_ms = ensure_dimension_cached(state, uid, category, status).await?;

    let page_size = page_size.max(1);
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM records WHERE uid = ? AND category = ? AND dim_status = ?",
    )
    .bind(uid)
    .bind(category.key())
    .bind(status.key())
    .fetch_one(&state.pool)
    .await?;
    let offset = (page - 1).max(0) * page_size;
    let rows = sqlx::query(
        "SELECT subject_id, title, alt_title, my_rating, date, comment,
                subject_url, cover, comment_id, intro
         FROM records
         WHERE uid = ? AND category = ? AND dim_status = ?
         ORDER BY date DESC, rowid
         LIMIT ? OFFSET ?",
    )
    .bind(uid)
    .bind(category.key())
    .bind(status.key())
    .bind(page_size)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let items: Vec<Record> = rows
        .into_iter()
        .map(|r| Record {
            category: category.key().to_string(),
            status: status.key().to_string(),
            title: r.get("title"),
            alt_title: r.get("alt_title"),
            my_rating: r.get::<Option<i64>, _>("my_rating").map(|v| v as u8),
            date: r.get("date"),
            comment: r.get("comment"),
            subject_url: r.get("subject_url"),
            subject_id: r.get("subject_id"),
            // 豆瓣 CDN 子域名 Referer 防盗链不一致，浏览器直连 img2/img3 会 403，
            // 改写为本地代理端点（cache-then-serve），见 covers 模块
            cover: cover_proxy_url(&r.get::<String, _>("cover")),
            comment_id: r.get("comment_id"),
            intro: r.get("intro"),
        })
        .collect();
    let total = total as usize;

    Ok(MarksPage {
        user: uid.to_string(),
        category,
        status,
        cached_at: fetched_ms.map(ms_to_rfc3339).unwrap_or_default(),
        fresh: fetched_ms
            .map(|t| t + state.config.cache_ttl_secs * 1000 > ServerConfig::now_ms())
            .unwrap_or(false),
        total,
        page,
        page_size,
        items,
    })
}

// --------------------------------------------------------------------------- //
// 跨类别合并读（category=all，书影音全部标记）
// --------------------------------------------------------------------------- //

pub struct AllMarksPage {
    pub user: String,
    pub status: Status,
    pub cached_at: String,
    pub fresh: bool,
    pub total: usize,
    pub page: i64,
    pub page_size: i64,
    pub items: Vec<Record>,
}

/// 三类别合并分页读：维度门禁放行口径比单类别宽——任一类别已同步即
/// 返回（已同步部分的记录），三类别全未同步才 DIMENSION_NOT_CACHED；
/// cached_at/fresh 取三维度最近一次抓取。排序与单类别一致
/// （date DESC, rowid），跨类别按日期自然归并；Record.category 用行
/// 自身类别（expand=subject 时路由层按它分组查视图）。
/// rating 为我的评分筛选（1-5 星，路由层已校验；None=不过滤），
/// 计数与列表同口径加 `my_rating = ?`
pub async fn read_all_marks(
    state: &AppState,
    uid: &str,
    status: Status,
    rating: Option<i64>,
    page: i64,
    page_size: i64,
) -> AppResult<AllMarksPage> {
    let mut any_cached = false;
    let mut latest_fetched: Option<i64> = None;
    for category in Category::ALL {
        match ensure_dimension_cached(state, uid, category, status).await {
            Ok(fetched_ms) => {
                any_cached = true;
                if let Some(ms) = fetched_ms {
                    if ms > latest_fetched.unwrap_or(i64::MIN) {
                        latest_fetched = Some(ms);
                    }
                }
            }
            Err(AppError::DimensionNotCached) => {}
            Err(other) => return Err(other),
        }
    }
    if !any_cached {
        return Err(AppError::DimensionNotCached);
    }

    let page_size = page_size.max(1);
    // 评分筛选子句拼进两条 SQL（与 ids 模式的 placeholders 拼装同款）：
    // 值为路由层校验过的 1-5 整数，内联无注入面
    let rating_clause = rating
        .map(|r| format!(" AND my_rating = {r}"))
        .unwrap_or_default();
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM records WHERE uid = ? AND dim_status = ?{rating_clause}"
    ))
    .bind(uid)
    .bind(status.key())
    .fetch_one(&state.pool)
    .await?;
    let offset = (page - 1).max(0) * page_size;
    let rows = sqlx::query(&format!(
        "SELECT category, subject_id, title, alt_title, my_rating, date, comment,
                subject_url, cover, comment_id, intro
         FROM records
         WHERE uid = ? AND dim_status = ?{rating_clause}
         ORDER BY date DESC, rowid
         LIMIT ? OFFSET ?"
    ))
    .bind(uid)
    .bind(status.key())
    .bind(page_size)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let items: Vec<Record> = rows
        .into_iter()
        .map(|r| Record {
            category: r.get("category"),
            status: status.key().to_string(),
            title: r.get("title"),
            alt_title: r.get("alt_title"),
            my_rating: r.get::<Option<i64>, _>("my_rating").map(|v| v as u8),
            date: r.get("date"),
            comment: r.get("comment"),
            subject_url: r.get("subject_url"),
            subject_id: r.get("subject_id"),
            cover: cover_proxy_url(&r.get::<String, _>("cover")),
            comment_id: r.get("comment_id"),
            intro: r.get("intro"),
        })
        .collect();

    Ok(AllMarksPage {
        user: uid.to_string(),
        status,
        cached_at: latest_fetched.map(ms_to_rfc3339).unwrap_or_default(),
        fresh: latest_fetched
            .map(|t| t + state.config.cache_ttl_secs * 1000 > ServerConfig::now_ms())
            .unwrap_or(false),
        total: total as usize,
        page,
        page_size,
        items,
    })
}

// --------------------------------------------------------------------------- //
// 订阅同步：RSS 短周期增量 + 长周期全量（auto 任务）
// --------------------------------------------------------------------------- //

#[derive(Serialize)]
pub struct SubscriptionView {
    pub uid: String,
    pub enabled: bool,
    pub rss_interval_secs: i64,
    pub full_interval_secs: i64,
    pub next_rss_at: Option<String>,
    pub next_full_at: Option<String>,
    pub last_rss_at: Option<String>,
    pub last_full_at: Option<String>,
    pub last_error: Option<String>,
    /// 该 uid 已缓存的维度数与记录数
    pub dimensions: i64,
    pub records: i64,
    pub created_at: String,
}

/// 间隔下限：RSS 轮询不快于 1 分钟；全量不快于 1 小时且不短于 RSS 间隔
const MIN_RSS_INTERVAL: i64 = 60;
const MIN_FULL_INTERVAL: i64 = 3_600;
/// full 因队列满 / single-flight 顺延时的重试间隔
const FULL_RETRY_SECS: i64 = 300;

pub async fn upsert_subscription(
    state: &AppState,
    uid: &str,
    rss_interval_secs: Option<i64>,
    full_interval_secs: Option<i64>,
) -> AppResult<SubscriptionView> {
    if !crate::crawler::is_valid_id(uid) {
        return Err(AppError::InvalidArgument(format!(
            "非法豆瓣用户 ID: {uid:?}"
        )));
    }
    let rss = rss_interval_secs.unwrap_or(state.config.sub_rss_interval_secs);
    let full = full_interval_secs.unwrap_or(state.config.sub_full_interval_secs);
    if rss < MIN_RSS_INTERVAL || full < MIN_FULL_INTERVAL || full < rss {
        return Err(AppError::InvalidArgument(format!(
            "间隔不合法：需 rss_interval_secs ≥ {MIN_RSS_INTERVAL}、\
             full_interval_secs ≥ {MIN_FULL_INTERVAL} 且 full ≥ rss"
        )));
    }

    let now = ServerConfig::now_ms();
    // 重复订阅只改间隔并重新启用，保持既有节奏（next_*_ms 不重置）
    sqlx::query(
        "INSERT INTO subscriptions (uid, rss_interval_secs, full_interval_secs,
                next_rss_ms, next_full_ms, created_ms, updated_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(uid) DO UPDATE SET
            rss_interval_secs=excluded.rss_interval_secs,
            full_interval_secs=excluded.full_interval_secs,
            enabled=1, updated_ms=excluded.updated_ms",
    )
    .bind(uid)
    .bind(rss)
    .bind(full)
    .bind(now + rss * 1000)
    .bind(now + full * 1000)
    .bind(now)
    .bind(now)
    .execute(&state.pool)
    .await?;
    get_subscription(state, uid).await
}

pub async fn delete_subscription(state: &AppState, uid: &str) -> AppResult<()> {
    let res = sqlx::query("DELETE FROM subscriptions WHERE uid = ?")
        .bind(uid)
        .execute(&state.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::SubscriptionNotFound(uid.to_string()));
    }
    Ok(())
}

pub async fn get_subscription(state: &AppState, uid: &str) -> AppResult<SubscriptionView> {
    sqlx::query(
        "SELECT uid, enabled, rss_interval_secs, full_interval_secs,
                next_rss_ms, next_full_ms, last_rss_ms, last_full_ms, last_error, created_ms,
                (SELECT COUNT(*) FROM dimensions d WHERE d.uid = s.uid) AS dim_count,
                (SELECT COUNT(*) FROM records r WHERE r.uid = s.uid) AS rec_count
         FROM subscriptions s WHERE uid = ?",
    )
    .bind(uid)
    .fetch_optional(&state.pool)
    .await?
    .map(sub_row_to_view)
    .ok_or_else(|| AppError::SubscriptionNotFound(uid.to_string()))
}

pub async fn list_subscriptions(state: &AppState) -> AppResult<Vec<SubscriptionView>> {
    let rows = sqlx::query(
        "SELECT uid, enabled, rss_interval_secs, full_interval_secs,
                next_rss_ms, next_full_ms, last_rss_ms, last_full_ms, last_error, created_ms,
                (SELECT COUNT(*) FROM dimensions d WHERE d.uid = s.uid) AS dim_count,
                (SELECT COUNT(*) FROM records r WHERE r.uid = s.uid) AS rec_count
         FROM subscriptions s ORDER BY s.created_ms",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(rows.into_iter().map(sub_row_to_view).collect())
}

fn sub_row_to_view(r: sqlx::sqlite::SqliteRow) -> SubscriptionView {
    SubscriptionView {
        uid: r.get("uid"),
        enabled: r.get::<i64, _>("enabled") != 0,
        rss_interval_secs: r.get("rss_interval_secs"),
        full_interval_secs: r.get("full_interval_secs"),
        next_rss_at: r.get::<Option<i64>, _>("next_rss_ms").map(ms_to_rfc3339),
        next_full_at: r.get::<Option<i64>, _>("next_full_ms").map(ms_to_rfc3339),
        last_rss_at: r.get::<Option<i64>, _>("last_rss_ms").map(ms_to_rfc3339),
        last_full_at: r.get::<Option<i64>, _>("last_full_ms").map(ms_to_rfc3339),
        last_error: r.get("last_error"),
        dimensions: r.get("dim_count"),
        records: r.get("rec_count"),
        created_at: ms_to_rfc3339(r.get("created_ms")),
    }
}

struct DueSubscription {
    uid: String,
    rss_interval_secs: i64,
    full_interval_secs: i64,
    next_rss_ms: i64,
    rss_attempts: i64,
}

/// 处理一个到期订阅（rss / full 谁先到期先做谁）。返回是否处理了订阅；
/// 供调度循环与测试复用（与 worker::process_next_due 同构）。
pub async fn subscription_tick<F: DoubanFetcher>(
    state: &AppState,
    fetcher: &F,
) -> anyhow::Result<bool> {
    let row = sqlx::query_as::<_, (String, i64, i64, i64, i64)>(
        "SELECT uid, rss_interval_secs, full_interval_secs, next_rss_ms, rss_attempts
         FROM subscriptions
         WHERE enabled = 1 AND (next_rss_ms <= ? OR next_full_ms <= ?)
         ORDER BY updated_ms ASC, rowid ASC
         LIMIT 1",
    )
    .bind(ServerConfig::now_ms())
    .bind(ServerConfig::now_ms())
    .fetch_optional(&state.pool)
    .await?;
    let Some((uid, rss_interval_secs, full_interval_secs, next_rss_ms, rss_attempts)) = row else {
        return Ok(false);
    };
    let sub = DueSubscription {
        uid,
        rss_interval_secs,
        full_interval_secs,
        next_rss_ms,
        rss_attempts,
    };
    let now = ServerConfig::now_ms();
    if sub.next_rss_ms <= now {
        rss_sync(state, fetcher, &sub, now).await?;
    } else {
        full_sync(state, &sub, now).await?;
    }
    Ok(true)
}

/// RSS 增量：feed 条目按维度分组写入 records——主键 (uid, cat, st, subject_id)
/// 冲突即已入库，INSERT OR IGNORE 天然去重，无需显式 guid diff。
/// 只写本地已有维度：订阅只维护既有维度，新维度由用户手动建任务引入。
async fn rss_sync<F: DoubanFetcher>(
    state: &AppState,
    fetcher: &F,
    sub: &DueSubscription,
    now: i64,
) -> anyhow::Result<()> {
    match fetcher.fetch_feed(&sub.uid).await {
        Ok(xml) => {
            let items = crate::crawler::parse_feed(&xml);
            let dims: Vec<(String, String)> =
                sqlx::query_as("SELECT category, dim_status FROM dimensions WHERE uid = ?")
                    .bind(&sub.uid)
                    .fetch_all(&state.pool)
                    .await?;
            let dim_set: std::collections::HashSet<(String, String)> = dims.into_iter().collect();

            let mut by_dim: std::collections::HashMap<(String, String), Vec<Record>> =
                Default::default();
            let mut skipped = 0i64;
            let newest_guid = items.first().map(|i| i.guid.clone()).unwrap_or_default();
            let feed_count = items.len();
            for it in items {
                let key = (it.record.category.clone(), it.status_key);
                if dim_set.contains(&key) {
                    by_dim.entry(key).or_default().push(it.record);
                } else {
                    skipped += 1;
                }
            }
            for ((cat, st), recs) in &by_dim {
                upsert_records(&state.pool, &sub.uid, cat, st, recs).await?;
                // 重算 fetched_count 保持进度视图一致（total 由下次全量校正）
                sqlx::query(
                    "UPDATE dimensions SET fetched_count =
                            (SELECT COUNT(*) FROM records
                             WHERE uid=? AND category=? AND dim_status=?), updated_ms=?
                     WHERE uid=? AND category=? AND dim_status=?",
                )
                .bind(&sub.uid)
                .bind(cat)
                .bind(st)
                .bind(now)
                .bind(&sub.uid)
                .bind(cat)
                .bind(st)
                .execute(&state.pool)
                .await?;
            }

            sqlx::query(
                "UPDATE subscriptions SET last_rss_ms=?, next_rss_ms=?, last_guid=?,
                        rss_attempts=0, last_error=NULL, updated_ms=?
                 WHERE uid=?",
            )
            .bind(now)
            .bind(now + sub.rss_interval_secs * 1000)
            .bind(newest_guid)
            .bind(now)
            .bind(&sub.uid)
            .execute(&state.pool)
            .await?;
            tracing::info!(
                uid = %sub.uid,
                feed_items = feed_count,
                dimensions_touched = by_dim.len(),
                skipped_unknown_dim = skipped,
                "RSS 增量同步"
            );
        }
        Err(e) => {
            // 失败退避（1/2/…分钟，封顶 15 分钟）：RSS 是增强层，连续失败不熔断主流程
            let attempts = sub.rss_attempts + 1;
            let backoff_ms = (attempts * 60_000).min(900_000);
            sqlx::query(
                "UPDATE subscriptions SET rss_attempts=?, next_rss_ms=?, last_error=?, updated_ms=?
                 WHERE uid=?",
            )
            .bind(attempts)
            .bind(now + backoff_ms)
            .bind(format!("{}: {e}", e.code()))
            .bind(now)
            .bind(&sub.uid)
            .execute(&state.pool)
            .await?;
            tracing::warn!(
                uid = %sub.uid,
                attempt = attempts,
                error = %e,
                "RSS 增量抓取失败，退避重试"
            );
        }
    }
    Ok(())
}

/// 长周期全量：为该 uid 创建 source='auto' 的任务并对其全部已有维度执行 refresh。
/// 只刷 state 为 ok/error 的维度——正在抓取中的维度 DELETE records 会与其竞态，跳过
/// 后下次周期自然补上。
async fn full_sync(state: &AppState, sub: &DueSubscription, now: i64) -> anyhow::Result<()> {
    let postpone = |secs: i64| {
        sqlx::query("UPDATE subscriptions SET next_full_ms=?, updated_ms=? WHERE uid=?")
            .bind(now + secs * 1000)
            .bind(now)
            .bind(&sub.uid)
            .execute(&state.pool)
    };

    let dims: Vec<(String, String)> = sqlx::query_as(
        "SELECT category, dim_status FROM dimensions WHERE uid = ? AND state IN ('ok','error')",
    )
    .bind(&sub.uid)
    .fetch_all(&state.pool)
    .await?;

    if dims.is_empty() {
        // 从未抓取过（或全部进行中）：无维度可刷，顺延一个完整周期
        postpone(sub.full_interval_secs).await?;
        return Ok(());
    }
    let active: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM dimensions WHERE state IN ('queued','running')")
            .fetch_one(&state.pool)
            .await?;
    if active + dims.len() as i64 > state.config.queue_limit {
        tracing::warn!(uid = %sub.uid, "订阅全量因队列上限顺延");
        postpone(FULL_RETRY_SECS).await?;
        return Ok(());
    }
    // single-flight：该 uid 已有未终态 auto 任务时不重复创建
    let running_auto: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks
         WHERE user = ? AND source = 'auto' AND status IN ('queued','running')",
    )
    .bind(&sub.uid)
    .fetch_one(&state.pool)
    .await?;
    if running_auto > 0 {
        postpone(FULL_RETRY_SECS).await?;
        return Ok(());
    }

    let task_id = new_task_id();
    let mut categories: Vec<&str> = Vec::new();
    let mut statuses: Vec<&str> = Vec::new();
    for (cat, st) in &dims {
        if !categories.contains(&cat.as_str()) {
            categories.push(cat);
        }
        if !statuses.contains(&st.as_str()) {
            statuses.push(st);
        }
    }
    let request_json = serde_json::to_string(&json!({
        "categories": categories,
        "statuses": statuses,
        "refresh": true,
    }))?;
    sqlx::query(
        "INSERT INTO tasks (task_id, user, request_json, status, created_ms, source)
         VALUES (?, ?, ?, 'queued', ?, 'auto')",
    )
    .bind(&task_id)
    .bind(&sub.uid)
    .bind(&request_json)
    .bind(now)
    .execute(&state.pool)
    .await?;

    for (cat, st) in &dims {
        refresh_dimension(&state.pool, &sub.uid, cat, st, now).await?;
        sqlx::query(
            "INSERT INTO task_dims (task_id, uid, category, dim_status, state, items)
             VALUES (?, ?, ?, ?, 'waiting', 0)",
        )
        .bind(&task_id)
        .bind(&sub.uid)
        .bind(cat)
        .bind(st)
        .execute(&state.pool)
        .await?;
    }
    state.wake_worker();

    sqlx::query(
        "UPDATE subscriptions SET last_full_ms=?, next_full_ms=?, updated_ms=? WHERE uid=?",
    )
    .bind(now)
    .bind(now + sub.full_interval_secs * 1000)
    .bind(now)
    .bind(&sub.uid)
    .execute(&state.pool)
    .await?;
    tracing::info!(
        uid = %sub.uid,
        task_id = %task_id,
        dimensions = dims.len(),
        "订阅全量同步已入队（auto 任务）"
    );
    Ok(())
}

/// 订阅调度循环——与 worker/cleanup 同风格：处理到期订阅，空闲时 30s 兜底扫描
pub async fn run_subscription_loop<F: DoubanFetcher>(state: AppState, fetcher: F) {
    loop {
        match subscription_tick(&state, &fetcher).await {
            Ok(true) => {} // 处理了一个到期订阅，立即取下一个
            Ok(false) => tokio::time::sleep(std::time::Duration::from_secs(30)).await,
            Err(e) => {
                tracing::warn!(error = %e, "subscription tick failed");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

// --------------------------------------------------------------------------- //
// 任务终态与文件生成（worker 与创建路径共用）
// --------------------------------------------------------------------------- //

/// 维度到达终态后，推进所有关联任务。
pub async fn finalize_linked_tasks(
    state: &AppState,
    uid: &str,
    category: &str,
    dim_status: &str,
    ok: bool,
    error_code: &str,
) -> anyhow::Result<()> {
    // 先取出受影响的任务（waiting → done/error）
    let task_ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT task_id FROM task_dims
         WHERE uid = ? AND category = ? AND dim_status = ? AND state = 'waiting'",
    )
    .bind(uid)
    .bind(category)
    .bind(dim_status)
    .fetch_all(&state.pool)
    .await?;

    let items: i64 = if ok {
        sqlx::query_scalar(
            "SELECT fetched_count FROM dimensions
             WHERE uid = ? AND category = ? AND dim_status = ?",
        )
        .bind(uid)
        .bind(category)
        .bind(dim_status)
        .fetch_optional(&state.pool)
        .await?
        .unwrap_or(0)
    } else {
        0
    };

    sqlx::query(
        "UPDATE task_dims SET state = ?, items = ?, error = ?
         WHERE uid = ? AND category = ? AND dim_status = ? AND state = 'waiting'",
    )
    .bind(if ok { "done" } else { "error" })
    .bind(items)
    .bind(if ok {
        None
    } else {
        Some(error_code.to_string())
    })
    .bind(uid)
    .bind(category)
    .bind(dim_status)
    .execute(&state.pool)
    .await?;

    for task_id in task_ids {
        recompute_task_status(state, &task_id).await?;
    }
    Ok(())
}

pub async fn recompute_task_status(state: &AppState, task_id: &str) -> anyhow::Result<()> {
    let rows = sqlx::query(
        "SELECT state, COUNT(*) AS n, COALESCE(SUM(items), 0) AS items
         FROM task_dims WHERE task_id = ? GROUP BY state",
    )
    .bind(task_id)
    .fetch_all(&state.pool)
    .await?;

    let mut done = 0i64;
    let mut error = 0i64;
    let mut waiting = 0i64;
    let mut items = 0i64;
    for r in rows {
        let state_str: String = r.get("state");
        match state_str.as_str() {
            "done" => {
                done += r.get::<i64, _>("n");
                items += r.get::<i64, _>("items");
            }
            "error" => error += r.get::<i64, _>("n"),
            _ => waiting += r.get::<i64, _>("n"),
        }
    }
    if waiting > 0 {
        return Ok(()); // 尚有维度未终态
    }

    let status = if error == 0 {
        "completed"
    } else if done > 0 {
        "partial"
    } else {
        "failed"
    };
    let now = ServerConfig::now_ms();
    finish_task(
        &state.pool,
        task_id,
        status,
        now,
        task_retention_ms(&state.config),
    )
    .await?;

    // 失败信息
    if status != "completed" {
        let failed: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT category, dim_status, COALESCE(error, '') FROM task_dims
             WHERE task_id = ? AND state = 'error'",
        )
        .bind(task_id)
        .fetch_all(&state.pool)
        .await?;
        let first_code = failed
            .first()
            .map(|(_, _, c)| c.clone())
            .unwrap_or_default();
        let error_json = json!({
            "code": if first_code.is_empty() { "INTERNAL" } else { first_code.as_str() },
            "message": "部分或全部维度抓取失败",
            "failed_dimensions": failed
                .iter()
                .map(|(c, s, _)| json!({"category": c, "status": s}))
                .collect::<Vec<_>>(),
        });
        sqlx::query("UPDATE tasks SET items_count = ?, error_json = ? WHERE task_id = ?")
            .bind(items)
            .bind(error_json.to_string())
            .bind(task_id)
            .execute(&state.pool)
            .await?;
    }

    Ok(())
}

async fn finish_task(
    pool: &SqlitePool,
    task_id: &str,
    status: &str,
    now: i64,
    task_retention_ms: i64,
) -> anyhow::Result<()> {
    // 任务记录保留期（数据本体在 records 表，与任务过期无关）
    let expires = now + task_retention_ms;
    sqlx::query(
        "UPDATE tasks SET status = ?, finished_ms = ?, expires_ms = ?,
                items_count = COALESCE((
                    SELECT SUM(items) FROM task_dims WHERE task_id = ? AND state = 'done'
                ), 0)
         WHERE task_id = ?",
    )
    .bind(status)
    .bind(now)
    .bind(expires)
    .bind(task_id)
    .bind(task_id)
    .execute(pool)
    .await?;
    Ok(())
}

fn task_retention_ms(config: &super::config::ServerConfig) -> i64 {
    config.task_retention_days * 24 * 3600 * 1000
}

// --------------------------------------------------------------------------- //
// 过期清理
// --------------------------------------------------------------------------- //

pub async fn run_cleanup_loop(state: AppState) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        if let Err(e) = cleanup_once(&state).await {
            tracing::warn!(error = %e, "cleanup failed");
        }
    }
}

async fn cleanup_once(state: &AppState) -> anyhow::Result<()> {
    let now = ServerConfig::now_ms();
    let retention = task_retention_ms(&state.config);

    // 只清理过期任务记录；records 表数据与任务生命周期解耦，不参与清理
    let expired: Vec<String> = sqlx::query_scalar(
        "SELECT task_id FROM tasks
         WHERE (expires_ms IS NOT NULL AND expires_ms < ?)
            OR (finished_ms IS NOT NULL AND finished_ms + ? < ?)",
    )
    .bind(now)
    .bind(retention)
    .bind(now)
    .fetch_all(&state.pool)
    .await?;

    for task_id in expired {
        sqlx::query("DELETE FROM tasks WHERE task_id = ?")
            .bind(&task_id)
            .execute(&state.pool)
            .await?;
        tracing::info!(task_id = %task_id, "清理过期任务");
    }
    Ok(())
}

// --------------------------------------------------------------------------- //
// 服务元信息
// --------------------------------------------------------------------------- //

pub async fn server_info(state: &AppState) -> AppResult<Value> {
    let mut queued = 0i64;
    let mut running = 0i64;
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT state, COUNT(*) FROM dimensions WHERE state IN ('queued','running') GROUP BY state",
    )
    .fetch_all(&state.pool)
    .await?;
    for (s, n) in rows {
        match s.as_str() {
            "queued" => queued = n,
            "running" => running = n,
            _ => {}
        }
    }
    let circuit_remaining = state.circuit_remaining_ms();
    let sub_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subscriptions")
        .fetch_one(&state.pool)
        .await?;
    let sub_enabled: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM subscriptions WHERE enabled = 1")
            .fetch_one(&state.pool)
            .await?;
    Ok(json!({
        "name": "weimo-douban-server",
        "version": env!("CARGO_PKG_VERSION"),
        "crawl": {
            "min_interval_seconds": state.config.crawl_min_interval_secs,
            "circuit": {
                "threshold": state.config.circuit_threshold,
                "cooldown_seconds": state.config.circuit_cooldown_secs,
                "open": circuit_remaining > 0,
                "remaining_ms": circuit_remaining,
            },
        },
        "cache": {
            "ttl_seconds": state.config.cache_ttl_secs,
            "retention_task_days": state.config.task_retention_days,
        },
        "subscriptions": {"count": sub_count, "enabled": sub_enabled},
        "queue": {"queued": queued, "running": running},
        "auth_enabled": !state.config.token.is_empty(),
        "cookie_enabled": !state.config.douban_cookie.is_empty(),
    }))
}
