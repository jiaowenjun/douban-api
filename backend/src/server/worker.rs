//! 抓取 worker——weimo-ocr image_sync 模式：数据库即队列，单 worker 串行
//! （天然实现豆瓣侧全局限速），watch channel 即时唤醒 + 60s 兜底扫描。

use anyhow::{Context, Result};
use rand::Rng;
use sqlx::{Row, SqlitePool};
use std::time::Duration;
use tracing::{info, warn};

use crate::crawler::{
    self, Category, Status, collection_url, parse_items, parse_total, referer_for,
};

use super::config::ServerConfig;
use super::fetcher::{DoubanFetcher, FetchError};
use super::service;
use super::state::AppState;

/// 单维度最大尝试次数（含首次）
const MAX_ATTEMPTS: i64 = 5;
const IDLE_SCAN: Duration = Duration::from_secs(60);

pub(super) struct DueDimension {
    uid: String,
    category: String,
    dim_status: String,
    next_start: i64,
    attempts: i64,
}

pub async fn run_worker<F: DoubanFetcher>(state: AppState, fetcher: F) {
    let mut wakeup = state.worker_wakeup_receiver();
    loop {
        // 熔断期间不抓取，等冷却结束（分段等待，最长 60s 一段）
        let circuit_ms = state.circuit_remaining_ms();
        if circuit_ms > 0 {
            tokio::time::sleep(Duration::from_millis(circuit_ms.min(60_000) as u64)).await;
            continue;
        }

        match process_next_due(&state, &fetcher).await {
            Ok(true) => {} // 处理了一个维度，立即取下一个
            Ok(false) => {
                // 维度队列空闲：处理条目富集 job（rexxar），同样串行共享限速/熔断；
                // 无到期 job 时进入等待（wakeup 即时唤醒 + 60s 兜底扫描）
                match crate::server::subject::process_next_job(&state, &fetcher).await {
                    Ok(true) => continue,
                    Ok(false) => {}
                    Err(e) => {
                        warn!(error = %e, "subject job tick failed");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                }
                tokio::select! {
                    changed = wakeup.changed() => {
                        if changed.is_err() {
                            warn!("worker wakeup channel closed");
                            tokio::time::sleep(IDLE_SCAN).await;
                        }
                    }
                    _ = tokio::time::sleep(IDLE_SCAN) => {}
                }
            }
            Err(e) => {
                warn!(error = %e, "worker tick failed");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// 处理一个到期维度（一页抓取 + 状态推进）。返回是否处理了维度；
/// 供 run_worker 循环与测试复用（测试可直接驱动而无需起后台 worker）。
pub async fn process_next_due<F: DoubanFetcher>(state: &AppState, fetcher: &F) -> Result<bool> {
    match pick_due_dimension(&state.pool).await? {
        Some(dim) => {
            process_dimension(state, fetcher, &dim).await?;
            Ok(true)
        }
        None => Ok(false),
    }
}

async fn pick_due_dimension(pool: &SqlitePool) -> Result<Option<DueDimension>> {
    let row = sqlx::query(
        "SELECT uid, category, dim_status, next_start, attempts
         FROM dimensions
         WHERE state IN ('queued','running')
           AND (next_sync_ms IS NULL OR next_sync_ms <= ?)
         ORDER BY updated_ms ASC, rowid ASC
         LIMIT 1",
    )
    .bind(ServerConfig::now_ms())
    .fetch_optional(pool)
    .await
    .context("查询到期维度失败")?;

    Ok(row.map(|r| DueDimension {
        uid: r.get("uid"),
        category: r.get("category"),
        dim_status: r.get("dim_status"),
        next_start: r.get("next_start"),
        attempts: r.get("attempts"),
    }))
}

async fn process_dimension<F: DoubanFetcher>(
    state: &AppState,
    fetcher: &F,
    dim: &DueDimension,
) -> Result<()> {
    let Some(cat) = Category::from_key(&dim.category) else {
        anyhow::bail!("未知 category: {}", dim.category);
    };
    let Some(st) = Status::from_key(&dim.dim_status) else {
        anyhow::bail!("未知 status: {}", dim.dim_status);
    };
    let now = ServerConfig::now_ms();

    // 首次处理的维度把关联任务置为 running
    sqlx::query(
        "UPDATE tasks SET status = 'running', started_ms = COALESCE(started_ms, ?)
         WHERE status = 'queued'
           AND task_id IN (SELECT task_id FROM task_dims
                           WHERE uid = ? AND category = ? AND dim_status = ? AND state = 'waiting')",
    )
    .bind(now)
    .bind(&dim.uid)
    .bind(&dim.category)
    .bind(&dim.dim_status)
    .execute(&state.pool)
    .await
    .context("标记关联任务 running 失败")?;

    let start = dim.next_start.max(0) as usize;
    let url = collection_url(cat, &dim.uid, st, start);
    let referer = referer_for(&dim.uid);

    match fetcher.fetch_page(&url, &referer).await {
        Ok(html) => {
            state.record_crawl_success();
            // 全局节流：单 worker 串行 + 每次成功抓取后休眠 = 全局令牌桶，
            // 覆盖跨维度连续抓取的场景
            let interval =
                state.config.crawl_min_interval_secs + rand::thread_rng().gen_range(0.0..1.0);
            tokio::time::sleep(Duration::from_secs_f64(interval)).await;
            let total = parse_total(&html).map(|t| t as i64);
            let items = parse_items(&html, cat, st);
            let empty_page = items.is_empty();

            // 页面顶部资料块顺带解析：头像/用户名随任意一页抓取即可获得
            if let Some(profile) = crawler::parse_profile(&html) {
                if let Err(e) = service::upsert_user_profile(&state.pool, &dim.uid, &profile).await
                {
                    // 资料是顺带产物，失败不阻断条目入库
                    warn!(uid = %dim.uid, error = %e, "用户资料入库失败");
                }
            }

            // 本页写库；主键冲突即重复（翻页偏移/标记新增），交给 INSERT OR IGNORE
            service::upsert_records(&state.pool, &dim.uid, &dim.category, &dim.dim_status, &items)
                .await?;
            let fetched_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM records WHERE uid = ? AND category = ? AND dim_status = ?",
            )
            .bind(&dim.uid)
            .bind(&dim.category)
            .bind(&dim.dim_status)
            .fetch_one(&state.pool)
            .await
            .context("统计维度记录数失败")?;

            // 终点判定：next_start 越过总数；无 total 时只能靠空页终止。
            // 不以"本页不满 15 条"判定——页面可能因条目被删而少渲染。
            let next_start = (start + crawler::PAGE_SIZE) as i64;
            let finished = match total {
                Some(t) => next_start >= t,
                None => empty_page,
            };

            if finished {
                sqlx::query(
                    "UPDATE dimensions SET state='ok', fetched_count=?, total=?,
                            next_start=?, fetched_ms=?, attempts=0, next_sync_ms=NULL,
                            last_error=NULL, updated_ms=?
                     WHERE uid=? AND category=? AND dim_status=?",
                )
                .bind(fetched_count)
                .bind(total)
                .bind(next_start)
                .bind(now)
                .bind(now)
                .bind(&dim.uid)
                .bind(&dim.category)
                .bind(&dim.dim_status)
                .execute(&state.pool)
                .await?;
                info!(
                    uid = %dim.uid,
                    dimension = format!("{}/{}", dim.category, dim.dim_status),
                    items = fetched_count,
                    "维度抓取完成"
                );
                service::finalize_linked_tasks(
                    state, &dim.uid, &dim.category, &dim.dim_status, true, "",
                )
                .await?;
                // 维度完成即入队条目富集（集合差 + INSERT OR IGNORE 跨用户去重）。
                // 失败不阻断主流程——富集是异步增强，缺席只是少了结构化字段。
                if state.config.subject_enrich {
                    if let Err(e) = crate::server::subject::enqueue_missing_subjects(
                        &state.pool,
                        &dim.uid,
                        &dim.category,
                        state.config.subject_ttl_secs,
                    )
                    .await
                    {
                        warn!(uid = %dim.uid, error = %e, "条目富集入队失败");
                    }
                }
            } else {
                sqlx::query(
                    "UPDATE dimensions SET state='running', fetched_count=?, total=?,
                            next_start=?, updated_ms=?
                     WHERE uid=? AND category=? AND dim_status=?",
                )
                .bind(fetched_count)
                .bind(total)
                .bind(next_start)
                .bind(now)
                .bind(&dim.uid)
                .bind(&dim.category)
                .bind(&dim.dim_status)
                .execute(&state.pool)
                .await?;
            }
        }
        Err(e) => handle_fetch_error(state, dim, e).await?,
    }

    // 通知该维度关联的所有任务（SSE 进度推流；无订阅者时为空操作）
    let linked: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT task_id FROM task_dims
         WHERE uid = ? AND category = ? AND dim_status = ?",
    )
    .bind(&dim.uid)
    .bind(&dim.category)
    .bind(&dim.dim_status)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    for task_id in linked {
        state.notify_progress(&task_id);
    }
    Ok(())
}

async fn handle_fetch_error(state: &AppState, dim: &DueDimension, e: FetchError) -> Result<()> {
    let now = ServerConfig::now_ms();
    let code = e.code();
    warn!(
        uid = %dim.uid,
        dimension = format!("{}/{}", dim.category, dim.dim_status),
        attempt = dim.attempts + 1,
        error = %e,
        "维度抓取失败"
    );

    // 404 永久失败；其余按次数退避重试，超限终态失败
    let permanent = !e.is_transient();
    let over_limit = dim.attempts + 1 >= MAX_ATTEMPTS;
    if permanent || over_limit {
        sqlx::query(
            "UPDATE dimensions SET state='error', attempts=?, next_sync_ms=NULL,
                    last_error=?, updated_ms=?
             WHERE uid=? AND category=? AND dim_status=?",
        )
        .bind(dim.attempts + 1)
        .bind(format!("{code}: {e}"))
        .bind(now)
        .bind(&dim.uid)
        .bind(&dim.category)
        .bind(&dim.dim_status)
        .execute(&state.pool)
        .await?;
        if !permanent {
            state.record_crawl_failure();
        }
        service::finalize_linked_tasks(state, &dim.uid, &dim.category, &dim.dim_status, false, code)
            .await?;
    } else {
        let tripped = state.record_crawl_failure();
        let backoff_ms = (dim.attempts + 1) * 20_000 + rand::thread_rng().gen_range(0..5_000);
        sqlx::query(
            "UPDATE dimensions SET attempts=?, next_sync_ms=?, last_error=?, updated_ms=?
             WHERE uid=? AND category=? AND dim_status=?",
        )
        .bind(dim.attempts + 1)
        .bind(now + backoff_ms)
        .bind(format!("{code}: {e}"))
        .bind(now)
        .bind(&dim.uid)
        .bind(&dim.category)
        .bind(&dim.dim_status)
        .execute(&state.pool)
        .await?;
        if tripped {
            info!("熔断已触发，worker 将暂停抓取直至冷却结束");
        }
    }
    Ok(())
}
