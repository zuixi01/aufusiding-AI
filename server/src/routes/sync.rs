//! Sync API 端点

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct SyncStatusResponse {
    pub last_sync: Option<String>,
    pub pending_ops: usize,
    pub connected: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct PushRequest {
    pub operation_type: String,
    pub data: String,
}

/// GET /api/v1/sync/status
#[utoipa::path(get, path = "/api/v1/sync/status", responses((status = 200, body = SyncStatusResponse)))]
pub async fn sync_status(State(state): State<AppState>) -> impl IntoResponse {
    let engine = state.sync_engine.read().await;
    let status = engine.get_status();
    Json(SyncStatusResponse {
        last_sync: status.last_sync.map(|t| t.to_rfc3339()),
        pending_ops: status.pending_ops,
        connected: status.connected,
    })
}

/// POST /api/v1/sync/push
#[utoipa::path(post, path = "/api/v1/sync/push", request_body = PushRequest, responses((status = 200)))]
pub async fn sync_push(
    State(state): State<AppState>,
    Json(req): Json<PushRequest>,
) -> impl IntoResponse {
    use prometheus_core::sync::protocol::Operation;
    let mut engine = state.sync_engine.write().await;
    let id = uuid::Uuid::new_v4();
    let op = match req.operation_type.as_str() {
        "insert" => Operation::Insert { id, data: req.data },
        "update" => Operation::Update { id, data: req.data },
        "delete" => Operation::Delete { id },
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid operation type"})),
            )
                .into_response()
        }
    };
    engine.push_change(op);
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "queued", "id": id.to_string()})),
    )
        .into_response()
}

/// POST /api/v1/sync/pull
#[utoipa::path(post, path = "/api/v1/sync/pull", responses((status = 200)))]
pub async fn sync_pull(State(state): State<AppState>) -> impl IntoResponse {
    let engine = state.sync_engine.read().await;
    let status = engine.get_status();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "pending_ops": status.pending_ops,
            "connected": status.connected,
        })),
    )
}
