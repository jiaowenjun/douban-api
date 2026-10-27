//! HTTP 路由与中间件（鉴权、客户端限流）。

use axum::{
    Json, Router,
    body::Bytes,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use std::net::SocketAddr;

use crate::crawler::{Category, Format, Status};

use super::error::{AppError, AppResult};
use super::service::{self, CreateTaskInput, TERMINAL_STATUSES};
use super::state::AppState;

pub fn build_app(state: AppState) -> Router {
    let api = Router::new()
        .route("/exports", get(list_exports).post(create_export))
        .route("/exports/{task_id}", get(get_export).delete(delete_export))
        .route("/exports/{task_id}/result", get(get_result))
        .route("/exports/{task_id}/result/{file_name}", get(get_result_file))
        .route("/users/{uid}/marks", get(get_marks))
        .route("/server/info", get(server_info))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .with_state(state);

    Router::new().route("/healthz", get(healthz)).nest("/api/v1", api)
}

// --------------------------------------------------------------------------- //
// 中间件
// --------------------------------------------------------------------------- //

async fn auth_middleware(
    State(state): State<AppState>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    if !state.config.token.is_empty() {
        let expected = format!("Bearer {}", state.config.token);
        let ok = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == expected);
        if !ok {
            return Err(AppError::Unauthorized("缺少或错误的 Bearer Token".to_string()));
        }
    }
    Ok(next.run(req).await)
}

async fn rate_limit_middleware(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    if !state.rate_allow(addr.ip()) {
        return Err(AppError::TooManyRequests);
    }
    Ok(next.run(req).await)
}

// --------------------------------------------------------------------------- //
// 基础端点
// --------------------------------------------------------------------------- //

async fn healthz() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn server_info(State(state): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(service::server_info(&state).await?))
}

// --------------------------------------------------------------------------- //
// /exports
// --------------------------------------------------------------------------- //

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct CreateReq {
    user: Option<String>,
    #[serde(default)]
    categories: Option<Vec<String>>,
    #[serde(default)]
    statuses: Option<Vec<String>>,
    #[serde(default)]
    formats: Option<Vec<String>>,
    #[serde(default)]
    refresh: bool,
    #[serde(default)]
    wait_timeout: u64,
}

fn parse_enum_list<T>(
    raw: Option<Vec<String>>,
    all: &[T],
    key: fn(&T) -> &'static str,
    what: &str,
) -> AppResult<Vec<T>>
where
    T: Copy,
{
    let Some(list) = raw else {
        return Ok(all.to_vec());
    };
    if list.is_empty() {
        return Err(AppError::InvalidArgument(format!("{what} 不能为空数组")));
    }
    let mut out = Vec::with_capacity(list.len());
    for item in list {
        let found = all.iter().find(|c| key(c) == item);
        match found {
            Some(c) => out.push(*c),
            None => {
                return Err(AppError::InvalidArgument(format!(
                    "非法 {what}: {item}（允许值: {}）",
                    all.iter().map(&key).collect::<Vec<_>>().join("/")
                )))
            }
        }
    }
    Ok(out)
}

/// 创建导出任务。body 手动解析以保证错误信封统一（400 而非 axum 默认 422）。
async fn create_export(
    State(state): State<AppState>,
    body: Bytes,
) -> AppResult<Response> {
    let req: CreateReq =
        serde_json::from_slice(&body).map_err(|e| AppError::InvalidArgument(e.to_string()))?;
    let user = req
        .user
        .ok_or_else(|| AppError::InvalidArgument("缺少 user 字段".to_string()))?;
    if req.wait_timeout > 30 {
        return Err(AppError::InvalidArgument("wait_timeout 最大 30 秒".to_string()));
    }

    let categories = parse_enum_list(req.categories, &Category::ALL, |c| c.key(), "categories")?;
    const DEFAULT_STATUSES: [Status; 1] = [Status::Collect];
    let statuses = parse_enum_list(req.statuses, &DEFAULT_STATUSES, |s| s.key(), "statuses")?;
    let formats = parse_enum_list(req.formats, &[Format::Csv, Format::Json, Format::Md, Format::Xlsx], |f| f.key(), "formats")?;

    let outcome = service::create_task(
        &state,
        CreateTaskInput {
            user,
            categories,
            statuses,
            formats,
            refresh: req.refresh,
        },
    )
    .await?;

    // 有限同步等待：wait_timeout 内完成则返回终态
    let mut view = outcome.view;
    if req.wait_timeout > 0 && !TERMINAL_STATUSES.contains(&view.status.as_str()) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(req.wait_timeout);
        while std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            view = service::get_task(&state, &view.task_id).await?;
            if TERMINAL_STATUSES.contains(&view.status.as_str()) {
                break;
            }
        }
    }

    let mut body = serde_json::to_value(&view)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    body["cached_dimensions"] = json!(outcome.cached_dimensions);
    body["created_dimensions"] = json!(outcome.created_dimensions);

    let code = if TERMINAL_STATUSES.contains(&view.status.as_str()) {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((code, Json(body)).into_response())
}

async fn get_export(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> AppResult<Json<service::TaskView>> {
    Ok(Json(service::get_task(&state, &task_id).await?))
}

#[derive(serde::Deserialize, Default)]
struct ListQuery {
    user: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    cursor: Option<i64>,
}

async fn list_exports(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    if let Some(s) = &q.status {
        if !["queued", "running", "completed", "partial", "failed"].contains(&s.as_str()) {
            return Err(AppError::InvalidArgument(format!("非法 status 过滤值: {s}")));
        }
    }
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let (tasks, next_cursor) = service::list_tasks(
        &state,
        service::ListFilter {
            user: q.user,
            status: q.status,
            limit,
            cursor_rowid: q.cursor,
        },
    )
    .await?;
    Ok(Json(json!({
        "tasks": tasks,
        "next_cursor": next_cursor,
        "has_more": next_cursor.is_some(),
    })))
}

async fn delete_export(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> AppResult<StatusCode> {
    service::delete_task(&state, &task_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_result(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> AppResult<Json<service::ResultView>> {
    Ok(Json(service::task_result(&state, &task_id).await?))
}

async fn get_result_file(
    State(state): State<AppState>,
    Path((task_id, file_name)): Path<(String, String)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let (bytes, meta) = service::read_task_file(&state, &task_id, &file_name).await?;
    let etag = format!("\"{}\"", &meta.sha256[..16.min(meta.sha256.len())]);
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag)
    {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }
    let mut resp = (StatusCode::OK, bytes).into_response();
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, meta.format.content_type().parse().unwrap());
    h.insert(header::ETAG, etag.parse().unwrap());
    if let Ok(cd) = format!("attachment; filename=\"{file_name}\"").parse() {
        h.insert(header::CONTENT_DISPOSITION, cd);
    }
    Ok(resp)
}

// --------------------------------------------------------------------------- //
// /users/{uid}/marks
// --------------------------------------------------------------------------- //

#[derive(serde::Deserialize)]
struct MarksQuery {
    category: Option<String>,
    status: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

async fn get_marks(
    State(state): State<AppState>,
    Path(uid): Path<String>,
    Query(q): Query<MarksQuery>,
) -> AppResult<Json<Value>> {
    let category = q
        .category
        .as_deref()
        .and_then(Category::from_key)
        .ok_or_else(|| AppError::InvalidArgument("缺少或非法 category 参数".to_string()))?;
    let status = q
        .status
        .as_deref()
        .and_then(Status::from_key)
        .ok_or_else(|| AppError::InvalidArgument("缺少或非法 status 参数".to_string()))?;
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(50).clamp(1, 200);

    let marks = service::read_marks(&state, &uid, category, status, page, page_size).await?;
    Ok(Json(json!({
        "user": marks.user,
        "category": marks.category.key(),
        "status": marks.status.key(),
        "cached_at": marks.cached_at,
        "fresh": marks.fresh,
        "total": marks.total,
        "page": marks.page,
        "page_size": marks.page_size,
        "items": marks.items,
    })))
}
