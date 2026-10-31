//! 统一错误——信封 `{"error":{"code","message"}}`，映射设计文档第 9 节的错误码表。

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("{0}")]
    InvalidArgument(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("任务不存在或已过期: {0}")]
    TaskNotFound(String),
    #[error("订阅不存在: {0}")]
    SubscriptionNotFound(String),
    #[error("该用户资料尚未同步（需成功抓取其收藏页）: {0}")]
    UserNotFound(String),
    /// verify 端点专用：豆瓣收藏页 404，ID 大概率有误。与 UserNotFound 的
    /// 「本地未同步」语义区分，前端据此提示检查 ID
    #[error("未找到该豆瓣用户，请检查 ID 是否正确: {0}")]
    UserVerifyNotFound(String),
    /// verify 端点专用：豆瓣侧故障（限流/超时/熔断中/页面缺资料块）
    #[error("豆瓣侧暂时无法完成验证: {0}")]
    UserVerifyUpstream(String),
    #[error("该维度无缓存，请先创建同步任务")]
    DimensionNotCached,
    #[error("条目元信息不存在或尚未富集: {0}")]
    SubjectNotFound(String),
    #[error("富集任务不存在: {0}")]
    JobNotFound(String),
    #[error("任务未完成（当前状态: {0}）")]
    TaskNotFinished(String),
    #[error("请求过于频繁，请稍后再试")]
    TooManyRequests,
    #[error("队列已满，请稍后再试")]
    TooManyTasks,
    #[error("封面回源失败: {0}")]
    CoverUpstream(String),
    #[error("{0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::InvalidArgument(_) => "INVALID_ARGUMENT",
            AppError::Unauthorized(_) => "UNAUTHORIZED",
            AppError::TaskNotFound(_) => "TASK_NOT_FOUND",
            AppError::SubscriptionNotFound(_) => "SUBSCRIPTION_NOT_FOUND",
            AppError::UserNotFound(_) => "USER_NOT_FOUND",
            AppError::UserVerifyNotFound(_) => "PAGE_NOT_FOUND",
            AppError::UserVerifyUpstream(_) => "DOUBAN_UNAVAILABLE",
            AppError::DimensionNotCached => "DIMENSION_NOT_CACHED",
            AppError::SubjectNotFound(_) => "SUBJECT_NOT_FOUND",
            AppError::JobNotFound(_) => "JOB_NOT_FOUND",
            AppError::TaskNotFinished(_) => "TASK_NOT_FINISHED",
            AppError::TooManyRequests => "TOO_MANY_REQUESTS",
            AppError::TooManyTasks => "TOO_MANY_TASKS",
            AppError::CoverUpstream(_) => "COVER_UPSTREAM_FAILED",
            AppError::Internal(_) => "INTERNAL",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            AppError::InvalidArgument(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::TaskNotFound(_)
            | AppError::SubscriptionNotFound(_)
            | AppError::UserNotFound(_)
            | AppError::UserVerifyNotFound(_)
            | AppError::DimensionNotCached
            | AppError::SubjectNotFound(_)
            | AppError::JobNotFound(_) => StatusCode::NOT_FOUND,
            AppError::TaskNotFinished(_) => StatusCode::CONFLICT,
            AppError::TooManyRequests | AppError::TooManyTasks => StatusCode::TOO_MANY_REQUESTS,
            AppError::CoverUpstream(_) | AppError::UserVerifyUpstream(_) => StatusCode::BAD_GATEWAY,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = Json(ErrorEnvelope {
            error: ErrorBody {
                code: self.code(),
                message: self.to_string(),
            },
        });
        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = %e, "database error");
        AppError::Internal("数据库错误".to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        tracing::error!(error = %e, "io error");
        AppError::Internal("文件系统错误".to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!(error = %e, "internal error");
        AppError::Internal(e.to_string())
    }
}
