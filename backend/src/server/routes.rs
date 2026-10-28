//! HTTP 路由与中间件（鉴权、客户端限流）。

use tower_http::cors::CorsLayer;

use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, Query, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{delete, get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::net::SocketAddr;

use crate::crawler::{Category, Status};

use super::error::{AppError, AppResult};
use super::service::{self, CreateTaskInput, TaskView, TERMINAL_STATUSES};
use super::state::AppState;
use super::subject;

/// CORS 白名单（逗号分隔；空 = 不启用）。跨源部署 admin（如 dist 静态站 + 远端 API）时配置。
fn cors_layer(origins: &str) -> Option<CorsLayer> {
    if origins.trim().is_empty() {
        return None;
    }
    let list: Vec<_> = origins
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<axum::http::HeaderValue>().ok())
        .collect();
    if list.is_empty() {
        return None;
    }
    Some(CorsLayer::new().allow_origin(list))
}

pub fn build_app(state: AppState) -> Router {
    let cors = cors_layer(&state.config.cors_origins);
    // 封面端点单列：仍走鉴权（img 标签带不了 Header，靠 ?token= 兜底），
    // 但豁免限流——一页 20 张封面属正常连发，且磁盘命中后近乎静态文件
    let covers = Router::new()
        .route("/covers", get(get_cover))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    let api = Router::new()
        .route("/syncs", get(list_syncs).post(create_sync))
        .route("/syncs/{task_id}", get(get_sync).delete(delete_sync))
        .route("/syncs/{task_id}/result", get(get_result))
        .route("/syncs/{task_id}/events", get(get_sync_events))
        .route("/users/{uid}/marks", get(get_marks))
        .route("/subjects/{category}/{subject_id}", get(get_subject))
        .route(
            "/subjects/{category}/{subject_id}/refresh",
            post(refresh_subject),
        )
        .route("/subjects", get(list_subjects))
        .route("/subjects/backfill", post(backfill_subjects))
        .route("/subjects/jobs", get(list_subject_jobs))
        .route("/subjects/jobs/clear", post(clear_subject_jobs))
        .route("/subjects/jobs/requeue", post(requeue_subject_jobs))
        .route(
            "/subjects/jobs/{category}/{subject_id}",
            delete(delete_subject_job),
        )
        .route("/users/{uid}/profile", get(get_user_profile))
        .route("/subscriptions", get(list_subscriptions))
        .route(
            "/subscriptions/{uid}",
            get(get_subscription_route)
                .put(put_subscription)
                .delete(delete_subscription),
        )
        .route("/server/info", get(server_info))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .with_state(state)
        .merge(covers);

    let router = Router::new()
        .route("/healthz", get(healthz))
        .nest("/api/v1", api);
    match cors {
        Some(layer) => router.layer(layer),
        None => router,
    }
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
            && req.uri().query().is_some_and(|q| {
                q.split('&').any(|kv| {
                    kv.split_once('=')
                        .is_some_and(|(k, v)| k == "token" && urldecode(v) == state.config.token)
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
async fn create_sync(State(state): State<AppState>, body: Bytes) -> AppResult<Response> {
    let req: CreateReq =
        serde_json::from_slice(&body).map_err(|e| AppError::InvalidArgument(e.to_string()))?;
    let user = req
        .user
        .ok_or_else(|| AppError::InvalidArgument("缺少 user 字段".to_string()))?;
    if req.wait_timeout > 30 {
        return Err(AppError::InvalidArgument(
            "wait_timeout 最大 30 秒".to_string(),
        ));
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

    let mut body = serde_json::to_value(&view).map_err(|e| AppError::Internal(e.to_string()))?;
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
    Sse<
        axum::response::sse::KeepAliveStream<
            tokio_stream::wrappers::ReceiverStream<Result<Event, Infallible>>,
        >,
    >,
> {
    // 校验任务存在（404 快速失败）；订阅必须发生在 spawn 之前，
    // 保证响应返回时已建立订阅，连接期间的通知不会丢失
    service::get_task(&state, &task_id).await?;
    let mut notify = state.subscribe_progress();

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);
    tokio::spawn(async move {
        loop {
            let Ok(view) = service::get_task(&state, &task_id).await else {
                return;
            };
            if TERMINAL_STATUSES.contains(&view.status.as_str()) {
                let name = if view.status == "failed" {
                    "error"
                } else {
                    "done"
                };
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

    Ok(Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
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
            return Err(AppError::InvalidArgument(format!(
                "非法 status 过滤值: {s}"
            )));
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
    /// 展开 items 关联的条目元信息；当前仅支持 "subject"（未富集条目该字段为 null）
    expand: Option<String>,
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

    let expand_subject = match q.expand.as_deref() {
        None => false,
        Some("subject") => true,
        Some(other) => {
            return Err(AppError::InvalidArgument(format!(
                "非法 expand 值: {other}（允许值: subject）"
            )))
        }
    };

    let marks = service::read_marks(&state, &uid, category, status, page, page_size).await?;
    let items: Value = if expand_subject {
        // subjects 已富集返回 rexxar 视图，未富集回退 intro 解析合成（source=intro）
        let views = subject::load_subject_views(&state, category.key(), &marks.items).await?;
        let mut arr =
            serde_json::to_value(&marks.items).map_err(|e| AppError::Internal(e.to_string()))?;
        if let Value::Array(list) = &mut arr {
            for (item, view) in list.iter_mut().zip(views) {
                item["subject"] = view;
            }
        }
        arr
    } else {
        serde_json::to_value(&marks.items).map_err(|e| AppError::Internal(e.to_string()))?
    };
    Ok(Json(json!({
        "user": marks.user,
        "category": marks.category.key(),
        "status": marks.status.key(),
        "cached_at": marks.cached_at,
        "fresh": marks.fresh,
        "total": marks.total,
        "page": marks.page,
        "page_size": marks.page_size,
        "items": items,
    })))
}

// --------------------------------------------------------------------------- //
// /subjects（条目元信息缓存，rexxar 富集；见《条目元信息富集方案.md》§7）
// --------------------------------------------------------------------------- //

fn parse_category(category: &str) -> AppResult<Category> {
    Category::from_key(category)
        .ok_or_else(|| AppError::InvalidArgument(format!("非法 category: {category}")))
}

async fn get_subject(
    State(state): State<AppState>,
    Path((category, subject_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    parse_category(&category)?;
    Ok(Json(
        subject::get_subject_view(&state, &category, &subject_id).await?,
    ))
}

#[derive(serde::Deserialize, Default)]
struct SubjectsQuery {
    /// movie/book/music；缺省或 all = 全部
    category: Option<String>,
    /// 标题模糊搜索
    q: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

async fn list_subjects(
    State(state): State<AppState>,
    Query(q): Query<SubjectsQuery>,
) -> AppResult<Json<Value>> {
    if let Some(c) = q.category.as_deref() {
        if !c.is_empty() && c != "all" {
            parse_category(c)?;
        }
    }
    Ok(Json(
        subject::list_subjects(
            &state,
            q.category.as_deref(),
            q.q.as_deref().unwrap_or("").trim(),
            q.page.unwrap_or(1),
            q.page_size.unwrap_or(20),
        )
        .await?,
    ))
}

/// 强制刷新单个条目（无视 TTL 立即重抓 rexxar）
async fn refresh_subject(
    State(state): State<AppState>,
    Path((category, subject_id)): Path<(String, String)>,
) -> AppResult<(StatusCode, Json<Value>)> {
    parse_category(&category)?;
    subject::force_refresh(&state.pool, &category, &subject_id).await?;
    state.wake_worker();
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"queued": true, "category": category, "subject_id": subject_id})),
    ))
}

/// 存量回填：对 records 已有的全部 (uid, category) 跑富集集合差（幂等可重跑）。
/// 只入缺失条目——subjects 已有的不重抓（重抓需显式 refresh / requeue）。
async fn backfill_subjects(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let (queued, pairs) = subject::backfill(&state.pool).await?;
    state.wake_worker();
    Ok(Json(
        json!({"queued_jobs": queued, "uid_category_pairs": pairs}),
    ))
}

// --------------------------------------------------------------------------- //
// /subjects/jobs（富集任务管理：查看 / 中断 / 删除 / 重排）
// --------------------------------------------------------------------------- //

const JOB_STATES: [&str; 5] = ["queued", "running", "ok", "error", "not_found"];

#[derive(serde::Deserialize)]
struct JobsQuery {
    /// 逗号分隔的状态过滤（queued/running/ok/error/not_found）；缺省 = 全部
    state: Option<String>,
    /// movie/book/music；缺省或 all = 全部
    category: Option<String>,
    /// 条目 ID / 标题模糊搜索
    q: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

async fn list_subject_jobs(
    State(state): State<AppState>,
    Query(q): Query<JobsQuery>,
) -> AppResult<Json<Value>> {
    let states: Vec<String> = match q.state.as_deref() {
        None | Some("") | Some("all") => vec![],
        Some(s) => {
            let mut out = Vec::new();
            for part in s.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if !JOB_STATES.contains(&part) {
                    return Err(AppError::InvalidArgument(format!(
                        "非法 state 过滤值: {part}（允许值: {}）",
                        JOB_STATES.join("/")
                    )));
                }
                out.push(part.to_string());
            }
            out
        }
    };
    if let Some(c) = q.category.as_deref() {
        if !c.is_empty() && c != "all" {
            parse_category(c)?;
        }
    }
    Ok(Json(
        subject::list_jobs(
            &state,
            &states,
            q.category.as_deref(),
            q.q.as_deref().unwrap_or("").trim(),
            q.page.unwrap_or(1),
            q.page_size.unwrap_or(20),
        )
        .await?,
    ))
}

async fn delete_subject_job(
    State(state): State<AppState>,
    Path((category, subject_id)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    parse_category(&category)?;
    subject::delete_job(&state.pool, &category, &subject_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ClearJobsReq {
    /// 待删除的状态集合（必填）：中断队列传 ["queued","running"]
    states: Vec<String>,
    /// 可选类别过滤
    #[serde(default)]
    category: Option<String>,
}

async fn clear_subject_jobs(State(state): State<AppState>, body: Bytes) -> AppResult<Json<Value>> {
    let req: ClearJobsReq =
        serde_json::from_slice(&body).map_err(|e| AppError::InvalidArgument(e.to_string()))?;
    if req.states.is_empty() {
        return Err(AppError::InvalidArgument("states 不能为空数组".to_string()));
    }
    for st in &req.states {
        if !JOB_STATES.contains(&st.as_str()) {
            return Err(AppError::InvalidArgument(format!(
                "非法 state: {st}（允许值: {}）",
                JOB_STATES.join("/")
            )));
        }
    }
    let deleted = subject::clear_jobs(&state.pool, &req.states, req.category.as_deref()).await?;
    Ok(Json(json!({"deleted": deleted})))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RequeueJobsReq {
    /// failed（重试 error 终态）/ not_found（重试已删除条目）/ stale（强制重抓过期条目）
    scope: String,
    #[serde(default)]
    category: Option<String>,
}

async fn requeue_subject_jobs(
    State(state): State<AppState>,
    body: Bytes,
) -> AppResult<Json<Value>> {
    let req: RequeueJobsReq =
        serde_json::from_slice(&body).map_err(|e| AppError::InvalidArgument(e.to_string()))?;
    if !["failed", "not_found", "stale"].contains(&req.scope.as_str()) {
        return Err(AppError::InvalidArgument(
            "非法 scope（允许值: failed/not_found/stale）".to_string(),
        ));
    }
    let queued = subject::requeue_jobs(&state, &req.scope, req.category.as_deref()).await?;
    state.wake_worker();
    Ok(Json(json!({"queued": queued})))
}

// --------------------------------------------------------------------------- //
// /users/{uid}/profile
// --------------------------------------------------------------------------- //

async fn get_user_profile(
    State(state): State<AppState>,
    Path(uid): Path<String>,
) -> AppResult<Json<service::UserProfileView>> {
    Ok(Json(service::get_user_profile(&state, &uid).await?))
}

// --------------------------------------------------------------------------- //
// /covers
// --------------------------------------------------------------------------- //

#[derive(serde::Deserialize)]
struct CoverQuery {
    u: String,
}

/// 封面代理（cache-then-serve）：磁盘命中直接出文件，
/// 未命中带豆瓣 Referer 回源一次并落盘，之后永久本地化。
async fn get_cover(
    State(state): State<AppState>,
    Query(q): Query<CoverQuery>,
) -> AppResult<Response> {
    let cover = state.covers.get(&q.u).await?;
    // 缓存键为豆瓣 photo id，内容不可变，允许浏览器长缓存
    let mut resp = (
        [(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("public, max-age=2592000, immutable"),
        )],
        cover.bytes,
    )
        .into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(cover.content_type),
    );
    Ok(resp)
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
        serde_json::from_slice(&body).map_err(|e| AppError::InvalidArgument(e.to_string()))?
    };
    let view =
        service::upsert_subscription(&state, &uid, req.rss_interval_secs, req.full_interval_secs)
            .await?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn get_subscription_route(
    State(state): State<AppState>,
    Path(uid): Path<String>,
) -> AppResult<Json<service::SubscriptionView>> {
    Ok(Json(service::get_subscription(&state, &uid).await?))
}

async fn list_subscriptions(State(state): State<AppState>) -> AppResult<Json<Value>> {
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
