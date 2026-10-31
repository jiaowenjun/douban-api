//! 首启种子——从远端同步服务器拉全库快照（GET /api/v1/server/snapshot），
//! 落盘为本地 douban_export.db。供桌面 App（仓库 app/）冷启动时调用：
//! 免去全量重抓（wenjun30 规模全量抓取 + 富集以天计），落盘后订阅/任务
//! 照常增量补差。covers 不随快照走（本地封面代理按需回源自愈）。
//!
//! 语义约定：Ok(true)=已种子落盘；Ok(false)=远端未提供快照（404，如服务器
//! 版本过旧未部署该端点），调用方静默回落常规冷启动；Err=下载/校验失败
//! （同样回落，但给出原因）。

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::time::Duration;

/// SQLite 文件魔数（前 16 字节，校验下载物确为 SQLite 库而非错误页 HTML）
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// 冷启动判定：数据库文件不存在（从未种子、也从未跑过任何同步）
pub fn is_cold(data_dir: &Path) -> bool {
    !data_dir.join("douban_export.db").exists()
}

/// 拉取远端快照并原子落盘到 `<data_dir>/douban_export.db`。
/// 调用方须保证此刻本地服务尚未启动（无连接持有该文件）。
pub async fn fetch_seed(seed_url: &str, token: &str, data_dir: &Path) -> Result<bool> {
    let mut request = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .context("构建 HTTP 客户端失败")?
        .get(seed_url);
    if !token.is_empty() {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.context("请求快照失败")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }
    if !response.status().is_success() {
        bail!("快照端点返回 HTTP {}", response.status());
    }
    let bytes = response.bytes().await.context("下载快照失败")?;
    if bytes.len() < 4096 {
        bail!("快照过小（{} 字节），疑似不完整", bytes.len());
    }
    if bytes.get(..16) != Some(&SQLITE_MAGIC[..]) {
        bail!("下载内容不是 SQLite 数据库（缺少文件头魔数）");
    }

    std::fs::create_dir_all(data_dir).context("创建数据目录失败")?;
    let tmp = data_dir.join("douban_export.db.seed-tmp");
    std::fs::write(&tmp, &bytes).context("写入种子临时文件失败")?;
    // 落盘前只读打开校验关键表存在（防截断/防拿到不含业务表的空库）
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&tmp)
        .read_only(true);
    let pool = sqlx::SqlitePool::connect_with(options)
        .await
        .context("打开种子快照失败")?;
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('users', 'records')",
    )
    .fetch_one(&pool)
    .await
    .context("校验种子快照结构失败")?;
    pool.close().await;
    if tables != 2 {
        let _ = std::fs::remove_file(&tmp);
        bail!("种子快照缺少业务表（users/records 命中 {tables}/2）");
    }

    // 原子换入
    let dest = data_dir.join("douban_export.db");
    std::fs::rename(&tmp, &dest).context("种子落盘失败")?;
    tracing::info!(
        "首启种子完成: {} 字节 -> {}",
        bytes.len(),
        dest.display()
    );
    Ok(true)
}
