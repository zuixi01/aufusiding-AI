//! Stats API 路由
//!
//! 提供系统统计信息：会话概览、模型列表、Token 用量。

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

// ── 响应类型 ──

/// 会话统计
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionStats {
    pub total_sessions: usize,
    pub sessions: Vec<SessionSummary>,
}

/// 单个会话摘要
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionSummary {
    pub id: Uuid,
    pub title: String,
    pub state: String,
    pub created_at: String,
    pub last_active: String,
    pub token_usage: Option<TokenUsageSummary>,
}

/// Token 用量摘要
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenUsageSummary {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// 系统概览
#[derive(Debug, Serialize, ToSchema)]
pub struct SystemOverview {
    pub sessions: usize,
    pub episodic_memories: usize,
    pub knowledge_chunks: usize,
    pub working_memory_items: usize,
}

// ── Handlers ──

/// GET /api/v1/stats/sessions - 会话统计
#[utoipa::path(get, path = "/api/v1/stats/sessions",
    responses((status = 200, body = SessionStats))
)]
pub async fn session_stats(State(state): State<AppState>) -> Result<Json<SessionStats>, ApiError> {
    let orch = state.orchestrator.read().await;
    let sessions = orch.sessions();

    let summaries: Vec<SessionSummary> = sessions
        .iter()
        .map(|s| {
            let usage = orch
                .session_token_usage(s.id)
                .ok()
                .flatten()
                .map(|u| TokenUsageSummary {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                });

            SessionSummary {
                id: s.id,
                title: s.title.clone(),
                state: format!("{:?}", s.state),
                created_at: s.created_at.to_rfc3339(),
                last_active: s.last_active.to_rfc3339(),
                token_usage: usage,
            }
        })
        .collect();

    Ok(Json(SessionStats {
        total_sessions: summaries.len(),
        sessions: summaries,
    }))
}

/// GET /api/v1/stats/overview - 系统概览
#[utoipa::path(get, path = "/api/v1/stats/overview",
    responses((status = 200, body = SystemOverview))
)]
pub async fn overview(State(state): State<AppState>) -> Result<Json<SystemOverview>, ApiError> {
    let orch = state.orchestrator.read().await;
    let session_count = orch.sessions().len();
    drop(orch);

    let episodic = state.episodic_memory.read().await;
    let episodic_count = episodic
        .count()
        .map_err(|e| ApiError::Internal(format!("episodic count: {e}")))?;
    drop(episodic);

    let kb = state.knowledge_base.read().await;
    let knowledge_count = kb
        .count()
        .map_err(|e| ApiError::Internal(format!("knowledge count: {e}")))?;
    drop(kb);

    let wm = state.working_memory.read().await;
    let wm_count = wm.len();
    drop(wm);

    Ok(Json(SystemOverview {
        sessions: session_count,
        episodic_memories: episodic_count,
        knowledge_chunks: knowledge_count,
        working_memory_items: wm_count,
    }))
}
