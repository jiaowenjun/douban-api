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
  records       TEXT NOT NULL DEFAULT '[]',
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

CREATE TABLE IF NOT EXISTS task_files (
  task_id    TEXT NOT NULL,
  name       TEXT NOT NULL,
  category   TEXT NOT NULL,
  dim_status TEXT NOT NULL,
  format     TEXT NOT NULL,
  size       INTEGER NOT NULL,
  sha256     TEXT NOT NULL,
  items      INTEGER NOT NULL,
  PRIMARY KEY (task_id, name),
  FOREIGN KEY (task_id) REFERENCES tasks(task_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_dims_dim ON task_dims(uid, category, dim_status);
CREATE INDEX IF NOT EXISTS idx_dimensions_due ON dimensions(state, next_sync_ms);
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
    Ok(pool)
}
