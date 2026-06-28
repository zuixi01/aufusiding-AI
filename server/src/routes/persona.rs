//! Persona API 端点

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct PersonaResponse {
    pub id: String,
    pub name: String,
    pub tone: String,
    pub expertise: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct SwitchPersonaRequest {
    pub persona_id: String,
}

/// GET /api/v1/persona/list
#[utoipa::path(get, path = "/api/v1/persona/list", responses((status = 200, body = Vec<PersonaResponse>)))]
pub async fn list_personas(State(state): State<AppState>) -> impl IntoResponse {
    let engine = state.persona_engine.read().await;
    let personas: Vec<PersonaResponse> = engine
        .list_personas()
        .iter()
        .map(|p| PersonaResponse {
            id: p.id.to_string(),
            name: p.name.clone(),
            tone: p.tone.clone(),
            expertise: p.expertise.clone(),
        })
        .collect();
    Json(personas)
}

/// POST /api/v1/persona/switch
#[utoipa::path(post, path = "/api/v1/persona/switch", request_body = SwitchPersonaRequest, responses((status = 200)))]
pub async fn switch_persona(
    State(state): State<AppState>,
    Json(req): Json<SwitchPersonaRequest>,
) -> impl IntoResponse {
    let id = match uuid::Uuid::parse_str(&req.persona_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid persona_id"})),
            )
                .into_response()
        }
    };
    let mut engine = state.persona_engine.write().await;
    match engine.switch_persona(id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "switched", "persona_id": req.persona_id})),
        )
            .into_response(),
        Err(_e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "人格未找到"})),
        )
            .into_response(),
    }
}

/// GET /api/v1/persona/current
#[utoipa::path(get, path = "/api/v1/persona/current", responses((status = 200)))]
pub async fn current_persona(State(state): State<AppState>) -> impl IntoResponse {
    let engine = state.persona_engine.read().await;
    match engine.current_persona() {
        Some(p) => (
            StatusCode::OK,
            Json(serde_json::json!(PersonaResponse {
                id: p.id.to_string(),
                name: p.name.clone(),
                tone: p.tone.clone(),
                expertise: p.expertise.clone(),
            })),
        )
            .into_response(),
        None => (StatusCode::OK, Json(serde_json::json!({"persona": null}))).into_response(),
    }
}
