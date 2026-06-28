//! Memory API 路由
//!
//! 提供情景记忆搜索、知识库管理、工作记忆查询接口。

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

// ── 请求/响应类型 ──

/// 记忆搜索请求
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct MemorySearchRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_top_k() -> usize {
    5
}

/// 情景记忆搜索结果
#[derive(Debug, Serialize, ToSchema)]
pub struct EpisodeResult {
    pub id: Uuid,
    pub summary: String,
    pub entities: Vec<String>,
    pub timestamp: String,
}

/// 添加知识请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddKnowledgeRequest {
    pub source: String,
    pub content: String,
}

/// 添加知识响应
#[derive(Debug, Serialize, ToSchema)]
pub struct AddKnowledgeResponse {
    pub chunks_added: usize,
    pub source: String,
}

/// 知识搜索结果
#[derive(Debug, Serialize, ToSchema)]
pub struct KnowledgeResult {
    pub id: Uuid,
    pub source: String,
    pub content: String,
}

/// 工作记忆项
#[derive(Debug, Serialize, ToSchema)]
pub struct WorkingMemoryItem {
    pub content: String,
}

/// 添加工作记忆请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddWorkingMemoryRequest {
    pub content: String,
    #[serde(default = "default_importance")]
    pub importance: f64,
}

fn default_importance() -> f64 {
    0.5
}

// ── Handlers ──

/// POST /api/v1/memory/search - 搜索情景记忆
#[utoipa::path(post, path = "/api/v1/memory/search",
    request_body = MemorySearchRequest,
    responses((status = 200, body = Vec<EpisodeResult>), (status = 400, description = "query 为空"))
)]
pub async fn search_episodic(
    State(state): State<AppState>,
    Json(req): Json<MemorySearchRequest>,
) -> Result<Json<Vec<EpisodeResult>>, ApiError> {
    if req.query.trim().is_empty() {
        return Err(ApiError::BadRequest("query cannot be empty".into()));
    }

    let memory = state.episodic_memory.read().await;
    let episodes = memory
        .search(&req.query, req.top_k)
        .await
        .map_err(|e| ApiError::Internal(format!("episodic search failed: {e}")))?;

    let results = episodes
        .into_iter()
        .map(|ep| EpisodeResult {
            id: ep.id,
            summary: ep.summary,
            entities: ep.entities,
            timestamp: ep.timestamp.to_rfc3339(),
        })
        .collect();

    Ok(Json(results))
}

/// POST /api/v1/memory/knowledge - 添加知识
#[utoipa::path(post, path = "/api/v1/memory/knowledge",
    request_body = AddKnowledgeRequest,
    responses((status = 201, body = AddKnowledgeResponse), (status = 400, description = "content 为空"))
)]
pub async fn add_knowledge(
    State(state): State<AppState>,
    Json(req): Json<AddKnowledgeRequest>,
) -> Result<(StatusCode, Json<AddKnowledgeResponse>), ApiError> {
    if req.content.trim().is_empty() {
        return Err(ApiError::BadRequest("content cannot be empty".into()));
    }

    let mut kb = state.knowledge_base.write().await;
    let chunks = kb
        .add_document(&req.source, &req.content)
        .await
        .map_err(|e| ApiError::Internal(format!("add knowledge failed: {e}")))?;

    Ok((
        StatusCode::CREATED,
        Json(AddKnowledgeResponse {
            chunks_added: chunks,
            source: req.source,
        }),
    ))
}

/// GET /api/v1/memory/knowledge?query=...&top_k=5 - 搜索知识库
#[utoipa::path(get, path = "/api/v1/memory/knowledge",
    params(MemorySearchRequest),
    responses((status = 200, body = Vec<KnowledgeResult>), (status = 400, description = "query 为空"))
)]
pub async fn search_knowledge(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<MemorySearchRequest>,
) -> Result<Json<Vec<KnowledgeResult>>, ApiError> {
    if params.query.trim().is_empty() {
        return Err(ApiError::BadRequest("query cannot be empty".into()));
    }

    let kb = state.knowledge_base.read().await;
    let chunks = kb
        .search(&params.query, params.top_k)
        .await
        .map_err(|e| ApiError::Internal(format!("knowledge search failed: {e}")))?;

    let results = chunks
        .into_iter()
        .map(|c| KnowledgeResult {
            id: c.id,
            source: c.source,
            content: c.content,
        })
        .collect();

    Ok(Json(results))
}

/// GET /api/v1/memory/working?max_items=10 - 获取工作记忆
#[utoipa::path(get, path = "/api/v1/memory/working",
    params(WorkingMemoryQuery),
    responses((status = 200, body = Vec<WorkingMemoryItem>))
)]
pub async fn get_working_memory(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<WorkingMemoryQuery>,
) -> Json<Vec<WorkingMemoryItem>> {
    let wm = state.working_memory.read().await;
    let items = wm.get_active_context(params.max_items);
    Json(
        items
            .into_iter()
            .map(|content| WorkingMemoryItem { content })
            .collect(),
    )
}

/// 工作记忆查询参数
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct WorkingMemoryQuery {
    #[serde(default = "default_max_items")]
    pub max_items: usize,
}

fn default_max_items() -> usize {
    10
}

/// POST /api/v1/memory/working - 添加工作记忆
#[utoipa::path(post, path = "/api/v1/memory/working",
    request_body = AddWorkingMemoryRequest,
    responses((status = 201, description = "已添加"), (status = 400, description = "content 为空"))
)]
pub async fn add_working_memory(
    State(state): State<AppState>,
    Json(req): Json<AddWorkingMemoryRequest>,
) -> Result<StatusCode, ApiError> {
    if req.content.trim().is_empty() {
        return Err(ApiError::BadRequest("content cannot be empty".into()));
    }

    let mut wm = state.working_memory.write().await;
    wm.add(req.content, req.importance);
    Ok(StatusCode::CREATED)
}
