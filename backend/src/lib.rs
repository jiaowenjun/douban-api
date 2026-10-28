//! weimo-douban 核心库
//!
//! - [`crawler`]：抓取/解析/导出的共享逻辑（CLI 与 server 共用）
//! - [`server`]：axum 服务器（任务队列、缓存、API）

pub mod crawler;
pub mod intro;
pub mod server;
