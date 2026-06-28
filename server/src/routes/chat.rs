//! Chat API 端点
//!
//! - POST /api/v1/sessions           创建会话
//! - GET  /api/v1/sessions           列出会话
//! - POST /api/v1/sessions/:id/chat  发送消息
//! - GET  /api/v1/sessions/:id/messages 获取会话消息

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use prometheus_core::security::{SecurityEvent, SecurityEventType, Severity};

// ── 请求 / 响应类型 ──

#[derive(Deserialize, ToSchema)]
pub struct CreateSessionRequest {
    pub title: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SessionResponse {
    pub id: Uuid,
    pub title: String,
    pub state: String,
    pub created_at: String,
    pub last_active: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ChatRequest {
    pub message: String,
}

#[derive(Serialize, ToSchema)]
pub struct ChatResponse {
    pub reply: String,
    pub route_reason: String,
}

#[derive(Serialize, ToSchema)]
pub struct MessageResponse {
    pub id: Uuid,
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

// ── 端点处理函数 ──

/// POST /api/v1/sessions - 创建新会话
#[utoipa::path(post, path = "/api/v1/sessions",
    request_body = CreateSessionRequest,
    responses((status = 201, body = SessionResponse))
)]
pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut orch = state.orchestrator.write().await;
    let session = orch
        .create_session(req.title.as_deref())
        .await
        .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(SessionResponse {
            id: session.id,
            title: session.title.clone(),
            state: format!("{:?}", session.state),
            created_at: session.created_at.to_rfc3339(),
            last_active: session.last_active.to_rfc3339(),
        }),
    ))
}

/// GET /api/v1/sessions - 列出所有会话
#[utoipa::path(get, path = "/api/v1/sessions",
    responses((status = 200, body = Vec<SessionResponse>))
)]
pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<SessionResponse>>, ApiError> {
    let orch = state.orchestrator.read().await;
    let sessions: Vec<SessionResponse> = orch
        .sessions()
        .iter()
        .map(|s| SessionResponse {
            id: s.id,
            title: s.title.clone(),
            state: format!("{:?}", s.state),
            created_at: s.created_at.to_rfc3339(),
            last_active: s.last_active.to_rfc3339(),
        })
        .collect();
    Ok(Json(sessions))
}

/// POST /api/v1/sessions/:id/chat - 发送消息
#[utoipa::path(post, path = "/api/v1/sessions/{id}/chat",
    request_body = ChatRequest,
    params(("id" = Uuid, Path, description = "会话 ID")),
    responses((status = 200, body = ChatResponse), (status = 400, description = "消息为空"))
)]
pub async fn send_message(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, ApiError> {
    if req.message.trim().is_empty() {
        return Err(ApiError::BadRequest("消息不能为空".into()));
    }

    // 安全检查: 输入验证
    if let Err(e) = state.input_validator.validate_text(&req.message) {
        state.audit_writer.send(SecurityEvent {
            timestamp: chrono::Utc::now(),
            event_type: SecurityEventType::InvalidInput,
            severity: Severity::Medium,
            client_id: None,
            details: serde_json::json!({"session_id": session_id.to_string(), "error": e.to_string()}),
            action_taken: "blocked".into(),
        });
        return Err(ApiError::BadRequest("请求内容不合法".into()));
    }
    // 安全检查: Prompt 注入检测
    if let Err(_e) = state.prompt_guard.sanitize(&req.message) {
        state.audit_writer.send(SecurityEvent {
            timestamp: chrono::Utc::now(),
            event_type: SecurityEventType::PromptInjectionAttempt,
            severity: Severity::High,
            client_id: None,
            details: serde_json::json!({"session_id": session_id.to_string(), "message": &req.message}),
            action_taken: "blocked".into(),
        });
        return Err(ApiError::BadRequest("请求被安全策略拒绝".into()));
    }

    let mut orch = state.orchestrator.write().await;
    let turn = orch
        .send_message(session_id, &req.message)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(ChatResponse {
        reply: turn.reply,
        route_reason: turn.route_reason,
    }))
}

/// GET /api/v1/sessions/:id/messages - 获取会话消息
#[utoipa::path(get, path = "/api/v1/sessions/{id}/messages",
    params(("id" = Uuid, Path, description = "会话 ID")),
    responses((status = 200, body = Vec<MessageResponse>))
)]
pub async fn get_messages(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Vec<MessageResponse>>, ApiError> {
    let orch = state.orchestrator.read().await;
    let messages = orch
        .session_messages(session_id)
        .await
        .map_err(|_e| ApiError::NotFound(format!("会话 {} 未找到", session_id)))?;

    let resp: Vec<MessageResponse> = messages
        .iter()
        .map(|m| MessageResponse {
            id: m.id,
            role: format!("{:?}", m.role),
            content: m.content.clone(),
            timestamp: m.timestamp.to_rfc3339(),
        })
        .collect();
    Ok(Json(resp))
}
