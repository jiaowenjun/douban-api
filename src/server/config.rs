//! 服务器配置——全部经 clap 参数 + 环境变量（weimo 惯例）。

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "douban-sync-server", about = "免登录豆瓣书影音同步服务器（抓取入库 + API 查询）")]
pub struct ServerConfig {
    /// 监听地址
    #[arg(long, env = "SYNC_SERVER_BIND", default_value = "0.0.0.0:8080")]
    pub bind: String,

    /// 数据目录（SQLite 数据库）
    #[arg(long, env = "SYNC_SERVER_DATA_DIR", default_value = "data")]
    pub data_dir: PathBuf,

    /// Bearer Token（留空则不启用鉴权）
    #[arg(long, env = "SYNC_SERVER_TOKEN", default_value = "")]
    pub token: String,

    /// 豆瓣登录 Cookie（如 'dbcl2="xxx"'；留空则免登录抓取，隐藏条目不可见）
    #[arg(long, env = "SYNC_DOUBAN_COOKIE", default_value = "")]
    pub douban_cookie: String,

    /// 允许的 CORS 来源（逗号分隔，留空关闭）
    #[arg(long, env = "SYNC_SERVER_CORS_ORIGINS", default_value = "")]
    pub cors_origins: String,

    /// 缓存 TTL（秒）
    #[arg(long, env = "SYNC_CACHE_TTL_SECS", default_value_t = 86_400)]
    pub cache_ttl_secs: i64,

    /// 豆瓣抓取最小间隔（秒）
    #[arg(long, env = "SYNC_CRAWL_MIN_INTERVAL_SECS", default_value_t = 2.0)]
    pub crawl_min_interval_secs: f64,

    /// 熔断阈值（连续失败维度次数）
    #[arg(long, env = "SYNC_CIRCUIT_THRESHOLD", default_value_t = 5)]
    pub circuit_threshold: u32,

    /// 熔断冷却（秒）
    #[arg(long, env = "SYNC_CIRCUIT_COOLDOWN_SECS", default_value_t = 600)]
    pub circuit_cooldown_secs: i64,

    /// 任务记录保留天数（records 表数据不随任务过期）
    #[arg(long, env = "SYNC_TASK_RETENTION_DAYS", default_value_t = 30)]
    pub task_retention_days: i64,

    /// 队列长度上限（排队中的维度数）
    #[arg(long, env = "SYNC_QUEUE_LIMIT", default_value_t = 100)]
    pub queue_limit: i64,

    /// 每分钟每 IP 请求上限
    #[arg(long, env = "SYNC_RATE_PER_MINUTE", default_value_t = 60)]
    pub rate_per_minute: u32,

    /// 订阅默认 RSS 增量轮询间隔（秒），创建订阅时未指定则用此值
    #[arg(long, env = "SYNC_SUB_RSS_INTERVAL_SECS", default_value_t = 600)]
    pub sub_rss_interval_secs: i64,

    /// 订阅默认全量同步间隔（秒）
    #[arg(long, env = "SYNC_SUB_FULL_INTERVAL_SECS", default_value_t = 86_400)]
    pub sub_full_interval_secs: i64,
}

impl ServerConfig {
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("douban_export.db")
    }

    /// 现在的 epoch 毫秒
    pub fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}
