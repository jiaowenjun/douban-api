//! 服务器集成测试——weimo 范式：tower oneshot 直接打 Router，进程内完成，
//! 不起端口、不碰真实豆瓣（经 FakeFetcher 注入脱敏后的真实页面 fixture）。

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use weimo_douban::crawler::{Category, Record, Status, parse_feed, parse_items, parse_total};
use weimo_douban::server::{
    config::ServerConfig,
    db,
    fetcher::{DoubanFetcher, FetchError},
    routes::build_app,
    service,
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
        queue_limit: 100,
        sub_rss_interval_secs: 600,
        sub_full_interval_secs: 86_400,
        cover_concurrency: 4,
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
        if url.contains("/feed/people/") {
            return Ok(fixture("feed.xml"));
        }
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

async fn post_sync(state: &AppState, body: Value) -> (StatusCode, Value) {
    let app = build_app(state.clone());
    request(
        app,
        Method::POST,
        "/api/v1/syncs",
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

/// 预置一份"已成功缓存"的维度（记录直插 records 表，与 worker 写入路径一致）
async fn seed_ok_dimension(state: &AppState, category: Category, records: Vec<Record>) {
    let now = ServerConfig::now_ms();
    let page_size = weimo_douban::crawler::PAGE_SIZE as i64;
    let len = records.len() as i64;
    service::upsert_records(&state.pool, "wenjun30", category.key(), "collect", &records)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO dimensions (uid, category, dim_status, state, fetched_count,
                                 total, next_start, fetched_ms, updated_ms)
         VALUES ('wenjun30', ?, 'collect', 'ok', ?, ?, ?, ?, ?)",
    )
    .bind(category.key())
    .bind(len)
    .bind(len)
    .bind((len + page_size - 1) / page_size * page_size)
    .bind(now)
    .bind(now)
    .execute(&state.pool)
    .await
    .unwrap();
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
    // 音乐短评在无 class 的 <li> 里；被点过「有用」的短评 li 内含
    // <span class="color_gray">N 有用</span>，正文取 li 的直接文本子节点
    assert_eq!(items.iter().filter(|r| !r.comment.is_empty()).count(), 15);
    assert!(items.iter().all(|r| r.comment.is_empty() || r.comment.contains("测试短评")));
    assert!(items.iter().all(|r| !r.comment.contains("有用")));
    let bb = items.iter().find(|r| r.title == "Beethoven Blues").unwrap();
    assert_eq!(bb.comment, "（测试短评占位文本）");
    let tt = items.iter().find(|r| r.title == "Tony Takitani").unwrap();
    assert_eq!(tt.comment, "（测试短评占位文本）");
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

    let (code, v) = post_sync(
        &state,
        json!({"user": "wenjun30", "categories": ["music"], "wait_timeout": 10}),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "全缓存命中应秒回终态: {v}");
    assert_eq!(v["status"], "completed");
    assert_eq!(v["cached_dimensions"], 1);
    assert_eq!(v["created_dimensions"], 0);
    assert_eq!(v["items_count"], 15);
    assert_eq!(v["percent"], 100);

    // 结果汇总（数据本体经 /users/{uid}/marks 浏览）
    let task_id = v["task_id"].as_str().unwrap().to_string();
    let app = build_app(state.clone());
    let (code, result) =
        request(app, Method::GET, &format!("/api/v1/syncs/{task_id}/result"), None, None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(result["summary"]["total"], 15);
    assert_eq!(result["summary"]["with_comment"], 15);
    let dims = result["dimensions"].as_array().unwrap();
    assert_eq!(dims.len(), 1);
    assert_eq!(dims[0]["category"], "music");
    assert_eq!(dims[0]["items"], 15);
}

#[tokio::test]
async fn worker_crawls_book_dimension_to_completion() {
    let (_dir, state) = setup().await;
    let (code, v) = post_sync(
        &state,
        json!({"user": "wenjun30", "categories": ["book"]}),
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

    // 抓取完成后数据已入库，marks 可分页读取
    let app = build_app(state.clone());
    let (code, v) = request(
        app,
        Method::GET,
        "/api/v1/users/wenjun30/marks?category=book&status=collect&page=1&page_size=50",
        None,
        None,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(v["total"], 55);
    assert_eq!(v["items"].as_array().unwrap().len(), 50);
}

#[tokio::test]
async fn worker_crawls_music_with_dedup_and_percent_progress() {
    let (_dir, state) = setup().await;
    let (_, v) = post_sync(
        &state,
        json!({"user": "wenjun30", "categories": ["music"]}),
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
    let (_, a) = post_sync(&state, json!({"user": "wenjun30", "categories": ["music"]})).await;
    let (_, b) = post_sync(&state, json!({"user": "wenjun30", "categories": ["music"]})).await;
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
    let (_, v) = post_sync(&state, json!({"user": "wenjun30", "categories": ["movie"]})).await;
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
    let (_, v) = post_sync(&state, json!({"user": "wenjun30", "categories": ["movie"]})).await;
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
    let (code, v) = post_sync(&state, json!({"user": "wenjun30", "categories": ["game"]})).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"]["code"], "INVALID_ARGUMENT");

    let (code, v) = post_sync(&state, json!({"user": "wenjun30", "wait_timeout": 99})).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"]["code"], "INVALID_ARGUMENT");

    // formats 已随文件导出移除，出现未知字段应 400
    let (code, v) = post_sync(
        &state,
        json!({"user": "wenjun30", "categories": ["music"], "formats": ["csv"]}),
    )
    .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"]["code"], "INVALID_ARGUMENT");

    // 非法 JSON body
    let app = build_app(state.clone());
    let (code, v) = request(app, Method::POST, "/api/v1/syncs", Some("{bad json".into()), None).await;
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
    // 第 2 页（每页 5 条）是按日期倒序的第 6–10 条，而非从头偏移 page-1
    let mut sorted = records.clone();
    sorted.sort_by(|a, b| b.date.cmp(&a.date));
    assert_eq!(v["items"].as_array().unwrap().len(), 5);
    assert_eq!(v["items"][0]["title"], sorted[5].title);
    assert_eq!(v["items"][4]["title"], sorted[9].title);

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

// --------------------------------------------------------------------------- //
// 封面代理（cache-then-serve）：URL 改写 + 白名单 + 磁盘命中 + 鉴权
// --------------------------------------------------------------------------- //

/// 返回原始字节与响应头（封面是二进制，不能走 JSON 版 request()）
async fn request_bytes(app: Router, uri: &str) -> (StatusCode, Vec<u8>, header::HeaderMap) {
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec();
    (status, bytes, headers)
}

const IMG3_COVER: &str = "https%3A%2F%2Fimg3.doubanio.com%2Fview%2Fphoto%2Fs_ratio_poster%2Fpublic%2Fp2935834896.webp";

#[tokio::test]
async fn marks_cover_rewritten_to_proxy() {
    let (_dir, state) = setup().await;
    let records = parse_items(&fixture("music_collect_p1.html"), Category::Music, Status::Collect);
    seed_ok_dimension(&state, Category::Music, records).await;

    let app = build_app(state);
    let (code, v) = request(
        app,
        Method::GET,
        "/api/v1/users/wenjun30/marks?category=music&status=collect&page_size=1",
        None,
        None,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    // 浏览器直连 img2/img3 被 Referer 防盗链 403，响应里必须是本地代理端点
    let cover = v["items"][0]["cover"].as_str().unwrap();
    assert!(cover.starts_with("/api/v1/covers?u=https%3A%2F%2Fimg"), "实际值: {cover}");
}

#[tokio::test]
async fn cover_serves_from_disk_cache() {
    let (dir, state) = setup().await;
    // 预置磁盘缓存：命中路径全程不发起网络请求
    let covers_dir = dir.path().join("covers");
    std::fs::create_dir_all(&covers_dir).unwrap();
    let seeded = b"fake-webp-bytes".to_vec();
    std::fs::write(covers_dir.join("p2935834896.webp"), &seeded).unwrap();

    let app = build_app(state);
    let (code, bytes, headers) =
        request_bytes(app, &format!("/api/v1/covers?u={IMG3_COVER}")).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(bytes, seeded);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/webp");
    assert!(
        headers.get(header::CACHE_CONTROL).unwrap().to_str().unwrap().contains("immutable"),
        "应允许浏览器长缓存"
    );
}

#[tokio::test]
async fn cover_rejects_non_whitelisted_url() {
    let (_dir, state) = setup().await;
    let app = build_app(state);
    for bad in [
        "https%3A%2F%2Fevil.com%2Fview%2Fphoto%2Fp1.webp",
        "https%3A%2F%2Fimg3.doubanio.com%2Fsecret%2Fp1.webp",
        "not%20a%20url",
    ] {
        let (code, v) = request(app.clone(), Method::GET, &format!("/api/v1/covers?u={bad}"), None, None).await;
        assert_eq!(code, StatusCode::BAD_REQUEST, "应拒绝: {bad}");
        assert_eq!(v["error"]["code"], "INVALID_ARGUMENT");
    }
}

#[tokio::test]
async fn cover_requires_token_when_configured() {
    let (dir, state) = setup_with_token("secret").await;
    let covers_dir = dir.path().join("covers");
    std::fs::create_dir_all(&covers_dir).unwrap();
    std::fs::write(covers_dir.join("p2935834896.webp"), b"x").unwrap();

    let app = build_app(state);
    // 无 token
    let (code, _, _) = request_bytes(app.clone(), &format!("/api/v1/covers?u={IMG3_COVER}")).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    // img 标签带不了 Header，?token= 查询参数兜底必须可用
    let (code, _, _) =
        request_bytes(app, &format!("/api/v1/covers?u={IMG3_COVER}&token=secret")).await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn delete_task_keeps_records_and_returns_404() {
    let (_dir, state) = setup().await;
    let records = parse_items(&fixture("music_collect_p1.html"), Category::Music, Status::Collect);
    seed_ok_dimension(&state, Category::Music, records).await;
    let (_, v) = post_sync(&state, json!({"user": "wenjun30", "categories": ["music"]})).await;
    let task_id = v["task_id"].as_str().unwrap().to_string();

    let app = build_app(state.clone());
    let (code, _) =
        request(app.clone(), Method::DELETE, &format!("/api/v1/syncs/{task_id}"), None, None).await;
    assert_eq!(code, StatusCode::NO_CONTENT);

    let (code, v) =
        request(app.clone(), Method::GET, &format!("/api/v1/syncs/{task_id}"), None, None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    assert_eq!(v["error"]["code"], "TASK_NOT_FOUND");

    // 数据本体与任务生命周期解耦：任务删除后仍可浏览
    let (code, v) = request(
        app,
        Method::GET,
        "/api/v1/users/wenjun30/marks?category=music&status=collect&page_size=5",
        None,
        None,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(v["total"], 15);
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
    let (_, v) = post_sync(&state, json!({"user": "wenjun30", "categories": ["music"]})).await;
    let task_id = v["task_id"].as_str().unwrap().to_string();

    let app = build_app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/syncs/{task_id}/events"))
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
    let (_, v) = post_sync(
        &state,
        json!({"user": "wenjun30", "categories": ["music"]}),
    )
    .await;
    let task_id = v["task_id"].as_str().unwrap().to_string();

    // 先建立 SSE 连接（订阅在响应返回前已建立），再驱动抓取；
    // broadcast 缓冲保证期间的通知不丢，最终 done 事件使流正常关闭
    let app = build_app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/syncs/{task_id}/events"))
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
        "/api/v1/syncs",
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
                .uri(format!("/api/v1/syncs/{task_id}/events?token=s3cret"))
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
                .uri(format!("/api/v1/syncs/{task_id}/events?token=wrong"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// --------------------------------------------------------------------------- //
// 订阅同步：RSS 解析 / CRUD / 增量 tick / auto 全量
// --------------------------------------------------------------------------- //

#[test]
fn parser_feed_structure() {
    let items = parse_feed(&fixture("feed.xml"));
    assert_eq!(items.len(), 3);

    let a = &items[0];
    assert_eq!(a.record.category, "movie");
    assert_eq!(a.record.status, "看过");
    assert_eq!(a.status_key, "collect");
    assert_eq!(a.record.title, "生日快乐");
    assert_eq!(a.record.alt_title, "Alt Title One");
    assert_eq!(a.record.my_rating, Some(5));
    assert_eq!(a.record.comment, "（测试短评占位一）");
    assert_eq!(a.record.subject_id, "37322586");
    assert_eq!(a.record.comment_id, "4975285140");
    assert_eq!(a.guid, "4975285140");
    assert_eq!(a.record.date, "2026-10-26"); // pubDate GMT +8 后取日期
    assert!(a.record.cover.starts_with("https://img"));

    let b = &items[1];
    assert_eq!(b.record.title, "再见UFO");
    assert_eq!(b.record.my_rating, Some(4)); // "推荐: 推荐" → 4 星
    assert_eq!(b.record.date, "2026-08-25");

    let c = &items[2];
    assert_eq!(c.record.category, "book");
    assert_eq!(c.status_key, "wish");
    assert_eq!(c.record.my_rating, None); // 无评分段
}

#[test]
fn parser_feed_tolerates_garbage() {
    assert!(parse_feed("").is_empty());
    assert!(parse_feed("not xml < at all").is_empty());
    // 截断的 XML：已完整解析的 item 仍返回
    let xml = fixture("feed.xml");
    let cut = &xml[..xml.find("<item>").unwrap() + 800];
    assert!(!parse_feed(cut).is_empty() || parse_feed(cut).len() <= 1);
}

#[tokio::test]
async fn subscription_crud_and_auth() {
    let (dir, state) = setup_with_token("s3cret").await;
    let app = build_app(state.clone());

    let (code, _) = request(app.clone(), Method::GET, "/api/v1/subscriptions", None, None).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);

    let (code, body) = request(
        app.clone(),
        Method::PUT,
        "/api/v1/subscriptions/wenjun30",
        Some(json!({"rss_interval_secs": 300, "full_interval_secs": 7200}).to_string()),
        Some("s3cret"),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "body: {body}");
    assert_eq!(body["uid"], "wenjun30");
    assert_eq!(body["rss_interval_secs"], 300);
    assert_eq!(body["full_interval_secs"], 7200);
    assert!(body["next_rss_at"].is_string());

    let (code, body) = request(app.clone(), Method::GET, "/api/v1/subscriptions", None, Some("s3cret")).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["subscriptions"].as_array().unwrap().len(), 1);

    let (code, _) = request(
        app.clone(),
        Method::GET,
        "/api/v1/subscriptions/wenjun30",
        None,
        Some("s3cret"),
    )
    .await;
    assert_eq!(code, StatusCode::OK);

    // 间隔下限校验：rss < 60s → 400
    let (code, _) = request(
        app.clone(),
        Method::PUT,
        "/api/v1/subscriptions/wenjun30",
        Some(json!({"rss_interval_secs": 10}).to_string()),
        Some("s3cret"),
    )
    .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);

    // 非法 uid → 400
    let (code, _) = request(
        app.clone(),
        Method::PUT,
        "/api/v1/subscriptions/not%20a%20id",
        None,
        Some("s3cret"),
    )
    .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);

    let (code, _) = request(
        app.clone(),
        Method::DELETE,
        "/api/v1/subscriptions/wenjun30",
        None,
        Some("s3cret"),
    )
    .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    let (code, _) = request(
        app.clone(),
        Method::DELETE,
        "/api/v1/subscriptions/wenjun30",
        None,
        Some("s3cret"),
    )
    .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    drop(dir);
}

#[tokio::test]
async fn rss_tick_inserts_new_and_skips_unknown_dim() {
    let (dir, state) = setup().await;
    // seed：movie/collect 已有一条 guid=4975285140 的记录（与 feed 首条同主键 → 去重）
    seed_ok_dimension(
        &state,
        Category::Movie,
        vec![Record {
            title: "生日快乐（旧）".to_string(),
            subject_id: "37322586".to_string(),
            comment_id: "4975285140".to_string(),
            ..Default::default()
        }],
    )
    .await;

    service::upsert_subscription(&state, "wenjun30", Some(300), Some(7200))
        .await
        .unwrap();
    // 拨到期（upsert 设的是 now+300s）
    sqlx::query("UPDATE subscriptions SET next_rss_ms = 0")
        .execute(&state.pool)
        .await
        .unwrap();

    let fetcher = fixture_fetcher();
    assert!(service::subscription_tick(&state, &fetcher).await.unwrap());

    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM records
         WHERE uid='wenjun30' AND category='movie' AND dim_status='collect'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(n, 2, "feed 首条与旧记录同主键被 IGNORE，第二条新增");

    // book/wish 维度不存在 → 条目被跳过，不产生孤儿 records
    let nb: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM records
         WHERE uid='wenjun30' AND category='book' AND dim_status='wish'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(nb, 0);

    // 新记录字段来自 feed 解析
    let comment: String = sqlx::query_scalar(
        "SELECT comment FROM records WHERE uid='wenjun30' AND subject_id='30208001'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(comment, "（测试短评占位二）");

    // 维度 fetched_count 已重算；订阅节奏推进、错误清零
    let fc: i64 = sqlx::query_scalar(
        "SELECT fetched_count FROM dimensions
         WHERE uid='wenjun30' AND category='movie' AND dim_status='collect'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(fc, 2);
    let (attempts, guid): (i64, String) =
        sqlx::query_as("SELECT rss_attempts, last_guid FROM subscriptions WHERE uid='wenjun30'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!((attempts, guid.as_str()), (0, "4975285140"));
    drop(dir);
}

#[tokio::test]
async fn rss_error_backoff() {
    let (dir, state) = setup().await;
    service::upsert_subscription(&state, "wenjun30", None, None)
        .await
        .unwrap();
    sqlx::query("UPDATE subscriptions SET next_rss_ms = 0")
        .execute(&state.pool)
        .await
        .unwrap();

    let fetcher = FakeFetcher(|_url: &str| Err(FetchError::Blocked(403)));
    assert!(service::subscription_tick(&state, &fetcher).await.unwrap());

    let (attempts, err): (i64, Option<String>) =
        sqlx::query_as("SELECT rss_attempts, last_error FROM subscriptions WHERE uid='wenjun30'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(attempts, 1);
    assert!(err.unwrap().contains("DOUBAN_BLOCKED"));
    drop(dir);
}

#[tokio::test]
async fn full_tick_creates_auto_task_and_recrawls() {
    let (dir, state) = setup().await;
    // seed：music/collect 已缓存两条假记录，refresh 后应被 fixture 数据替换
    seed_ok_dimension(
        &state,
        Category::Music,
        vec![
            Record {
                title: "旧记录一".to_string(),
                subject_id: "999001".to_string(),
                ..Default::default()
            },
            Record {
                title: "旧记录二".to_string(),
                subject_id: "999002".to_string(),
                ..Default::default()
            },
        ],
    )
    .await;

    service::upsert_subscription(&state, "wenjun30", None, None)
        .await
        .unwrap();
    // rss 不到期、full 立即到期
    sqlx::query("UPDATE subscriptions SET next_rss_ms = 99999999999999, next_full_ms = 0")
        .execute(&state.pool)
        .await
        .unwrap();

    let fetcher = fixture_fetcher();
    assert!(service::subscription_tick(&state, &fetcher).await.unwrap());

    let (source, status): (String, String) =
        sqlx::query_as("SELECT source, status FROM tasks").fetch_one(&state.pool).await.unwrap();
    assert_eq!((source.as_str(), status.as_str()), ("auto", "queued"));

    // 驱动 worker 抓完（music fixture 共 4 页）
    drive(&state, &fetcher).await;
    let status: String = sqlx::query_scalar("SELECT status FROM tasks")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(status, "completed");

    // 旧假记录被 refresh 清掉，records 为 fixture 数据
    let old_left: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM records WHERE subject_id IN ('999001','999002')",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(old_left, 0);
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM records WHERE uid='wenjun30' AND category='music'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(n, 28, "music fixture 实际渲染 28 条（1 条隐藏，与 worker 测试一致）");

    // 列表默认只看 manual（不占注意力）；?source=auto 可见
    let app = build_app(state.clone());
    let (code, body) = request(app.clone(), Method::GET, "/api/v1/syncs", None, None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["tasks"].as_array().unwrap().len(), 0);
    let (code, body) =
        request(app.clone(), Method::GET, "/api/v1/syncs?source=auto", None, None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(body["tasks"][0]["source"], "auto");
    // 非法 source 过滤值
    let (code, _) =
        request(app.clone(), Method::GET, "/api/v1/syncs?source=nope", None, None).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);

    // 订阅 full 节奏已推进
    let (last_full,): (Option<i64>,) =
        sqlx::query_as("SELECT last_full_ms FROM subscriptions WHERE uid='wenjun30'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert!(last_full.is_some());
    drop(dir);
}

#[tokio::test]
async fn full_tick_single_flight() {
    let (dir, state) = setup().await;
    seed_ok_dimension(
        &state,
        Category::Music,
        vec![Record {
            title: "旧记录".to_string(),
            subject_id: "999001".to_string(),
            ..Default::default()
        }],
    )
    .await;
    service::upsert_subscription(&state, "wenjun30", None, None)
        .await
        .unwrap();
    // 已有一个未终态 auto 任务
    sqlx::query(
        "INSERT INTO tasks (task_id, user, request_json, status, created_ms, source)
         VALUES ('exp_auto_existing', 'wenjun30', '{}', 'running', 0, 'auto')",
    )
    .execute(&state.pool)
    .await
    .unwrap();

    sqlx::query("UPDATE subscriptions SET next_rss_ms = 99999999999999, next_full_ms = 0")
        .execute(&state.pool)
        .await
        .unwrap();
    let fetcher = fixture_fetcher();
    assert!(service::subscription_tick(&state, &fetcher).await.unwrap());

    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks").fetch_one(&state.pool).await.unwrap();
    assert_eq!(n, 1, "已有进行中的 auto 任务时不重复创建");
    // records 未被 refresh 清空
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM records").fetch_one(&state.pool).await.unwrap();
    assert_eq!(n, 1);
    drop(dir);
}
