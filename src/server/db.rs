//! SQLite 连接与建表（weimo-ocr 风格：内嵌 bootstrap SQL，不用 sqlx migrate）。

use anyhow::{Context, Result};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

const BOOTSTRAP_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
  task_id     TEXT PRIMARY KEY,
  user        TEXT NOT NULL,
  request_json TEXT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'queued',
  items_count INTEGER NOT NULL DEFAULT 0,
  error_json  TEXT,
  created_ms  INTEGER NOT NULL,
  started_ms  INTEGER,
  finished_ms INTEGER,
  expires_ms  INTEGER
);

CREATE TABLE IF NOT EXISTS dimensions (
  uid           TEXT NOT NULL,
  category      TEXT NOT NULL,
  dim_status    TEXT NOT NULL,
  state         TEXT NOT NULL DEFAULT 'idle',
  fetched_count INTEGER NOT NULL DEFAULT 0,
  total         INTEGER,
  next_start    INTEGER NOT NULL DEFAULT 0,
  fetched_ms    INTEGER,
  attempts      INTEGER NOT NULL DEFAULT 0,
  next_sync_ms  INTEGER,
  last_error    TEXT,
  updated_ms    INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (uid, category, dim_status)
);

CREATE TABLE IF NOT EXISTS task_dims (
  task_id    TEXT NOT NULL,
  uid        TEXT NOT NULL,
  category   TEXT NOT NULL,
  dim_status TEXT NOT NULL,
  state      TEXT NOT NULL DEFAULT 'waiting',
  items      INTEGER NOT NULL DEFAULT 0,
  error      TEXT,
  PRIMARY KEY (task_id, category, dim_status),
  FOREIGN KEY (task_id) REFERENCES tasks(task_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS records (
  uid         TEXT NOT NULL,
  category    TEXT NOT NULL,
  dim_status  TEXT NOT NULL,
  subject_id  TEXT NOT NULL,
  title       TEXT NOT NULL DEFAULT '',
  alt_title   TEXT NOT NULL DEFAULT '',
  my_rating   INTEGER,
  date        TEXT NOT NULL DEFAULT '',
  comment     TEXT NOT NULL DEFAULT '',
  subject_url TEXT NOT NULL DEFAULT '',
  cover       TEXT NOT NULL DEFAULT '',
  comment_id  TEXT NOT NULL DEFAULT '',
  intro       TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (uid, category, dim_status, subject_id)
);

CREATE TABLE IF NOT EXISTS subscriptions (
  uid          TEXT PRIMARY KEY,
  enabled      INTEGER NOT NULL DEFAULT 1,
  rss_interval_secs  INTEGER NOT NULL,
  full_interval_secs INTEGER NOT NULL,
  next_rss_ms  INTEGER NOT NULL,
  next_full_ms INTEGER NOT NULL,
  last_rss_ms  INTEGER,
  last_full_ms INTEGER,
  last_guid    TEXT NOT NULL DEFAULT '',
  rss_attempts INTEGER NOT NULL DEFAULT 0,
  last_error   TEXT,
  created_ms   INTEGER NOT NULL,
  updated_ms   INTEGER NOT NULL DEFAULT 0
);

-- 旧库的文件导出元数据表（数据现已直接入库，无迁移、直接丢弃）
DROP TABLE IF EXISTS task_files;

CREATE INDEX IF NOT EXISTS idx_task_dims_dim ON task_dims(uid, category, dim_status);
CREATE INDEX IF NOT EXISTS idx_dimensions_due ON dimensions(state, next_sync_ms);
CREATE INDEX IF NOT EXISTS idx_records_dim_date ON records(uid, category, dim_status, date DESC);
"#;

pub async fn connect(db_path: &std::path::Path) -> Result<SqlitePool> {
    let url = format!("sqlite:{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .with_context(|| format!("连接数据库失败: {}", db_path.display()))?;

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .context("启用外键失败")?;

    for statement in BOOTSTRAP_SCHEMA
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .with_context(|| format!("建表失败: {statement}"))?;
    }
    ensure_column(&pool, "tasks", "source", "TEXT NOT NULL DEFAULT 'manual'").await?;
    Ok(pool)
}

/// 幂等加列（CREATE TABLE IF NOT EXISTS 不会更新已有表结构）
async fn ensure_column(pool: &SqlitePool, table: &str, column: &str, ddl: &str) -> Result<()> {
    let sql = format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'");
    let has: i64 = sqlx::query_scalar(&sql)
        .fetch_one(pool)
        .await
        .with_context(|| format!("检查 {table}.{column} 列失败"))?;
    if has == 0 {
        let alter = format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}");
        sqlx::query(&alter)
            .execute(pool)
            .await
            .with_context(|| format!("加列失败: {alter}"))?;
    }
    Ok(())
}
