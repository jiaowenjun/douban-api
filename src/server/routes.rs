//! HTTP 路由与中间件（鉴权、客户端限流）。

use axum::{
    Json, Router,
    body::Bytes,
    extract::{ConnectInfo, Path, Query, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use serde_json::{Value, json};
use std::convert::Infallible;
use std::net::SocketAddr;

use crate::crawler::{Category, Status};

use super::error::{AppError, AppResult};
use super::service::{self, CreateTaskInput, TERMINAL_STATUSES, TaskView};
use super::state::AppState;

pub fn build_app(state: AppState) -> Router {
    let api = Router::new()
        .route("/syncs", get(list_syncs).post(create_sync))
        .route("/syncs/{task_id}", get(get_sync).delete(delete_sync))
        .route("/syncs/{task_id}/result", get(get_result))
        .route("/syncs/{task_id}/events", get(get_sync_events))
        .route("/users/{uid}/marks", get(get_marks))
        .route("/subscriptions", get(list_subscriptions))
        .route(
            "/subscriptions/{uid}",
            get(get_subscription_route).put(put_subscription).delete(delete_subscription),
        )
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
        let header_ok = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == expected);
        // 浏览器 EventSource 无法设置自定义 Header，允许 ?token= 查询参数兜底
        let query_ok = !header_ok
            && req
                .uri()
                .query()
                .is_some_and(|q| {
                    q.split('&').any(|kv| {
                        kv.split_once('=').is_some_and(|(k, v)| {
                            k == "token" && urldecode(v) == state.config.token
                        })
                    })
                });
        if !header_ok && !query_ok {
            return Err(AppError::Unauthorized(
                "缺少或错误的 Bearer Token（支持 Authorization 头或 ?token= 参数）".to_string(),
            ));
        }
    }
    Ok(next.run(req).await)
}

/// 极简百分号解码（token 一般为 URL 安全字符，覆盖 %+XX 即可）
fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let decoded = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], decoded) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    // ConnectInfo 由 into_make_service_with_connect_info 注入请求扩展；
    // 缺失时（如测试 oneshot）回退 loopback
    let ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.ip())
        .unwrap_or(std::net::IpAddr::from([127, 0, 0, 1]));
    if !state.rate_allow(ip) {
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
// /syncs
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

/// 创建同步任务。body 手动解析以保证错误信封统一（400 而非 axum 默认 422）。
async fn create_sync(
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

    let outcome = service::create_task(
        &state,
        CreateTaskInput {
            user,
            categories,
            statuses,
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

async fn get_sync(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> AppResult<Json<service::TaskView>> {
    Ok(Json(service::get_task(&state, &task_id).await?))
}

// --------------------------------------------------------------------------- //
// SSE 进度推流（设计文档 4.8）
// --------------------------------------------------------------------------- //

fn task_event(name: &str, view: &TaskView) -> Result<Event, Infallible> {
    let data = serde_json::to_string(view).unwrap_or_else(|_| "{}".to_string());
    Ok(Event::default().event(name).data(data))
}

/// SSE 流：先推当前快照（progress），随后仅在进度变更时推送；
/// 终态推 done（partial 也归 done，内容里带 error）/ error 后关流。
/// worker 侧经 AppState 的 broadcast 通道通知（state.notify_progress）。
async fn get_sync_events(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> AppResult<
    Sse<axum::response::sse::KeepAliveStream<
        tokio_stream::wrappers::ReceiverStream<Result<Event, Infallible>>,
    >>,
> {
    // 校验任务存在（404 快速失败）；订阅必须发生在 spawn 之前，
    // 保证响应返回时已建立订阅，连接期间的通知不会丢失
    service::get_task(&state, &task_id).await?;
    let mut notify = state.subscribe_progress();

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);
    tokio::spawn(async move {
        loop {
            let Ok(view) = service::get_task(&state, &task_id).await else { return };
            if TERMINAL_STATUSES.contains(&view.status.as_str()) {
                let name = if view.status == "failed" { "error" } else { "done" };
                let _ = tx.send(task_event(name, &view)).await;
                return;
            }
            if tx.send(task_event("progress", &view)).await.is_err() {
                return; // 客户端断开
            }
            // 等待下一次该任务的进度通知
            loop {
                match notify.recv().await {
                    Ok(id) if id == task_id => break,
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return,
                }
            }
        }
    });

    Ok(Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx))
        .keep_alive(KeepAlive::default()))
}

#[derive(serde::Deserialize, Default)]
struct ListQuery {
    user: Option<String>,
    status: Option<String>,
    /// 任务来源过滤：manual（默认）/ auto / all
    source: Option<String>,
    limit: Option<i64>,
    cursor: Option<i64>,
}

async fn list_syncs(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    if let Some(s) = &q.status {
        if !["queued", "running", "completed", "partial", "failed"].contains(&s.as_str()) {
            return Err(AppError::InvalidArgument(format!("非法 status 过滤值: {s}")));
        }
    }
    let source = match q.source.as_deref() {
        None | Some("manual") => Some("manual".to_string()), // 默认不展示 auto 任务
        Some("auto") => Some("auto".to_string()),
        Some("all") => None,
        Some(other) => {
            return Err(AppError::InvalidArgument(format!(
                "非法 source 过滤值: {other}（允许值: manual/auto/all）"
            )))
        }
    };
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let (tasks, next_cursor) = service::list_tasks(
        &state,
        service::ListFilter {
            user: q.user,
            status: q.status,
            source,
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

async fn delete_sync(
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

// --------------------------------------------------------------------------- //
// /subscriptions
// --------------------------------------------------------------------------- //

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct SubscriptionReq {
    #[serde(default)]
    rss_interval_secs: Option<i64>,
    #[serde(default)]
    full_interval_secs: Option<i64>,
}

/// 创建/更新订阅。body 手动解析以保证错误信封统一（400 而非 422）。
async fn put_subscription(
    State(state): State<AppState>,
    Path(uid): Path<String>,
    body: Bytes,
) -> AppResult<(StatusCode, Json<service::SubscriptionView>)> {
    let req: SubscriptionReq = if body.is_empty() {
        SubscriptionReq::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| AppError::InvalidArgument(e.to_string()))?
    };
    let view = service::upsert_subscription(
        &state,
        &uid,
        req.rss_interval_secs,
        req.full_interval_secs,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn get_subscription_route(
    State(state): State<AppState>,
    Path(uid): Path<String>,
) -> AppResult<Json<service::SubscriptionView>> {
    Ok(Json(service::get_subscription(&state, &uid).await?))
}

async fn list_subscriptions(
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let subs = service::list_subscriptions(&state).await?;
    Ok(Json(json!({ "subscriptions": subs })))
}

async fn delete_subscription(
    State(state): State<AppState>,
    Path(uid): Path<String>,
) -> AppResult<StatusCode> {
    service::delete_subscription(&state, &uid).await?;
    Ok(StatusCode::NO_CONTENT)
}
