//! 首次启动引导 — 检测 Ollama、安装、拉模型、完成设置

use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

use crate::commands::AppState;
use prometheus_core::orchestrator::ProviderChatBackend;
use prometheus_core::providers::ollama::OllamaProvider;
use prometheus_core::providers::registry::ProviderRegistry;
use prometheus_core::router::fallback::FallbackRouter;
use prometheus_core::router::runtime::RoutingRuntime;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct SetupStatus {
    pub ollama_installed: bool,
    pub ollama_running: bool,
    pub models: Vec<String>,
    pub chat_ready: bool,
    pub embed_ready: bool,
    pub setup_complete: bool,
}

fn setup_marker_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".prometheus")
        .join("desktop")
        .join(".setup_done")
}

fn is_setup_complete() -> bool {
    setup_marker_path().exists()
}

/// 检测 Ollama 安装 / 运行 / 模型状态
#[tauri::command]
pub async fn check_setup_status() -> SetupStatus {
    let setup_complete = is_setup_complete();

    // 检测 ollama 是否安装
    let ollama_installed = std::process::Command::new("which")
        .arg("ollama")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    // 检测 ollama 服务是否运行
    let mut ollama_running = false;
    let mut models: Vec<String> = Vec::new();

    if ollama_installed {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();

        if let Ok(resp) = client.get("http://localhost:11434/api/tags").send().await {
            if resp.status().is_success() {
                ollama_running = true;
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(arr) = body.get("models").and_then(|v| v.as_array()) {
                        models = arr
                            .iter()
                            .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
                            .map(|s| s.to_string())
                            .collect();
                    }
                }
            }
        }
    }

    let chat_ready = models.iter().any(|m| m.starts_with("qwen2.5:1.5b"));
    let embed_ready = models.iter().any(|m| m.starts_with("all-minilm"));

    SetupStatus {
        ollama_installed,
        ollama_running,
        models,
        chat_ready,
        embed_ready,
        setup_complete,
    }
}

/// 安装 Ollama（调用官方安装脚本）
#[tauri::command]
pub async fn install_ollama() -> Result<String, String> {
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg("curl -fsSL https://ollama.com/install.sh | sh")
        .output()
        .await
        .map_err(|e| format!("执行安装命令失败: {e}"))?;

    if output.status.success() {
        // 等待 ollama 服务启动
        tokio::time::sleep(Duration::from_secs(2)).await;
        Ok("Ollama 安装成功".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("安装失败: {stderr}"))
    }
}

/// 拉取模型，返回状态消息
#[tauri::command]
pub async fn pull_model(model: String) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post("http://localhost:11434/api/pull")
        .json(&serde_json::json!({ "name": model, "stream": false }))
        .send()
        .await
        .map_err(|e| format!("拉取模型失败: {e}"))?;

    if resp.status().is_success() {
        Ok(format!("模型 {model} 下载完成"))
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(format!("拉取失败: {body}"))
    }
}

/// 完成设置：写标记文件，重建 provider
#[tauri::command]
pub async fn complete_setup(state: State<'_, AppState>) -> Result<String, String> {
    // 写标记文件
    let marker = setup_marker_path();
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    std::fs::write(&marker, "done").map_err(|e| format!("写标记文件失败: {e}"))?;

    // 重建 orchestrator，使用真实 OllamaProvider
    rebuild_with_ollama(&state).map_err(|e| format!("重建 provider 失败: {e}"))?;

    Ok("设置完成".to_string())
}

/// 用 OllamaProvider 替换当前 orchestrator 中的 provider
fn rebuild_with_ollama(state: &AppState) -> anyhow::Result<()> {
    let provider = OllamaProvider::new();
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(provider));

    let fallback =
        FallbackRouter::with_config(vec!["ollama".to_string()], 2, Duration::from_secs(120));
    let routing = RoutingRuntime::new(registry, fallback);
    let backend = ProviderChatBackend::new(routing);

    let base_dir = crate::commands::default_data_dir();
    let history_db = base_dir.join("history.db");
    let session_db = base_dir.join("sessions.db");
    let history = prometheus_core::session::HistoryStore::new(history_db);

    let orchestrator = prometheus_core::orchestrator::ChatOrchestrator::new_with_persistence(
        session_db, history, backend,
    )?;

    *state.orchestrator.lock().unwrap() = orchestrator;
    *state.active_model.lock().unwrap() = "qwen2.5:1.5b".to_string();

    Ok(())
}
