//! 微末·豆瓣书影音 macOS 桌面 App——Tauri 壳 + 内嵌 weimo-douban 同步服务器。
//!
//! 架构（全栈单进程）：启动时在回环随机端口拉起 `weimo_douban::server`
//! （API + 抓取/富集/订阅后台任务），前端静态资源（frontend/web 构建产物，
//! vite base=/douban/）经 rust-embed 编进二进制、由同一个 axum 在 /douban/
//! 前缀下服务（与线上 weimo.ink/douban/ 的 URL 结构一致，前端零改动），
//! 窗口直接加载 http://127.0.0.1:<port>/douban/ ——前后端同源，零 CORS、
//! 零跨进程。数据落在 ~/Library/Application Support/ink.weimo.douban/。
//!
//! 桌面形态三件套：
//! - 首启种子：本地无 douban_export.db 时从远端服务器拉全库快照（SYNC_SEED=
//!   ask|auto|off），免去全量重抓；窗口在种子完成后才创建；
//! - 菜单栏常驻：LSUIElement（无 Dock 图标）+ 托盘菜单，关窗=隐藏，
//!   进程与订阅轮询/富集后台任务持续推进；
//! - 自动更新：tauri-plugin-updater，manifest 默认走 GitHub Releases
//!   latest-download 重定向（SYNC_UPDATER_URL 可覆盖），启动静默自检 +
//!   托盘手动检查，minisign 签名校验后安装重启。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod static_files;

use anyhow::{Context, Result};
use clap::Parser as _;
use std::path::Path;
use std::time::Duration;
use tauri::menu::{Menu, MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;
use weimo_douban::server::config::ServerConfig;
use weimo_douban::server::seed;

/// 前端静态资源挂载前缀（与生产站点 weimo.ink/douban/ 及 vite base 一致）
const WEB_MOUNT: &str = "/douban";

/// 更新 manifest 默认地址：GitHub Releases 的 latest/download 重定向（仓库
/// 公开，免鉴权）；SYNC_UPDATER_URL 可覆盖（自托管 / 本地联调）
const UPDATER_ENDPOINT: &str =
    "https://github.com/jiaowenjun/weimo-douban/releases/latest/download/latest.json";

/// 首启种子默认源：txy 生产服务器（需服务器 ≥0.3.1 提供 /server/snapshot）
const SEED_URL_DEFAULT: &str = "https://weimo.ink/douban/api/v1/server/snapshot";

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "weimo_douban=info,weimo_douban_app=info".into()),
        )
        .init();
    tracing::info!("weimo-douban app v{} 启动", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        // 单实例：二次启动聚焦既有窗口——两个进程写同一 SQLite 必须避免
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_updater::Builder::new()
                .pubkey(include_str!("../updater.pubkey").to_string())
                .build(),
        )
        .menu(|handle| Ok(Menu::default(handle)?))
        // 菜单栏常驻：关窗=隐藏不退出，订阅轮询/富集后台任务继续推进
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            build_tray(app.handle())?;

            // 先把回环随机端口绑定到手（backlog 生效，后续连接不会拒绝），
            // 服务装配/首启种子在后台任务里做，完成后才创建窗口
            let listener = tauri::async_runtime::block_on(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .context("绑定回环端口失败")?;
                anyhow::Ok(listener)
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

            // 启动后静默自检更新（手动检查走托盘菜单）
            let updater_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                check_update(updater_handle, false).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("运行 weimo-douban app 失败");
}

/// 装配并运行内嵌服务器：首启种子 → 后台任务 + API 路由 + /douban/ 静态
/// 资源 → 创建主窗口（服务就绪前开窗只会砸在一个空端口上）
async fn run_server(handle: tauri::AppHandle, listener: tokio::net::TcpListener) -> Result<()> {
    let data_dir = handle.path().app_data_dir().context("获取应用数据目录失败")?;

    maybe_seed(&handle, &data_dir).await;

    // 空参数 parse：忽略 Finder 传入的启动参数（clap 对未知参数会报错），
    // 但保留 SYNC_CRAWL_*/SYNC_SUBJECT_* 等环境变量覆盖（从终端启动时可调参）
    let mut config = ServerConfig::try_parse_from(["weimo-douban-app"])
        .context("解析服务器配置失败")?;
    config.bind = listener.local_addr()?.to_string();
    config.data_dir = data_dir;
    // 回环单用户 + 前后端同源：不启用鉴权与 CORS
    config.token = String::new();
    config.cors_origins = String::new();

    let router = weimo_douban::server::build_server(config).await?;
    let router = router.nest_service(WEB_MOUNT, static_files::router());

    let addr = listener.local_addr()?;
    tracing::info!("app 服务就绪: http://{addr}{WEB_MOUNT}/");
    // 窗口创建派发到主线程：macOS 上从后台线程建窗会丢失尺寸/定位
    // （实测落成 500×500 异位窗），tao 的窗口操作必须在主线程
    let window_handle = handle.clone();
    let port = addr.port();
    handle
        .run_on_main_thread(move || create_main_window(&window_handle, port))
        .ok();

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}

fn create_main_window(handle: &tauri::AppHandle, port: u16) {
    let url: tauri::Url = format!("http://127.0.0.1:{port}{WEB_MOUNT}/")
        .parse()
        .expect("窗口地址构造失败");
    if let Err(error) = WebviewWindowBuilder::new(handle, "main", WebviewUrl::External(url))
        .title("微末·豆瓣书影音")
        .inner_size(1280.0, 860.0)
        .min_inner_size(960.0, 600.0)
        .focused(true)
        .build()
    {
        // 窗口失败不致命（托盘仍可重试打开），但基本不可能发生
        tracing::error!("创建主窗口失败: {error:#}");
    } else if let Some(window) = handle.get_webview_window("main") {
        let _ = window.center();
        let _ = window.set_focus();
        tracing::info!(
            "主窗口已创建: outer={:?} pos={:?}",
            window.outer_size(),
            window.outer_position()
        );
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

// --------------------------------------------------------------------------- //
// 首启种子
// --------------------------------------------------------------------------- //

/// SYNC_SEED=ask（默认，弹窗询问）| auto（直接拉）| off（禁用）；
/// SYNC_SEED_URL / SYNC_SEED_TOKEN 覆盖默认源（token 亦可在构建期经
/// SYNC_SEED_TOKEN 注入，语义同 web 端 VITE_API_TOKEN：防滥用不保密）
async fn maybe_seed(handle: &tauri::AppHandle, data_dir: &Path) {
    let policy = env_or("SYNC_SEED", "ask");
    if policy == "off" || !seed::is_cold(data_dir) {
        return;
    }
    let url = env_or("SYNC_SEED_URL", SEED_URL_DEFAULT);
    if policy != "auto" {
        let go = dialog_ask(
            handle,
            "首次启动",
            &format!(
                "本地暂无数据，是否从服务器拉取数据快照？\n\n源：{url}\n\n约 10MB，含已同步用户与条目富集缓存；跳过则首次同步需完整抓取（较慢）。"
            ),
        )
        .await;
        if !go {
            tracing::info!("用户跳过首启种子");
            return;
        }
    }
    tracing::info!("开始拉取首启种子: {url}");
    let token = env_or(
        "SYNC_SEED_TOKEN",
        option_env!("SYNC_SEED_TOKEN").unwrap_or(""),
    );
    match seed::fetch_seed(&url, &token, data_dir).await {
        Ok(true) => {
            dialog_info(handle, "首启种子完成，服务器数据已导入本地");
        }
        Ok(false) => {
            tracing::warn!("远端未提供快照（服务器需 ≥0.3.1），回落全量抓取");
            if policy != "auto" {
                dialog_info(handle, "远端未提供快照（服务器需 ≥0.3.1），将回落全量抓取");
            }
        }
        Err(error) => {
            tracing::warn!("首启种子失败: {error:#}");
            if policy != "auto" {
                dialog_info(handle, &format!("首启种子失败：{error:#}\n将回落全量抓取"));
            }
        }
    }
}

// --------------------------------------------------------------------------- //
// 托盘（菜单栏常驻）
// --------------------------------------------------------------------------- //

fn build_tray(handle: &tauri::AppHandle) -> Result<()> {
    let open = MenuItem::with_id(handle, "open", "打开主窗口", true, None::<&str>)?;
    let update = MenuItem::with_id(handle, "update", "检查更新…", true, None::<&str>)?;
    let quit = MenuItem::with_id(handle, "quit", "退出", true, None::<&str>)?;
    let menu = MenuBuilder::new(handle)
        .item(&open)
        .item(&update)
        .separator()
        .item(&quit)
        .build()?;

    let icon = handle
        .default_window_icon()
        .context("应用图标缺失，无法创建托盘")?
        .clone();
    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("微末·豆瓣托盘")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "update" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    check_update(app, true).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        // 左键点托盘 = 开/聚焦主窗口（菜单只走右键）
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(handle)?;
    Ok(())
}

// --------------------------------------------------------------------------- //
// 自动更新
// --------------------------------------------------------------------------- //

/// 构建更新器：默认端点为 UPDATER_ENDPOINT（GitHub Releases latest-download
/// 重定向），SYNC_UPDATER_URL 可覆盖（自托管/本地联调；注意 release 构建仅
/// 接受 https，http 需在 tauri.conf 开 dangerousInsecureTransportProtocol）
fn build_updater(handle: &tauri::AppHandle) -> Result<tauri_plugin_updater::Updater> {
    let mut builder = handle.updater_builder();
    let endpoint = env_or("SYNC_UPDATER_URL", UPDATER_ENDPOINT);
    if endpoint != UPDATER_ENDPOINT {
        builder = builder.endpoints(vec![endpoint.parse()?])?;
    } else {
        builder = builder.endpoints(vec![UPDATER_ENDPOINT.parse()?])?;
    }
    Ok(builder.build()?)
}

/// 检查并（经确认）安装更新。manual=true 为托盘手动触发（无更新/失败均弹
/// 窗告知）；启动自检静默失败。SYNC_UPDATE_AUTO=1 跳过确认直接装（无人值守）
async fn check_update(handle: tauri::AppHandle, manual: bool) {
    let updater = match build_updater(&handle) {
        Ok(updater) => updater,
        Err(error) => {
            tracing::warn!("更新器初始化失败: {error:#}");
            if manual {
                dialog_error(&handle, &format!("更新器初始化失败：{error:#}"));
            }
            return;
        }
    };
    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => {
            if manual {
                dialog_info(&handle, "已是最新版本");
            }
            return;
        }
        Err(error) => {
            tracing::warn!("检查更新失败: {error}");
            if manual {
                dialog_error(&handle, &format!("检查更新失败：{error}"));
            }
            return;
        }
    };
    let version = update.version.clone();
    tracing::info!("发现新版本 v{version}");
    if env_or("SYNC_UPDATE_AUTO", "") != "1" {
        let go = dialog_ask(
            &handle,
            "发现新版本",
            &format!("是否更新到 v{version}？\n\n下载完成后 App 将自动重启。"),
        )
        .await;
        if !go {
            return;
        }
    }
    if let Err(error) = update
        .download_and_install(|_, _| {}, || {
            tracing::info!("更新包下载完成，开始安装");
        })
        .await
    {
        tracing::error!("更新安装失败: {error}");
        dialog_error(&handle, &format!("更新安装失败：{error}"));
        return;
    }
    tracing::info!("已更新到 v{version}，重启生效");
    if env_or("SYNC_UPDATE_AUTO", "") == "1" {
        // 无人值守：不给确认对话框留时间
        tokio::time::sleep(Duration::from_millis(300)).await;
    } else {
        // 阻塞展示结果，用户知晓后再重启（show 是即发即忘，restart 会立刻杀掉它）
        let _ = handle
            .dialog()
            .message(format!("已更新到 v{version}，即将重启"))
            .blocking_show();
    }
    handle.restart();
}

// --------------------------------------------------------------------------- //
// 原生对话框薄封装（blocking 版：均从后台任务线程调用，切勿在主线程用）
// --------------------------------------------------------------------------- //

async fn dialog_ask(handle: &tauri::AppHandle, title: &str, message: &str) -> bool {
    let (send, receive) = tokio::sync::oneshot::channel();
    handle
        .dialog()
        .message(message)
        .title(title)
        .buttons(MessageDialogButtons::OkCancelCustom("确定".into(), "取消".into()))
        .show(move |ok| {
            let _ = send.send(ok);
        });
    receive.await.unwrap_or(false)
}

fn dialog_info(handle: &tauri::AppHandle, message: &str) {
    handle.dialog().message(message).show(|_| {});
}

fn dialog_error(handle: &tauri::AppHandle, message: &str) {
    handle
        .dialog()
        .message(message)
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}
