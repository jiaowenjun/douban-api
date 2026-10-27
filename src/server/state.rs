//! 应用状态——配置、连接池、worker 唤醒通道、熔断计数、客户端限流。

use sqlx::SqlitePool;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::{broadcast, watch};

use super::config::ServerConfig;

#[derive(Clone)]
pub struct AppState {
    pub config: ServerConfig,
    pub pool: SqlitePool,
    worker_wakeup_tx: watch::Sender<u64>,
    /// 维度进度变更通知（载荷为 task_id），SSE 订阅端按 task_id 过滤
    progress_tx: broadcast::Sender<String>,
    /// 连续失败次数（豆瓣侧）——Arc 以便各 clone 共享熔断状态
    consecutive_failures: Arc<AtomicU32>,
    /// 熔断截止时间（epoch ms）
    blocked_until_ms: Arc<AtomicI64>,
    rate_table: &'static Mutex<HashMap<IpAddr, (u64, u32)>>,
}

impl AppState {
    pub fn new(config: ServerConfig, pool: SqlitePool) -> Self {
        let (worker_wakeup_tx, _rx) = watch::channel(0);
        let (progress_tx, _) = broadcast::channel(256);
        static RATE_TABLE: OnceLock<Mutex<HashMap<IpAddr, (u64, u32)>>> = OnceLock::new();
        Self {
            config,
            pool,
            worker_wakeup_tx,
            progress_tx,
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            blocked_until_ms: Arc::new(AtomicI64::new(0)),
            rate_table: RATE_TABLE.get_or_init(|| Mutex::new(HashMap::new())),
        }
    }

    pub fn worker_wakeup_receiver(&self) -> watch::Receiver<u64> {
        self.worker_wakeup_tx.subscribe()
    }

    pub fn wake_worker(&self) {
        self.worker_wakeup_tx
            .send_modify(|v| *v = v.wrapping_add(1));
    }

    /// 广播某任务进度有更新（无订阅者时发送被忽略，无副作用）
    pub fn notify_progress(&self, task_id: &str) {
        let _ = self.progress_tx.send(task_id.to_string());
    }

    pub fn subscribe_progress(&self) -> broadcast::Receiver<String> {
        self.progress_tx.subscribe()
    }

    // ---- 熔断 ----

    pub fn record_crawl_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    /// 返回是否触发熔断
    pub fn record_crawl_failure(&self) -> bool {
        let n = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= self.config.circuit_threshold {
            let until = ServerConfig::now_ms() + self.config.circuit_cooldown_secs * 1000;
            self.blocked_until_ms.store(until, Ordering::Relaxed);
            self.consecutive_failures.store(0, Ordering::Relaxed);
            tracing::error!(
                consecutive_failures = n,
                cooldown_secs = self.config.circuit_cooldown_secs,
                "豆瓣侧连续失败，熔断开启"
            );
            return true;
        }
        false
    }

    /// 熔断剩余毫秒（0 = 未熔断）
    pub fn circuit_remaining_ms(&self) -> i64 {
        let until = self.blocked_until_ms.load(Ordering::Relaxed);
        (until - ServerConfig::now_ms()).max(0)
    }

    // ---- 客户端限流（固定窗口） ----

    pub fn rate_allow(&self, ip: IpAddr) -> bool {
        let mut table = self.rate_table.lock().unwrap();
        let minute = (ServerConfig::now_ms() / 60_000) as u64;
        let entry = table.entry(ip).or_insert((minute, 0));
        if entry.0 != minute {
            *entry = (minute, 1);
            return true;
        }
        if entry.1 >= self.config.rate_per_minute {
            return false;
        }
        entry.1 += 1;
        true
    }
}
