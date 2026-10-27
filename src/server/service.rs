//! 任务/缓存/文件的服务层——创建任务、查询进度、生成导出文件、缓存读取、过期清理。

use rand::Rng;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;

use crate::crawler::{Category, Format, Record, Status, export_records, parse_uid};

use super::config::ServerConfig;
use super::error::{AppError, AppResult};
use super::state::AppState;

pub const TERMINAL_STATUSES: [&str; 3] = ["completed", "partial", "failed"];

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
}

fn ms_to_rfc3339(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

fn new_task_id() -> String {
    let now = ServerConfig::now_ms() as u64;
    let rand6: u32 = rand::thread_rng().gen_range(0x100000..=0xFFFFFF);
    format!("exp_{now:012x}{rand6:06x}")
}

// --------------------------------------------------------------------------- //
// 创建任务
// --------------------------------------------------------------------------- //

#[derive(Debug, Clone)]
pub struct CreateTaskInput {
    pub user: String,
    pub categories: Vec<Category>,
    pub statuses: Vec<Status>,
    pub formats: Vec<Format>,
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
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM dimensions WHERE state IN ('queued','running')",
    )
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
        "formats": input.formats.iter().map(|f| f.key()).collect::<Vec<_>>(),
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
                insert_task_dim(&state.pool, &task_id, &uid, cat.key(), st.key(), "done", fetched_count).await?;
                cached += 1;
            }
            // single-flight：已有运行中/排队的抓取，并入
            Some((state_str, _, _)) if state_str == "queued" || state_str == "running" => {
                insert_task_dim(&state.pool, &task_id, &uid, cat.key(), st.key(), "waiting", 0).await?;
                created += 1;
            }
            // 需要新抓取（idle / 过期缓存 / refresh / 上次失败）
            dim_state => {
                let is_fresh_ok = dim_state
                    .as_ref()
                    .is_some_and(|(s, _, _)| s == "ok");
                if is_fresh_ok {
                    // refresh：清空旧数据重新全量抓
                    sqlx::query(
                        "UPDATE dimensions SET state='queued', records='[]', fetched_count=0,
                                total=NULL, next_start=0, fetched_ms=NULL, attempts=0,
                                next_sync_ms=NULL, last_error=NULL, updated_ms=?
                         WHERE uid=? AND category=? AND dim_status=?",
                    )
                    .bind(now)
                    .bind(&uid)
                    .bind(cat.key())
                    .bind(st.key())
                    .execute(&state.pool)
                    .await?;
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
                insert_task_dim(&state.pool, &task_id, &uid, cat.key(), st.key(), "waiting", 0).await?;
                created += 1;
                need_wake = true;
            }
        }
    }

    if need_wake {
        state.wake_worker();
    }

    // 全部维度命中缓存 → 直接终态并生成文件
    let waiting: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_dims WHERE task_id = ? AND state != 'done'",
    )
    .bind(&task_id)
    .fetch_one(&state.pool)
    .await?;
    if waiting == 0 {
        finish_task(
            &state.pool,
            &task_id,
            "completed",
            now,
            state.config.file_retention_days * 24 * 3600 * 1000,
        )
        .await?;
        generate_task_files(state, &task_id).await?;
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
                d.state AS dim_state, d.fetched_count, d.total
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
            ProgressEntry {
                category: r.get("category"),
                status: r.get("dim_status"),
                state,
                fetched,
                total,
                percent,
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
        result_url: format!("/api/v1/exports/{task_id}/result"),
    })
}

pub struct ListFilter {
    pub user: Option<String>,
    pub status: Option<String>,
    pub limit: i64,
    pub cursor_rowid: Option<i64>,
}

pub async fn list_tasks(state: &AppState, f: ListFilter) -> AppResult<(Vec<TaskSummary>, Option<i64>)> {
    let rows = sqlx::query(
        "SELECT rowid, task_id, user, status, items_count, created_ms FROM tasks
         WHERE (? IS NULL OR user = ?)
           AND (? IS NULL OR status = ?)
           AND (? IS NULL OR rowid < ?)
         ORDER BY rowid DESC
         LIMIT ?",
    )
    .bind(&f.user)
    .bind(&f.user)
    .bind(&f.status)
    .bind(&f.status)
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
            url: format!("/api/v1/exports/{task_id}"),
            task_id,
            user: r.get("user"),
            status: r.get("status"),
            items_count: r.get("items_count"),
            created_at: ms_to_rfc3339(r.get("created_ms")),
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
    let dir = state.config.files_dir().join(task_id);
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    sqlx::query("DELETE FROM tasks WHERE task_id = ?")
        .bind(task_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

// --------------------------------------------------------------------------- //
// 结果与文件
// --------------------------------------------------------------------------- //

#[derive(Serialize)]
pub struct ResultFileView {
    pub name: String,
    pub category: String,
    pub status: String,
    pub format: String,
    pub size: i64,
    pub sha256: String,
    pub items: i64,
    pub url: String,
}

#[derive(Serialize)]
pub struct ResultView {
    pub task_id: String,
    pub user: String,
    pub summary: Value,
    pub files: Vec<ResultFileView>,
    pub expires_at: Option<String>,
}

pub async fn task_result(state: &AppState, task_id: &str) -> AppResult<ResultView> {
    let task = sqlx::query("SELECT status, user, expires_ms FROM tasks WHERE task_id = ?")
        .bind(task_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::TaskNotFound(task_id.to_string()))?;
    let status: String = task.get("status");
    if !TERMINAL_STATUSES.contains(&status.as_str()) {
        return Err(AppError::TaskNotFinished(status));
    }
    let user: String = task.get("user");

    let file_rows = sqlx::query(
        "SELECT name, category, dim_status, format, size, sha256, items
         FROM task_files WHERE task_id = ? ORDER BY name",
    )
    .bind(task_id)
    .fetch_all(&state.pool)
    .await?;
    let files: Vec<ResultFileView> = file_rows
        .into_iter()
        .map(|r| {
            let name: String = r.get("name");
            ResultFileView {
                url: format!("/api/v1/exports/{task_id}/result/{name}"),
                name,
                category: r.get("category"),
                status: r.get("dim_status"),
                format: r.get("format"),
                size: r.get("size"),
                sha256: r.get("sha256"),
                items: r.get("items"),
            }
        })
        .collect();

    // 汇总（含短评数：读 done 维度的 records 统计）
    let dim_rows = sqlx::query(
        "SELECT td.category, td.dim_status, td.items, d.records
         FROM task_dims td
         LEFT JOIN dimensions d
           ON d.uid = td.uid AND d.category = td.category AND d.dim_status = td.dim_status
         WHERE td.task_id = ? AND td.state = 'done'",
    )
    .bind(task_id)
    .fetch_all(&state.pool)
    .await?;

    let mut total = 0i64;
    let mut with_comment = 0i64;
    let mut by: serde_json::Map<String, Value> = serde_json::Map::new();
    for r in dim_rows {
        let category: String = r.get("category");
        let dim_status: String = r.get("dim_status");
        let items: i64 = r.get("items");
        total += items;
        if let Some(records_json) = r.try_get::<Option<String>, _>("records").ok().flatten() {
            if let Ok(recs) = serde_json::from_str::<Vec<Record>>(&records_json) {
                with_comment += recs.iter().filter(|x| !x.comment.is_empty()).count() as i64;
            }
        }
        let label = Category::from_key(&category)
            .map(|c| format!("{}·{}", c.key(), c.status_label(status_of(&dim_status))))
            .unwrap_or_else(|| format!("{category}·{dim_status}"));
        by.insert(label, json!(items));
    }

    Ok(ResultView {
        task_id: task_id.to_string(),
        user,
        summary: json!({"total": total, "with_comment": with_comment, "by_category_status": by}),
        files,
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

pub async fn read_task_file(
    state: &AppState,
    task_id: &str,
    file_name: &str,
) -> AppResult<(Vec<u8>, ResultFileMeta)> {
    let row = sqlx::query(
        "SELECT name, format, sha256 FROM task_files WHERE task_id = ? AND name = ?",
    )
    .bind(task_id)
    .bind(file_name)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::FileNotFound(file_name.to_string()))?;

    let path = state.config.files_dir().join(task_id).join(file_name);
    let bytes = std::fs::read(&path).map_err(|_| AppError::FileNotFound(file_name.to_string()))?;
    let format_key: String = row.get("format");
    let fmt = match format_key.as_str() {
        "csv" => Format::Csv,
        "json" => Format::Json,
        "md" => Format::Md,
        _ => Format::Xlsx,
    };
    Ok((
        bytes,
        ResultFileMeta {
            sha256: row.get("sha256"),
            format: fmt,
        },
    ))
}

pub struct ResultFileMeta {
    pub sha256: String,
    pub format: Format,
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

pub async fn read_marks(
    state: &AppState,
    uid: &str,
    category: Category,
    status: Status,
    page: i64,
    page_size: i64,
) -> AppResult<MarksPage> {
    let row = sqlx::query(
        "SELECT state, records, fetched_ms FROM dimensions
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
    let state_str: String = row.get("state");
    if state_str != "ok" {
        return Err(AppError::DimensionNotCached);
    }
    let records: Vec<Record> =
        serde_json::from_str(&row.get::<String, _>("records")).unwrap_or_default();
    let fetched_ms: Option<i64> = row.get("fetched_ms");
    let total = records.len();
    let start = ((page - 1).max(0) as usize).min(total);
    let end = (start + page_size as usize).min(total);
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
        items: records[start..end].to_vec(),
    })
}

// --------------------------------------------------------------------------- //
// 任务终态与文件生成（worker 与创建路径共用）
// --------------------------------------------------------------------------- //

/// 维度到达终态后，推进所有关联任务；必要时生成导出文件。
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
    .bind(if ok { None } else { Some(error_code.to_string()) })
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
        state.config.file_retention_days * 24 * 3600 * 1000,
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
        let first_code = failed.first().map(|(_, _, c)| c.clone()).unwrap_or_default();
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

    generate_task_files(state, task_id).await?;
    Ok(())
}

async fn finish_task(
    pool: &SqlitePool,
    task_id: &str,
    status: &str,
    now: i64,
    file_retention_ms: i64,
) -> anyhow::Result<()> {
    let expires = now + file_retention_ms;
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

/// 任务终态后按请求格式生成导出文件（文件名与 CLI 一致）
pub async fn generate_task_files(state: &AppState, task_id: &str) -> anyhow::Result<()> {
    let task = sqlx::query("SELECT user, request_json FROM tasks WHERE task_id = ?")
        .bind(task_id)
        .fetch_optional(&state.pool)
        .await?;
    let Some(task) = task else { return Ok(()) };
    let uid: String = task.get("user");
    let request: Value =
        serde_json::from_str(&task.get::<String, _>("request_json")).unwrap_or(json!({}));
    let formats: Vec<Format> = request["formats"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .filter_map(|s| match s {
                    "csv" => Some(Format::Csv),
                    "json" => Some(Format::Json),
                    "md" => Some(Format::Md),
                    "xlsx" => Some(Format::Xlsx),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    let dims = sqlx::query(
        "SELECT td.category, td.dim_status, d.records
         FROM task_dims td
         JOIN dimensions d
           ON d.uid = td.uid AND d.category = td.category AND d.dim_status = td.dim_status
         WHERE td.task_id = ? AND td.state = 'done'
         ORDER BY td.rowid",
    )
    .bind(task_id)
    .fetch_all(&state.pool)
    .await?;

    let dir = state.config.files_dir().join(task_id);
    std::fs::create_dir_all(&dir)?;

    for dim in dims {
        let category: String = dim.get("category");
        let dim_status: String = dim.get("dim_status");
        let Some(cat) = Category::from_key(&category) else { continue };
        let Some(st) = Status::from_key(&dim_status) else { continue };
        let records: Vec<Record> =
            serde_json::from_str(&dim.get::<String, _>("records")).unwrap_or_default();
        if records.is_empty() {
            continue;
        }
        let md_title = format!(
            "{} 的{}记录·{}",
            uid,
            cat.status_label(st),
            cat.kind_label()
        );

        for fmt in &formats {
            let name = format!("{uid}_{category}_{dim_status}.{}", fmt.key());
            let path = dir.join(&name);
            export_records(*fmt, &records, &path, &md_title)?;
            let bytes = std::fs::read(&path)?;
            let sha256 = hex_sha256(&bytes);
            sqlx::query(
                "INSERT OR REPLACE INTO task_files
                        (task_id, name, category, dim_status, format, size, sha256, items)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(task_id)
            .bind(&name)
            .bind(&category)
            .bind(&dim_status)
            .bind(fmt.key())
            .bind(bytes.len() as i64)
            .bind(&sha256)
            .bind(records.len() as i64)
            .execute(&state.pool)
            .await?;
        }
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
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
    let task_ret = state.config.task_retention_days * 24 * 3600 * 1000;
    let file_ret = state.config.file_retention_days * 24 * 3600 * 1000;

    let expired: Vec<String> = sqlx::query_scalar(
        "SELECT task_id FROM tasks
         WHERE (expires_ms IS NOT NULL AND expires_ms + ? < ?)
            OR (finished_ms IS NOT NULL AND finished_ms + ? < ?)",
    )
    .bind(file_ret)
    .bind(now)
    .bind(task_ret)
    .bind(now)
    .fetch_all(&state.pool)
    .await?;

    for task_id in expired {
        let dir: PathBuf = state.config.files_dir().join(&task_id);
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
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
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT state, COUNT(*) FROM dimensions WHERE state IN ('queued','running') GROUP BY state")
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
    Ok(json!({
        "name": "douban-export-server",
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
            "retention_file_days": state.config.file_retention_days,
        },
        "queue": {"queued": queued, "running": running},
        "auth_enabled": !state.config.token.is_empty(),
    }))
}
