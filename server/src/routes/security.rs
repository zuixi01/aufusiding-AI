//! Security API 端点
//!
//! - GET /api/v1/security/audit  查询安全审计日志

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct AuditQuery {
    pub limit: Option<usize>,
}

#[derive(Serialize)]
struct AuditEventResponse {
    timestamp: String,
    event_type: String,
    severity: String,
    client_id: Option<String>,
    details: serde_json::Value,
    action_taken: String,
}

#[derive(Serialize)]
pub struct AuditResponse {
    events: Vec<AuditEventResponse>,
    total: u64,
}

/// GET /api/v1/security/audit?limit=N
pub async fn query_audit_log(
    State(state): State<AppState>,
    Query(params): Query<AuditQuery>,
) -> Result<Json<AuditResponse>, ApiError> {
    let limit = params.limit.unwrap_or(50).min(1000);

    let events = state
        .audit_log
        .query_recent(limit)
        .map_err(|e| ApiError::Internal(format!("审计日志查询失败: {}", e)))?;

    let total = state
        .audit_log
        .count_events()
        .map_err(|e| ApiError::Internal(format!("审计日志统计失败: {}", e)))?;

    let resp_events: Vec<AuditEventResponse> = events
        .into_iter()
        .map(|e| AuditEventResponse {
            timestamp: e.timestamp.to_rfc3339(),
            event_type: format!("{:?}", e.event_type),
            severity: format!("{:?}", e.severity),
            client_id: e.client_id,
            details: e.details,
            action_taken: e.action_taken,
        })
        .collect();

    Ok(Json(AuditResponse {
        events: resp_events,
        total,
    }))
}
