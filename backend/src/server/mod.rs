//! 服务器模块——按《服务器API设计.md》《服务器技术选型.md》实现。

pub mod config;
pub mod covers;
pub mod db;
pub mod error;
pub mod facets;
pub mod fetcher;
pub mod routes;
pub mod service;
pub mod subject;
pub mod state;
pub mod worker;

use anyhow::{Context, Result};
use clap::Parser as _;

/// 装配并运行服务器（供 bin/server.rs 调用）
pub async fn run_server() -> Result<()> {
    let config = config::ServerConfig::parse();
    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("创建数据目录失败: {}", config.data_dir.display()))?;

    let pool = db::connect(&config.db_path())
        .await
        .context("数据库初始化失败")?;
    let state = state::AppState::new(config.clone(), pool);

    tokio::spawn(worker::run_worker(
        state.clone(),
        fetcher::ReqwestFetcher::with_cookie(&config.douban_cookie),
    ));
    tokio::spawn(service::run_subscription_loop(
        state.clone(),
        fetcher::ReqwestFetcher::with_cookie(&config.douban_cookie),
    ));
    tokio::spawn(service::run_cleanup_loop(state.clone()));

    if config.subject_enrich {
        tracing::info!(
            "条目元信息富集已启用（rexxar；存量数据可 POST /api/v1/subjects/backfill 回填）"
        );
    }

    let app = routes::build_app(state);
    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .with_context(|| format!("监听 {} 失败", config.bind))?;
    tracing::info!("weimo-douban-server listening on {}", config.bind);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
