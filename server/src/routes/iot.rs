//! IoT API 端点

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use prometheus_core::security::CommandSanitizer;
use prometheus_core::security::{SecurityEvent, SecurityEventType, Severity};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct DeviceResponse {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub ip: String,
    pub port: u16,
    pub protocol: String,
}

#[derive(Serialize, ToSchema)]
pub struct ScanResponse {
    pub devices_found: usize,
    pub devices: Vec<DeviceResponse>,
}

#[derive(Deserialize, ToSchema)]
pub struct PairRequest {
    pub device_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct PairInitResponse {
    pub pin: String,
    pub device_id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct PairConfirmRequest {
    pub pin: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ControlRequest {
    pub device_id: String,
    pub command: String,
    pub params: serde_json::Value,
}

/// POST /api/v1/iot/scan
#[utoipa::path(post, path = "/api/v1/iot/scan", responses((status = 200, body = ScanResponse)))]
pub async fn scan_devices(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let mut discovery = state.iot_discovery.write().await;
    match discovery.scan(5).await {
        Ok(devices) => {
            let resp = ScanResponse {
                devices_found: devices.len(),
                devices: devices
                    .iter()
                    .map(|d| DeviceResponse {
                        id: d.id.clone(),
                        name: d.name.clone(),
                        device_type: d.device_type.clone(),
                        ip: d.ip.clone(),
                        port: d.port,
                        protocol: d.protocol.clone(),
                    })
                    .collect(),
            };
            Ok((StatusCode::OK, Json(serde_json::json!(resp))).into_response())
        }
        Err(_e) => Err(ApiError::Internal("设备扫描失败".into())),
    }
}

/// GET /api/v1/iot/devices
#[utoipa::path(get, path = "/api/v1/iot/devices", responses((status = 200, body = Vec<DeviceResponse>)))]
pub async fn list_devices(State(state): State<AppState>) -> impl IntoResponse {
    let discovery = state.iot_discovery.read().await;
    let devices: Vec<DeviceResponse> = discovery
        .devices()
        .iter()
        .map(|d| DeviceResponse {
            id: d.id.clone(),
            name: d.name.clone(),
            device_type: d.device_type.clone(),
            ip: d.ip.clone(),
            port: d.port,
            protocol: d.protocol.clone(),
        })
        .collect();
    Json(devices)
}

/// POST /api/v1/iot/pair
#[utoipa::path(post, path = "/api/v1/iot/pair", request_body = PairRequest, responses((status = 200, body = PairInitResponse)))]
pub async fn pair_device(
    State(state): State<AppState>,
    Json(req): Json<PairRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut pairing = state.iot_pairing.write().await;
    match pairing.initiate(&req.device_id).await {
        Ok(pin) => Ok((
            StatusCode::OK,
            Json(serde_json::json!(PairInitResponse {
                pin,
                device_id: req.device_id,
            })),
        )
            .into_response()),
        Err(_e) => Err(ApiError::BadRequest("配对初始化失败".into())),
    }
}

/// POST /api/v1/iot/pair/confirm
#[utoipa::path(post, path = "/api/v1/iot/pair/confirm", request_body = PairConfirmRequest, responses((status = 200)))]
pub async fn confirm_pair(
    State(state): State<AppState>,
    Json(req): Json<PairConfirmRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut pairing = state.iot_pairing.write().await;
    match pairing.confirm(&req.pin).await {
        Ok(()) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({"status": "paired"})),
        )
            .into_response()),
        Err(_e) => Err(ApiError::BadRequest("配对确认失败".into())),
    }
}

/// POST /api/v1/iot/control
#[utoipa::path(post, path = "/api/v1/iot/control", request_body = ControlRequest, responses((status = 200)))]
pub async fn control_device(
    State(state): State<AppState>,
    Json(req): Json<ControlRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // 安全检查: 命令注入防护
    CommandSanitizer::sanitize_device_id(&req.device_id)
        .map_err(|e| {
            state.audit_writer.send(SecurityEvent {
                timestamp: chrono::Utc::now(),
                event_type: SecurityEventType::CommandInjectionAttempt,
                severity: Severity::High,
                client_id: None,
                details: serde_json::json!({"device_id": &req.device_id, "error": e.to_string()}),
                action_taken: "blocked".into(),
            });
            ApiError::BadRequest("设备标识无效".into())
        })?;
    CommandSanitizer::sanitize_command(&req.command)
        .map_err(|e| {
            state.audit_writer.send(SecurityEvent {
                timestamp: chrono::Utc::now(),
                event_type: SecurityEventType::InvalidInput,
                severity: Severity::Medium,
                client_id: None,
                details: serde_json::json!({"command": &req.command, "error": e.to_string()}),
                action_taken: "blocked".into(),
            });
            ApiError::BadRequest("控制命令无效".into())
        })?;
    CommandSanitizer::sanitize_params(&req.params)
        .map_err(|e| {
            state.audit_writer.send(SecurityEvent {
                timestamp: chrono::Utc::now(),
                event_type: SecurityEventType::InvalidInput,
                severity: Severity::Medium,
                client_id: None,
                details: serde_json::json!({"error": e.to_string()}),
                action_taken: "blocked".into(),
            });
            ApiError::BadRequest("控制参数无效".into())
        })?;

    let controller = state.iot_controller.read().await;
    match controller
        .execute(&req.device_id, &req.command, req.params)
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(_e) => Err(ApiError::BadRequest("设备控制操作失败".into())),
    }
}

// ── Scene API ──────────────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub struct ExecuteSceneRequest {
    pub scene_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct SceneResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub action_count: usize,
}

/// GET /api/v1/iot/scenes
#[utoipa::path(get, path = "/api/v1/iot/scenes", responses((status = 200, body = Vec<SceneResponse>)))]
pub async fn list_scenes() -> impl IntoResponse {
    let scenes = vec![
        SceneResponse {
            id: "come-home".into(),
            name: "回家模式".into(),
            description: "开灯+空调+音乐".into(),
            action_count: 3,
        },
        SceneResponse {
            id: "leave-home".into(),
            name: "离家模式".into(),
            description: "关灯+锁门+布防".into(),
            action_count: 3,
        },
        SceneResponse {
            id: "good-night".into(),
            name: "晚安模式".into(),
            description: "调暗灯光+锁门+降温".into(),
            action_count: 3,
        },
        SceneResponse {
            id: "movie-time".into(),
            name: "观影模式".into(),
            description: "调暗灯光+开电视+调音量".into(),
            action_count: 3,
        },
    ];
    Json(serde_json::json!({"scenes": scenes}))
}

/// POST /api/v1/iot/scenes/execute
#[utoipa::path(post, path = "/api/v1/iot/scenes/execute", request_body = ExecuteSceneRequest, responses((status = 200)))]
pub async fn execute_scene(Json(req): Json<ExecuteSceneRequest>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "scene_id": req.scene_id,
        "message": "scene execution started"
    }))
}
