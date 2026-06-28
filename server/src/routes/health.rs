//! 健康检查与服务信息端点

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;
use std::sync::OnceLock;
use std::time::Instant;
use utoipa::ToSchema;

use crate::state::AppState;

static START_TIME: OnceLock<Instant> = OnceLock::new();

/// 在服务器启动时调用，记录启动时间
pub fn init_start_time() {
    START_TIME.get_or_init(Instant::now);
}

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    status: &'static str,
    uptime_secs: u64,
}

#[derive(Serialize, ToSchema)]
pub struct InfoResponse {
    name: &'static str,
    version: &'static str,
    description: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct ComponentHealth {
    name: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    latency_ms: f64,
}

#[derive(Serialize, ToSchema)]
pub struct DeepHealthResponse {
    status: String,
    uptime_secs: u64,
    components: Vec<ComponentHealth>,
}

/// GET /api/v1/health
#[utoipa::path(get, path = "/api/v1/health", responses((status = 200, body = HealthResponse)))]
pub async fn health() -> impl IntoResponse {
    let uptime = START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0);

    // 关机期间返回 503，通知负载均衡器移除此实例
    if crate::SHUTTING_DOWN.load(std::sync::atomic::Ordering::SeqCst) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "shutting_down",
                uptime_secs: uptime,
            }),
        );
    }

    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok",
            uptime_secs: uptime,
        }),
    )
}

/// GET /api/v1/info
#[utoipa::path(get, path = "/api/v1/info", responses((status = 200, body = InfoResponse)))]
pub async fn info() -> impl IntoResponse {
    Json(InfoResponse {
        name: "prometheus-ai",
        version: env!("CARGO_PKG_VERSION"),
        description: "Prometheus AI HTTP API 服务器",
    })
}

/// GET /api/v1/health/deep — 深度健康检查，验证各子系统
#[utoipa::path(get, path = "/api/v1/health/deep", responses(
    (status = 200, body = DeepHealthResponse, description = "所有组件健康"),
    (status = 503, body = DeepHealthResponse, description = "部分组件异常"),
))]
pub async fn deep_health(State(state): State<AppState>) -> impl IntoResponse {
    let uptime = START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0);

    let mut components = Vec::new();

    // 检查 Orchestrator（会话管理）
    let start = Instant::now();
    let orch_ok = {
        let orch = state.orchestrator.read().await;
        let _ = orch.sessions();
        true
    };
    components.push(ComponentHealth {
        name: "orchestrator".into(),
        status: if orch_ok { "ok" } else { "error" }.into(),
        message: if orch_ok {
            None
        } else {
            Some("session store unavailable".into())
        },
        latency_ms: start.elapsed().as_secs_f64() * 1000.0,
    });

    // 检查 EpisodicMemory
    let start = Instant::now();
    let episodic_ok = {
        let em = state.episodic_memory.read().await;
        em.count().is_ok()
    };
    components.push(ComponentHealth {
        name: "episodic_memory".into(),
        status: if episodic_ok { "ok" } else { "error" }.into(),
        message: if episodic_ok {
            None
        } else {
            Some("episodic store unavailable".into())
        },
        latency_ms: start.elapsed().as_secs_f64() * 1000.0,
    });

    // 检查 KnowledgeBase
    let start = Instant::now();
    let kb_ok = {
        let kb = state.knowledge_base.read().await;
        kb.count().is_ok()
    };
    components.push(ComponentHealth {
        name: "knowledge_base".into(),
        status: if kb_ok { "ok" } else { "error" }.into(),
        message: if kb_ok {
            None
        } else {
            Some("knowledge store unavailable".into())
        },
        latency_ms: start.elapsed().as_secs_f64() * 1000.0,
    });

    // 检查 WorkingMemory（内存结构，始终可用）
    let start = Instant::now();
    let wm_len = {
        let wm = state.working_memory.read().await;
        wm.len()
    };
    components.push(ComponentHealth {
        name: "working_memory".into(),
        status: "ok".into(),
        message: Some(format!("{wm_len} items")),
        latency_ms: start.elapsed().as_secs_f64() * 1000.0,
    });

    let all_ok = components.iter().all(|c| c.status == "ok");
    let status_code = if all_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(DeepHealthResponse {
            status: if all_ok {
                "healthy".into()
            } else {
                "degraded".into()
            },
            uptime_secs: uptime,
            components,
        }),
    )
}

/// GET /metrics - Prometheus 指标导出
pub async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        state.metrics.export(),
    )
}
