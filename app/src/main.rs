//! 微末·豆瓣书影音 macOS 桌面 App——Tauri 壳 + 内嵌 weimo-douban 同步服务器。
//!
//! 架构（全栈单进程）：启动时在回环随机端口拉起 `weimo_douban::server`
//! （API + 抓取/富集/订阅后台任务），前端静态资源（frontend/web 构建产物，
//! vite base=/douban/）经 rust-embed 编进二进制、由同一个 axum 在 /douban/
//! 前缀下服务——URL 结构与线上 weimo.ink/douban/ 完全一致，前端零改动；
//! 窗口直接加载 http://127.0.0.1:<port>/douban/，前后端同源：零 CORS、
//! 零跨进程通信。数据落在 ~/Library/Application Support/ink.weimo.douban/。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod static_files;

use anyhow::{Context, Result};
use clap::Parser as _;
use tauri::menu::Menu;
use tauri::Manager;
use tauri::{WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use weimo_douban::server::config::ServerConfig;

/// 前端静态资源挂载前缀（与生产站点 weimo.ink/douban/ 及 vite base 一致）
const WEB_MOUNT: &str = "/douban";

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "weimo_douban=info".into()),
        )
        .init();

    tauri::Builder::default()
        // 单实例：二次启动聚焦既有窗口——两个进程写同一 SQLite 必须避免
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .menu(|handle| Ok(Menu::default(handle)?))
        .setup(|app| {
            // 先把回环随机端口绑定到手，再开窗口——端口由本进程掌握
            let (listener, port) = tauri::async_runtime::block_on(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .context("绑定回环端口失败")?;
                let port = listener.local_addr()?.port();
                anyhow::Ok((listener, port))
            })?;

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = run_server(handle.clone(), listener).await {
                    eprintln!("本地服务启动失败: {error:#}");
                    handle
                        .dialog()
                        .message(format!("本地服务启动失败：{error:#}"))
                        .kind(MessageDialogKind::Error)
                        .show(move |_| handle.exit(1));
                }
            });

            let url: tauri::Url = format!("http://127.0.0.1:{port}{WEB_MOUNT}/").parse()?;
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("微末·豆瓣书影音")
                .inner_size(1280.0, 860.0)
                .min_inner_size(960.0, 600.0)
                .build()
                .context("创建窗口失败")?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("运行 weimo-douban app 失败");
}

/// 装配并运行内嵌服务器：后台任务 + API 路由 + /douban/ 前端静态资源
async fn run_server(handle: tauri::AppHandle, listener: tokio::net::TcpListener) -> Result<()> {
    let data_dir = handle.path().app_data_dir().context("获取应用数据目录失败")?;

    // 空参数 parse：忽略 Finder 传入的启动参数（clap 对未知参数会报错），
    // 但保留 SYNC_CRAWL_*/SYNC_SUBJECT_* 等环境变量覆盖（从终端启动时可调参）
    let mut config =
        ServerConfig::try_parse_from(["weimo-douban-app"]).context("解析服务器配置失败")?;
    config.bind = listener.local_addr()?.to_string();
    config.data_dir = data_dir;
    // 回环单用户 + 前后端同源：不启用鉴权与 CORS
    config.token = String::new();
    config.cors_origins = String::new();

    let router = weimo_douban::server::build_server(config).await?;
    let router = router.nest_service(WEB_MOUNT, static_files::router());

    let addr = listener.local_addr()?;
    tracing::info!("app 服务就绪: http://{addr}{WEB_MOUNT}/");
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
