//! 侧边栏统计（weimo-ui-stats StatGroup/Heatmap 数据源）——records 的
//! 轻量聚合：按类别计数 + 按标记日期计数。只读缓存不触发抓取；维度未
//! 同步的类别计 0，不走 marks/facets 的 DIMENSION_NOT_CACHED 404——
//! 侧边栏小部件允许部分数据缺失。

use serde_json::Value;
use sqlx::Row;

use crate::crawler::{Category, Status};

use super::error::AppResult;
use super::state::AppState;

/// 类别计数行 → 固定顺序（movie/book/music）的 (key, count) 列表，
/// 缺失类别补 0；未知类别忽略（防御脏数据行）
fn category_totals(rows: &[(String, i64)]) -> Vec<(&'static str, i64)> {
    Category::ALL
        .iter()
        .map(|category| {
            let count = rows
                .iter()
                .find(|(key, _)| key == category.key())
                .map(|(_, n)| *n)
                .unwrap_or(0);
            (category.key(), count)
        })
        .collect()
}

/// GET /users/{uid}/stats：totals 按类别计数（三类恒出现，未同步为 0），
/// daily_counts 按标记日期（YYYY-MM-DD）计数升序，空日期行不入聚合
pub async fn user_stats(state: &AppState, uid: &str, status: Status) -> AppResult<Value> {
    let total_rows = sqlx::query(
        "SELECT category, COUNT(*) AS n FROM records
         WHERE uid = ? AND dim_status = ?
         GROUP BY category",
    )
    .bind(uid)
    .bind(status.key())
    .fetch_all(&state.pool)
    .await?;
    let totals_raw: Vec<(String, i64)> = total_rows
        .iter()
        .map(|r| (r.get::<String, _>("category"), r.get::<i64, _>("n")))
        .collect();

    let daily_rows = sqlx::query(
        "SELECT date, COUNT(*) AS n FROM records
         WHERE uid = ? AND dim_status = ? AND date != ''
         GROUP BY date ORDER BY date",
    )
    .bind(uid)
    .bind(status.key())
    .fetch_all(&state.pool)
    .await?;
    let daily_counts: Vec<Value> = daily_rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "date": r.get::<String, _>("date"),
                "count": r.get::<i64, _>("n"),
            })
        })
        .collect();

    let totals: serde_json::Map<String, Value> = category_totals(&totals_raw)
        .into_iter()
        .map(|(key, count)| (key.to_string(), Value::from(count)))
        .collect();

    Ok(serde_json::json!({
        "user": uid,
        "status": status.key(),
        "totals": Value::Object(totals),
        "daily_counts": Value::Array(daily_counts),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_totals_fills_zeros_in_fixed_order() {
        let rows = vec![
            ("music".to_string(), 7),
            ("movie".to_string(), 120),
            ("dirty".to_string(), 9),
        ];
        // 固定 movie/book/music 顺序，book 缺失补 0，未知类别忽略
        assert_eq!(
            category_totals(&rows),
            vec![("movie", 120), ("book", 0), ("music", 7)]
        );
        // 全空 → 三类全 0
        assert_eq!(
            category_totals(&[]),
            vec![("movie", 0), ("book", 0), ("music", 0)]
        );
    }
}
