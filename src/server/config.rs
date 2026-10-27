//! 服务器配置——全部经 clap 参数 + 环境变量（weimo 惯例）。

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "douban-export-server", about = "免登录豆瓣书影音导出服务器")]
pub struct ServerConfig {
    /// 监听地址
    #[arg(long, env = "EXPORT_SERVER_BIND", default_value = "0.0.0.0:8080")]
    pub bind: String,

    /// 数据目录（SQLite 与导出文件）
    #[arg(long, env = "EXPORT_SERVER_DATA_DIR", default_value = "data")]
    pub data_dir: PathBuf,

    /// Bearer Token（留空则不启用鉴权）
    #[arg(long, env = "EXPORT_SERVER_TOKEN", default_value = "")]
    pub token: String,

    /// 允许的 CORS 来源（逗号分隔，留空关闭）
    #[arg(long, env = "EXPORT_SERVER_CORS_ORIGINS", default_value = "")]
    pub cors_origins: String,

    /// 缓存 TTL（秒）
    #[arg(long, env = "EXPORT_CACHE_TTL_SECS", default_value_t = 86_400)]
    pub cache_ttl_secs: i64,

    /// 豆瓣抓取最小间隔（秒）
    #[arg(long, env = "EXPORT_CRAWL_MIN_INTERVAL_SECS", default_value_t = 2.0)]
    pub crawl_min_interval_secs: f64,

    /// 熔断阈值（连续失败维度次数）
    #[arg(long, env = "EXPORT_CIRCUIT_THRESHOLD", default_value_t = 5)]
    pub circuit_threshold: u32,

    /// 熔断冷却（秒）
    #[arg(long, env = "EXPORT_CIRCUIT_COOLDOWN_SECS", default_value_t = 600)]
    pub circuit_cooldown_secs: i64,

    /// 任务记录保留天数
    #[arg(long, env = "EXPORT_TASK_RETENTION_DAYS", default_value_t = 30)]
    pub task_retention_days: i64,

    /// 结果文件保留天数
    #[arg(long, env = "EXPORT_FILE_RETENTION_DAYS", default_value_t = 7)]
    pub file_retention_days: i64,

    /// 队列长度上限（排队中的维度数）
    #[arg(long, env = "EXPORT_QUEUE_LIMIT", default_value_t = 100)]
    pub queue_limit: i64,

    /// 每分钟每 IP 请求上限
    #[arg(long, env = "EXPORT_RATE_PER_MINUTE", default_value_t = 60)]
    pub rate_per_minute: u32,
}

impl ServerConfig {
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("douban_export.db")
    }

    pub fn files_dir(&self) -> PathBuf {
        self.data_dir.join("files")
    }

    /// 现在的 epoch 毫秒
    pub fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}
