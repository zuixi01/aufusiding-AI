//! HTTP 中间件层

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::auth::Claims;
use crate::error::ApiError;
use crate::metrics::Metrics;
use crate::state::AppState;

/// CORS 中间件 — 支持 PROMETHEUS_CORS_ORIGINS 环境变量白名单
pub fn cors_layer() -> CorsLayer {
    let origins = std::env::var("PROMETHEUS_CORS_ORIGINS").unwrap_or_default();
    let layer = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any);

    if origins.is_empty() {
        layer.allow_origin(Any)
    } else {
        let parsed: Vec<HeaderValue> = origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        layer.allow_origin(parsed)
    }
}

/// 请求追踪中间件
pub fn trace_layer(
) -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
}

/// gzip 响应压缩中间件
pub fn compression_layer() -> CompressionLayer {
    CompressionLayer::new().gzip(true)
}

/// 请求超时中间件 — 通过 PROMETHEUS_TIMEOUT_SECS 配置（默认 30s）
pub fn timeout_layer() -> TimeoutLayer {
    let secs = std::env::var("PROMETHEUS_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);
    TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        Duration::from_secs(secs),
    )
}

/// 并发请求限制 — 通过 PROMETHEUS_CONCURRENCY_LIMIT 配置（默认 64）
pub fn concurrency_limit_layer() -> ConcurrencyLimitLayer {
    let limit = std::env::var("PROMETHEUS_CONCURRENCY_LIMIT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64);
    ConcurrencyLimitLayer::new(limit)
}

/// 请求体大小限制 — 通过 PROMETHEUS_BODY_LIMIT_MB 配置（默认 2MB）
pub fn body_limit_layer() -> RequestBodyLimitLayer {
    let mb = std::env::var("PROMETHEUS_BODY_LIMIT_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2);
    RequestBodyLimitLayer::new(mb * 1024 * 1024)
}

/// 指标收集中间件 — 自动记录请求计数、延迟、状态码分类，并输出结构化日志
pub async fn metrics_middleware(
    State(metrics): State<Metrics>,
    request: Request<axum::body::Body>,
    next: Next,
) -> impl IntoResponse {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let is_chat = path.ends_with("/chat");
    let is_memory_search = path.contains("/memory/search");
    let is_knowledge_write = method == axum::http::Method::POST && path.contains("/knowledge");

    metrics.inc_requests_total();
    metrics.inc_active();

    let start = Instant::now();
    let response = next.run(request).await;
    let elapsed = start.elapsed();
    let elapsed_us = elapsed.as_micros() as u64;
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

    metrics.record_response_time_us(elapsed_us);
    metrics.dec_active();

    let status = response.status().as_u16();
    match status {
        200..=299 => metrics.inc_success(),
        400..=499 => metrics.inc_client_error(),
        500..=599 => metrics.inc_server_error(),
        _ => {}
    }

    if is_chat {
        metrics.inc_chat();
    }
    if is_memory_search {
        metrics.inc_memory_search();
    }
    if is_knowledge_write {
        metrics.inc_knowledge_write();
    }

    // 结构化请求日志
    tracing::info!(
        http.method = %method,
        http.path = %path,
        http.status = status,
        http.latency_ms = format!("{elapsed_ms:.2}"),
        "request completed"
    );

    response
}

/// Request-ID 追踪中间件 — 传播或生成 X-Request-Id
pub async fn request_id_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> impl IntoResponse {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut response = next.run(request).await;
    if let Ok(header_value) = request_id.parse() {
        response.headers_mut().insert("x-request-id", header_value);
    }
    response
}

/// Per-client 速率限制中间件
///
/// 从 Claims (已认证) 或 ConnectInfo (未认证) 获取 client_id 进行限流。
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<impl IntoResponse, ApiError> {
    use prometheus_core::security::{SecurityEvent, SecurityEventType, Severity};

    // 优先使用认证身份，否则使用客户端 IP
    let client_id = request
        .extensions()
        .get::<Claims>()
        .map(|c| c.sub.clone())
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ci| ci.0.ip().to_string())
        })
        .unwrap_or_else(|| "anonymous".into());

    if let Err(e) = state.rate_limiter.check(&client_id).await {
        state.audit_writer.send(SecurityEvent {
            timestamp: chrono::Utc::now(),
            event_type: SecurityEventType::RateLimitExceeded,
            severity: Severity::Medium,
            client_id: Some(client_id),
            details: serde_json::json!({"error": e.to_string()}),
            action_taken: "blocked".into(),
        });
        return Err(ApiError::TooManyRequests("请求过于频繁，请稍后重试".into()));
    }

    Ok(next.run(request).await)
}
