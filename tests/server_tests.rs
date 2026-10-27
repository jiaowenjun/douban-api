//! 服务器集成测试——weimo 范式：tower oneshot 直接打 Router，进程内完成，
//! 不起端口、不碰真实豆瓣（经 FakeFetcher 注入脱敏后的真实页面 fixture）。

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use douban_export::crawler::{Category, Record, Status, parse_items, parse_total};
use douban_export::server::{
    config::ServerConfig,
    db,
    fetcher::{DoubanFetcher, FetchError},
    routes::build_app,
    service::{self, CreateTaskInput},
    state::AppState,
    worker,
};
use serde_json::{Value, json};
use std::net::IpAddr;
use tempfile::TempDir;
use tower::ServiceExt;

// --------------------------------------------------------------------------- //
// 基础设施
// --------------------------------------------------------------------------- //

fn test_config(dir: &TempDir, token: &str) -> ServerConfig {
    ServerConfig {
        bind: "127.0.0.1:0".to_string(),
        data_dir: dir.path().to_path_buf(),
        token: token.to_string(),
        douban_cookie: String::new(),
        cors_origins: String::new(),
        cache_ttl_secs: 86_400,
        crawl_min_interval_secs: 0.0, // 测试不限速
        circuit_threshold: 5,
        circuit_cooldown_secs: 600,
        task_retention_days: 30,
        file_retention_days: 7,
        queue_limit: 100,
        // oneshot 无真实连接，IP 回退到 loopback 且全测试共享；调大避免互扰
        rate_per_minute: 10_000_000,
    }
}

async fn setup() -> (TempDir, AppState) {
    setup_with_token("").await
}

async fn setup_with_token(token: &str) -> (TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(&dir, token);
    std::fs::create_dir_all(config.files_dir()).unwrap();
    let pool = db::connect(&config.db_path()).await.unwrap();
    let state = AppState::new(config, pool);
    (dir, state)
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap()
}

/// 按 URL（域名 + start 偏移）返回对应的脱敏真实页面
fn fixture_fetcher() -> FakeFetcher<impl Fn(&str) -> Result<String, FetchError>> {
    FakeFetcher(|url: &str| {
        let start: usize = url
            .split("start=")
            .nth(1)
            .and_then(|s| s.split('&').next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let page = start / 15 + 1;
        if url.contains("music.douban.com") {
            Ok(fixture(&format!("music_collect_p{page}.html")))
        } else if url.contains("book.douban.com") {
            Ok(fixture(&format!("book_collect_p{page}.html")))
        } else {
            Err(FetchError::NotFound)
        }
    })
}

struct FakeFetcher<F>(F);

impl<F> DoubanFetcher for FakeFetcher<F>
where
    F: Fn(&str) -> Result<String, FetchError> + Send + Sync,
{
    fn fetch_page(
        &self,
        url: &str,
        _referer: &str,
    ) -> impl std::future::Future<Output = Result<String, FetchError>> + Send {
        let out = (self.0)(url);
        async move { out }
    }
}

/// 驱动 worker 直到没有到期维度（每页含 ≤1s 抖动休眠，页数少耗时可忽略）
async fn drive(state: &AppState, fetcher: &impl DoubanFetcher) {
    for _ in 0..100 {
        if !worker::process_next_due(state, fetcher).await.unwrap() {
            break;
        }
    }
}

async fn post_export(state: &AppState, body: Value) -> (StatusCode, Value) {
    let app = build_app(state.clone());
    request(
        app,
        Method::POST,
        "/api/v1/exports",
        Some(body.to_string()),
        None,
    )
    .await
}

async fn request(
    app: Router,
    method: Method,
    uri: &str,
    body: Option<String>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let req = match body {
        Some(b) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// 预置一份"已成功缓存"的维度（records 来自 fixture 解析）
async fn seed_ok_dimension(state: &AppState, category: Category, records: Vec<Record>) {
    let now = ServerConfig::now_ms();
    let page_size = douban_export::crawler::PAGE_SIZE as i64;
    let len = records.len() as i64;
    sqlx::query(
        "INSERT INTO dimensions (uid, category, dim_status, state, records, fetched_count,
                                 total, next_start, fetched_ms, updated_ms)
         VALUES ('wenjun30', ?, 'collect', 'ok', ?, ?, ?, ?, ?, ?)",
    )
    .bind(category.key())
    .bind(serde_json::to_string(&records).unwrap())
    .bind(len)
    .bind(len)
    .bind((len + page_size - 1) / page_size * page_size)
    .bind(now)
    .bind(now)
    .execute(&state.pool)
    .await
    .unwrap();
}

fn create_input(categories: &[Category]) -> CreateTaskInput {
    CreateTaskInput {
        user: "wenjun30".to_string(),
        categories: categories.to_vec(),
        statuses: vec![Status::Collect],
        formats: vec![douban_export::crawler::Format::Csv],
        refresh: false,
    }
}

async fn get_task_view(state: &AppState, task_id: &str) -> Value {
    serde_json::to_value(service::get_task(state, task_id).await.unwrap()).unwrap()
}

// --------------------------------------------------------------------------- //
// 解析器（fixture 锁定豆瓣两代页面结构，改版时测试第一时间红）
// --------------------------------------------------------------------------- //

#[test]
fn parser_music_old_structure() {
    let items = parse_items(&fixture("music_collect_p1.html"), Category::Music, Status::Collect);
    assert_eq!(items.len(), 15);
    // 音乐短评在无 class 的 <li> 里（脱敏后为占位文本）
    assert_eq!(items.iter().filter(|r| !r.comment.is_empty()).count(), 9);
    assert!(items.iter().all(|r| r.comment.is_empty() || r.comment.contains("测试短评")));
    let first = &items[0];
    assert_eq!(first.title, "歌之女");
    assert_eq!(first.my_rating, Some(5));
    assert!(first.subject_url.starts_with("https://music.douban.com/subject/"));
    assert!(!first.subject_id.is_empty());
    assert!(first.cover.starts_with("https://img"));
    assert_eq!(parse_total(&fixture("music_collect_p1.html")), Some(29));
}

#[test]
fn parser_book_new_structure() {
    let items = parse_items(&fixture("book_collect_p1.html"), Category::Book, Status::Collect);
    assert_eq!(items.len(), 15);
    // 图书为 li.subject-item 新结构，副书名在标题链接内的 span
    let fy = items.iter().find(|r| r.title == "浮生六记").unwrap();
    assert_eq!(fy.alt_title, "人间三梦：古典文库系列3");
    assert!(fy.intro.contains("广西师范大学出版社"));
    assert_eq!(parse_total(&fixture("book_collect_p4.html")), Some(55));
}

// --------------------------------------------------------------------------- //
// 任务生命周期（oneshot 打 Router）
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn cached_dimension_creates_completed_task_instantly() {
    let (_dir, state) = setup().await;
    let records = parse_items(&fixture("music_collect_p1.html"), Category::Music, Status::Collect);
    seed_ok_dimension(&state, Category::Music, records).await;

    let (code, v) = post_export(
        &state,
        json!({"user": "wenjun30", "categories": ["music"], "formats": ["csv","json"],
               "wait_timeout": 10}),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "全缓存命中应秒回终态: {v}");
    assert_eq!(v["status"], "completed");
    assert_eq!(v["cached_dimensions"], 1);
    assert_eq!(v["created_dimensions"], 0);
    assert_eq!(v["items_count"], 15);
    assert_eq!(v["percent"], 100);

    // 结果与文件
    let task_id = v["task_id"].as_str().unwrap().to_string();
    let app = build_app(state.clone());
    let (code, result) = request(app.clone(), Method::GET, &format!("/api/v1/exports/{task_id}/result"), None, None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(result["files"].as_array().unwrap().len(), 2);
    assert_eq!(result["summary"]["total"], 15);

    let (code, _body) = request(
        app,
        Method::GET,
        &format!("/api/v1/exports/{task_id}/result/wenjun30_music_collect.csv"),
        None,
        None,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn worker_crawls_book_dimension_to_completion() {
    let (_dir, state) = setup().await;
    let (code, v) = post_export(
        &state,
        json!({"user": "wenjun30", "categories": ["book"], "formats": ["csv"]}),
    )
    .await;
    assert_eq!(code, StatusCode::ACCEPTED);
    let task_id = v["task_id"].as_str().unwrap().to_string();

    drive(&state, &fixture_fetcher()).await;

    let view = get_task_view(&state, &task_id).await;
    assert_eq!(view["status"], "completed");
    assert_eq!(view["items_count"], 55);
    assert_eq!(view["percent"], 100);
    let p = &view["progress"][0];
    assert_eq!(p["next_start"], 60);
    assert_eq!(p["page"], 4);
    assert_eq!(p["pages_total"], 4);
}

#[tokio::test]
async fn worker_crawls_music_with_dedup_and_percent_progress() {
    let (_dir, state) = setup().await;
    let (_, v) = post_export(
        &state,
        json!({"user": "wenjun30", "categories": ["music"], "formats": ["csv"]}),
    )
    .await;
    let task_id = v["task_id"].as_str().unwrap().to_string();

    // 抓完第一页后中断检查中间进度
    assert!(worker::process_next_due(&state, &fixture_fetcher()).await.unwrap());
    let view = get_task_view(&state, &task_id).await;
    assert_eq!(view["status"], "running");
    let p = &view["progress"][0];
    assert_eq!(p["state"], "running");
    assert_eq!(p["fetched"], 15);
    assert_eq!(p["total"], 29);
    assert_eq!(p["percent"], 51);
    assert_eq!(p["page"], 1);
    assert_eq!(p["pages_total"], 2);

    drive(&state, &fixture_fetcher()).await;
    let view = get_task_view(&state, &task_id).await;
    assert_eq!(view["status"], "completed");
    assert_eq!(view["items_count"], 28); // 28/29，缺 1 条为豆瓣隐藏条目
    assert_eq!(view["percent"], 100);
}

#[tokio::test]
async fn single_flight_links_two_tasks_to_same_dimension() {
    let (_dir, state) = setup().await;
    let (_, a) = post_export(&state, json!({"user": "wenjun30", "categories": ["music"]})).await;
    let (_, b) = post_export(&state, json!({"user": "wenjun30", "categories": ["music"]})).await;
    let a_id = a["task_id"].as_str().unwrap().to_string();
    let b_id = b["task_id"].as_str().unwrap().to_string();

    drive(&state, &fixture_fetcher()).await;

    // 两个任务都应完成且条数一致（共享同一次抓取）
    for id in [&a_id, &b_id] {
        let view = get_task_view(&state, id).await;
        assert_eq!(view["status"], "completed", "task {id}");
        assert_eq!(view["items_count"], 28);
    }
}

#[tokio::test]
async fn not_found_marks_task_failed() {
    let (_dir, state) = setup().await;
    let fetcher = FakeFetcher(|_url: &str| Err(FetchError::NotFound));
    let (_, v) = post_export(&state, json!({"user": "wenjun30", "categories": ["movie"]})).await;
    let task_id = v["task_id"].as_str().unwrap().to_string();

    drive(&state, &fetcher).await;

    let view = get_task_view(&state, &task_id).await;
    assert_eq!(view["status"], "failed");
    assert_eq!(view["error"]["code"], "PAGE_NOT_FOUND");
    assert_eq!(view["progress"][0]["state"], "error");
    assert!(view["percent"].is_null());
}

#[tokio::test]
async fn retry_exhaustion_fails_dimension_and_trips_circuit() {
    let (_dir, state) = setup().await;
    let fetcher = FakeFetcher(|_url: &str| Err(FetchError::Blocked(403)));
    let (_, v) = post_export(&state, json!({"user": "wenjun30", "categories": ["movie"]})).await;
    let task_id = v["task_id"].as_str().unwrap().to_string();

    // 重试带退避（next_sync_ms 推后），测试中强制置为到期以驱动下一次尝试
    for attempt in 1..=5 {
        sqlx::query("UPDATE dimensions SET next_sync_ms = 0")
            .execute(&state.pool)
            .await
            .unwrap();
        assert!(worker::process_next_due(&state, &fetcher).await.unwrap(), "第 {attempt} 次应处理");
    }

    let view = get_task_view(&state, &task_id).await;
    assert_eq!(view["status"], "failed");
    assert_eq!(view["error"]["code"], "DOUBAN_BLOCKED");

    // 连续失败达到阈值，熔断开启
    assert!(state.circuit_remaining_ms() > 0);

    // 维度 last_error 记录了错误码
    let last_error: Option<String> =
        sqlx::query_scalar("SELECT last_error FROM dimensions WHERE category = 'movie'")
            .fetch_optional(&state.pool)
            .await
            .unwrap()
            .flatten();
    assert!(last_error.unwrap().contains("DOUBAN_BLOCKED"));
}

// --------------------------------------------------------------------------- //
// 鉴权 / 校验 / marks / 删除
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn auth_required_when_token_configured() {
    let (_dir, state) = setup_with_token("s3cret").await;
    let app = build_app(state.clone());

    let (code, v) = request(app.clone(), Method::GET, "/api/v1/server/info", None, None).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    assert_eq!(v["error"]["code"], "UNAUTHORIZED");

    let (code, _) = request(
        app.clone(),
        Method::GET,
        "/api/v1/server/info",
        None,
        Some("wrong"),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);

    let (code, _) = request(app, Method::GET, "/api/v1/server/info", None, Some("s3cret")).await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn invalid_arguments_return_400_envelope() {
    let (_dir, state) = setup().await;
    let (code, v) = post_export(&state, json!({"user": "wenjun30", "categories": ["game"]})).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"]["code"], "INVALID_ARGUMENT");

    let (code, v) = post_export(&state, json!({"user": "wenjun30", "wait_timeout": 99})).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"]["code"], "INVALID_ARGUMENT");

    // 非法 JSON body
    let app = build_app(state.clone());
    let (code, v) = request(app, Method::POST, "/api/v1/exports", Some("{bad json".into()), None).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"]["code"], "INVALID_ARGUMENT");
}

#[tokio::test]
async fn marks_pagination_and_not_cached() {
    let (_dir, state) = setup().await;
    let records = parse_items(&fixture("music_collect_p1.html"), Category::Music, Status::Collect);
    seed_ok_dimension(&state, Category::Music, records.clone()).await;

    let app = build_app(state.clone());
    let (code, v) = request(
        app.clone(),
        Method::GET,
        "/api/v1/users/wenjun30/marks?category=music&status=collect&page=2&page_size=5",
        None,
        None,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(v["total"], 15);
    assert_eq!(v["fresh"], true);
    // 第 2 页（每页 5 条）必须是第 6–10 条，而非从头偏移 page-1
    assert_eq!(v["items"].as_array().unwrap().len(), 5);
    assert_eq!(v["items"][0]["title"], records[5].title);
    assert_eq!(v["items"][4]["title"], records[9].title);

    // 未缓存维度
    let (code, v) = request(
        app,
        Method::GET,
        "/api/v1/users/wenjun30/marks?category=book&status=collect",
        None,
        None,
    )
    .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    assert_eq!(v["error"]["code"], "DIMENSION_NOT_CACHED");
}

#[tokio::test]
async fn delete_task_removes_files_and_returns_404() {
    let (dir, state) = setup().await;
    let records = parse_items(&fixture("music_collect_p1.html"), Category::Music, Status::Collect);
    seed_ok_dimension(&state, Category::Music, records).await;
    let (_, v) = post_export(&state, json!({"user": "wenjun30", "categories": ["music"]})).await;
    let task_id = v["task_id"].as_str().unwrap().to_string();
    let task_dir = state.config.files_dir().join(&task_id);
    assert!(task_dir.exists());

    let app = build_app(state.clone());
    let (code, _) = request(app.clone(), Method::DELETE, &format!("/api/v1/exports/{task_id}"), None, None).await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    assert!(!task_dir.exists(), "任务文件目录应被删除");

    let (code, v) = request(app, Method::GET, &format!("/api/v1/exports/{task_id}"), None, None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    assert_eq!(v["error"]["code"], "TASK_NOT_FOUND");
    drop(dir);
}

#[tokio::test]
async fn rate_allow_fixed_window() {
    let (dir, mut state) = setup().await;
    // setup 的限流很大；单独构造一个 limit=2 的 state（IP 用独立地址避免共享表互扰）
    state.config.rate_per_minute = 2;
    let ip: IpAddr = "203.0.113.7".parse().unwrap();
    assert!(state.rate_allow(ip));
    assert!(state.rate_allow(ip));
    assert!(!state.rate_allow(ip), "同 IP 同窗口第三次应拒绝");
    let other: IpAddr = "203.0.113.8".parse().unwrap();
    assert!(state.rate_allow(other), "不同 IP 不受影响");
    drop(dir);
}

// --------------------------------------------------------------------------- //
// SSE 进度推流
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn sse_emits_done_for_terminal_task() {
    let (_dir, state) = setup().await;
    let records = parse_items(&fixture("music_collect_p1.html"), Category::Music, Status::Collect);
    seed_ok_dimension(&state, Category::Music, records).await;
    let (_, v) = post_export(&state, json!({"user": "wenjun30", "categories": ["music"]})).await;
    let task_id = v["task_id"].as_str().unwrap().to_string();

    let app = build_app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/exports/{task_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap(),
        "text/event-stream"
    );
    // 终态任务：推 done 事件后流关闭，body 可完整读取
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("event: done"), "body: {body}");
    assert!(body.contains("\"status\":\"completed\""), "body: {body}");
}

#[tokio::test]
async fn sse_follows_crawl_to_completion() {
    let (_dir, state) = setup().await;
    let (_, v) = post_export(
        &state,
        json!({"user": "wenjun30", "categories": ["music"], "formats": ["csv"]}),
    )
    .await;
    let task_id = v["task_id"].as_str().unwrap().to_string();

    // 先建立 SSE 连接（订阅在响应返回前已建立），再驱动抓取；
    // broadcast 缓冲保证期间的通知不丢，最终 done 事件使流正常关闭
    let app = build_app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/exports/{task_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    drive(&state, &fixture_fetcher()).await;

    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("event: progress"), "body: {body}");
    assert!(body.contains("event: done"), "body: {body}");
    assert!(body.contains("\"items_count\":28"), "body: {body}");
}

#[tokio::test]
async fn sse_auth_via_query_param_for_eventsource() {
    let (_dir, state) = setup_with_token("s3cret").await;
    let records = parse_items(&fixture("music_collect_p1.html"), Category::Music, Status::Collect);
    seed_ok_dimension(&state, Category::Music, records).await;

    // 创建任务需带鉴权头
    let app = build_app(state.clone());
    let (code, v) = request(
        app.clone(),
        Method::POST,
        "/api/v1/exports",
        Some(json!({"user": "wenjun30", "categories": ["music"]}).to_string()),
        Some("s3cret"),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    let task_id = v["task_id"].as_str().unwrap().to_string();

    // EventSource 无法设置 Header → ?token= 兜底
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/exports/{task_id}/events?token=s3cret"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 错误 token 仍拒绝
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/exports/{task_id}/events?token=wrong"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
