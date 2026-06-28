//! API 统一错误处理

#![allow(dead_code)]

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// API 错误类型
#[derive(Debug)]
pub enum ApiError {
    /// 请求参数错误
    BadRequest(String),
    /// 未认证
    Unauthorized(String),
    /// 权限不足
    Forbidden(String),
    /// 资源未找到
    NotFound(String),
    /// 内部错误
    Internal(String),
    /// 速率限制
    TooManyRequests(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: u16,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "内部服务器错误".into())
            }
            ApiError::TooManyRequests(msg) => (StatusCode::TOO_MANY_REQUESTS, msg),
        };

        let body = ErrorBody {
            error: ErrorDetail {
                code: status.as_u16(),
                message,
            },
        };

        (status, axum::Json(body)).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::Internal(err.to_string())
    }
}
