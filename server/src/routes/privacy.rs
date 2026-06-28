//! Privacy API 路由
//!
//! GDPR 合规端点：同意管理、数据导出、数据删除、匿名化。

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::state::AppState;
use prometheus_core::privacy::{Anonymizer, ConsentPurpose};

// ── 请求/响应类型 ──

/// 同意记录请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct ConsentRequest {
    pub user_id: String,
    pub purpose: String,
    pub granted: bool,
}

/// 同意记录响应
#[derive(Debug, Serialize, ToSchema)]
pub struct ConsentResponse {
    pub success: bool,
    pub message: String,
}

/// 数据导出请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct ExportRequest {
    pub session_id: String,
}

/// 数据删除请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteDataRequest {
    pub session_id: String,
}

/// 删除响应
#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteDataResponse {
    pub messages_deleted: usize,
    pub success: bool,
}

/// 匿名化请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct AnonymizeRequest {
    pub text: String,
}

/// 匿名化响应
#[derive(Debug, Serialize, ToSchema)]
pub struct AnonymizeResponse {
    pub anonymized: String,
}

// ── Handlers ──

fn parse_purpose(s: &str) -> Result<ConsentPurpose, ApiError> {
    match s {
        "data_collection" => Ok(ConsentPurpose::DataCollection),
        "analytics" => Ok(ConsentPurpose::Analytics),
        "personalization" => Ok(ConsentPurpose::Personalization),
        "third_party_sharing" => Ok(ConsentPurpose::ThirdPartySharing),
        "voice_processing" => Ok(ConsentPurpose::VoiceProcessing),
        _ => Err(ApiError::BadRequest(format!("未知的同意目的: {s}"))),
    }
}

/// POST /api/v1/privacy/consent - 记录同意
#[utoipa::path(post, path = "/api/v1/privacy/consent",
    request_body = ConsentRequest,
    responses(
        (status = 200, body = ConsentResponse),
        (status = 400, description = "参数错误")
    )
)]
pub async fn record_consent(
    State(state): State<AppState>,
    Json(req): Json<ConsentRequest>,
) -> Result<Json<ConsentResponse>, ApiError> {
    if req.user_id.trim().is_empty() {
        return Err(ApiError::BadRequest("user_id 不能为空".into()));
    }
    let purpose = parse_purpose(&req.purpose)?;
    let mgr = state.consent_manager.read().await;
    mgr.record_consent(&req.user_id, purpose, req.granted)
        .map_err(|e| ApiError::Internal(format!("记录同意失败: {e}")))?;

    Ok(Json(ConsentResponse {
        success: true,
        message: if req.granted {
            "同意已记录".into()
        } else {
            "同意已撤销".into()
        },
    }))
}

/// GET /api/v1/privacy/export?session_id=... - 导出数据
#[utoipa::path(get, path = "/api/v1/privacy/export",
    params(("session_id" = String, Query, description = "会话 ID")),
    responses(
        (status = 200, description = "导出的 JSON 数据"),
        (status = 400, description = "参数错误")
    )
)]
pub async fn export_data(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<ExportRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session_id = uuid::Uuid::parse_str(&params.session_id)
        .map_err(|_| ApiError::BadRequest("无效的 session_id".into()))?;

    let episodic = state.episodic_memory.read().await;
    let knowledge = state.knowledge_base.read().await;
    let working = state.working_memory.read().await;

    let exporter = prometheus_core::privacy::DataExporter::new(
        &state.history_store,
        &episodic,
        &knowledge,
        &working,
    );

    let data = exporter
        .export_all(session_id)
        .await
        .map_err(|e| ApiError::Internal(format!("导出失败: {e}")))?;

    Ok(Json(data))
}

/// DELETE /api/v1/privacy/data - 删除数据
#[utoipa::path(delete, path = "/api/v1/privacy/data",
    request_body = DeleteDataRequest,
    responses(
        (status = 200, body = DeleteDataResponse),
        (status = 400, description = "参数错误")
    )
)]
pub async fn delete_data(
    State(state): State<AppState>,
    Json(req): Json<DeleteDataRequest>,
) -> Result<Json<DeleteDataResponse>, ApiError> {
    let session_id = uuid::Uuid::parse_str(&req.session_id)
        .map_err(|_| ApiError::BadRequest("无效的 session_id".into()))?;

    let mut episodic = state.episodic_memory.write().await;
    let mut working = state.working_memory.write().await;

    let mut eraser = prometheus_core::privacy::deletion::DataEraser::new(
        &state.history_store,
        &mut episodic,
        &mut working,
    );

    let receipt = eraser
        .delete_all(session_id)
        .await
        .map_err(|e| ApiError::Internal(format!("删除失败: {e}")))?;

    Ok(Json(DeleteDataResponse {
        messages_deleted: receipt.messages_deleted,
        success: true,
    }))
}

/// POST /api/v1/privacy/anonymize - 匿名化文本
#[utoipa::path(post, path = "/api/v1/privacy/anonymize",
    request_body = AnonymizeRequest,
    responses(
        (status = 200, body = AnonymizeResponse),
        (status = 400, description = "text 为空")
    )
)]
pub async fn anonymize_text(
    Json(req): Json<AnonymizeRequest>,
) -> Result<Json<AnonymizeResponse>, ApiError> {
    if req.text.is_empty() {
        return Err(ApiError::BadRequest("text 不能为空".into()));
    }
    let anonymized = Anonymizer::anonymize(&req.text);
    Ok(Json(AnonymizeResponse { anonymized }))
}
