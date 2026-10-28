//! weimo-douban-server —— 免登录豆瓣书影音同步服务器
//!
//! 用法：weimo-douban-server [--bind 0.0.0.0:8080] [--data-dir data] ...
//! 全部参数均可经环境变量配置（见 config 模块）。

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "weimo_douban=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    weimo_douban::server::run_server().await
}
