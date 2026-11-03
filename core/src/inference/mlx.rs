//! MLX 绑定 (Apple Silicon)
//!
//! 通过 MLX 框架在 Apple Silicon 上进行高效本地推理。
//! 调用 python3 -m mlx_lm.generate 子进程执行推理。
//! 仅适用于 macOS/iOS (Apple M 系列芯片)。

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// MLX 推理后端
pub struct MlxBackend {
    /// 模型路径
    model_path: Option<PathBuf>,
    /// 是否使用 Metal GPU 加速
    use_metal: bool,
    /// 估算内存占用
    memory_bytes: usize,
}

impl MlxBackend {
    /// 创建 MLX 后端（仅 Apple Silicon）
    pub fn new(use_metal: bool) -> Self {
        Self {
            model_path: None,
            use_metal,
            memory_bytes: 0,
        }
    }

    /// 检查当前设备是否支持 MLX
    pub fn is_available() -> bool {
        cfg!(target_os = "macos") || cfg!(target_os = "ios")
    }
}

#[async_trait]
impl super::backend::InferenceBackend for MlxBackend {
    fn name(&self) -> &str {
        "mlx"
    }

    async fn load_model(&mut self, model_path: &Path) -> anyhow::Result<()> {
        if !Self::is_available() {
            anyhow::bail!("MLX is only available on Apple platforms");
        }
        if !model_path.exists() {
            anyhow::bail!("model file not found: {}", model_path.display());
        }

        self.model_path = Some(model_path.to_path_buf());
        let base = 384 * 1024 * 1024usize;
        self.memory_bytes = if self.use_metal {
            base + 64 * 1024 * 1024
        } else {
            base
        };
        Ok(())
    }

    async fn infer(
        &self,
        request: &super::backend::InferenceRequest,
    ) -> anyhow::Result<super::backend::InferenceResponse> {
        let model_path = self
            .model_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("mlx backend model not loaded"))?;
        let start = Instant::now();
        let prompt = request.prompt.trim();

        let output = tokio::process::Command::new("python3")
            .arg("-m")
            .arg("mlx_lm.generate")
            .arg("--model")
            .arg(model_path.to_string_lossy().as_ref())
            .arg("--prompt")
            .arg(prompt)
            .arg("--max-tokens")
            .arg(request.max_tokens.to_string())
            .arg("--temp")
            .arg(format!("{:.2}", request.temperature))
            .output()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to run python3 -m mlx_lm.generate: {}. \
                     Ensure python3 and mlx_lm are installed.",
                    e
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("mlx_lm.generate failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let text = stdout.trim().to_string();
        let tokens_used = prompt.split_whitespace().count() + request.max_tokens;

        Ok(super::backend::InferenceResponse {
            text,
            tokens_used,
            latency_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn unload(&mut self) -> anyhow::Result<()> {
        self.model_path = None;
        self.memory_bytes = 0;
        Ok(())
    }

    fn memory_usage(&self) -> usize {
        self.memory_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::backend::InferenceBackend;

    #[tokio::test]
    async fn infer_requires_loaded_model() {
        let backend = MlxBackend::new(true);
        let result = backend
            .infer(&super::super::backend::InferenceRequest {
                prompt: "hello".to_string(),
                max_tokens: 32,
                temperature: 0.7,
                top_p: 0.9,
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn load_fails_on_non_apple_platform() {
        if MlxBackend::is_available() {
            return;
        }

        let mut backend = MlxBackend::new(true);
        let model_path = crate::test_support::temp_path("mlx-test-model");
        let _ = std::fs::write(&model_path, b"mlx");
        let result = backend.load_model(&model_path).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn load_nonexistent_model_errors() {
        let mut backend = MlxBackend::new(false);
        let result = backend
            .load_model(std::path::Path::new("/nonexistent/mlx-model"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn unload_clears_memory() {
        let mut backend = MlxBackend::new(false);
        backend.unload().await.unwrap();
        assert_eq!(backend.memory_usage(), 0);
        assert!(backend.model_path.is_none());
    }

    #[tokio::test]
    async fn mlx_apple_silicon_only_check() {
        let available = MlxBackend::is_available();
        if !cfg!(target_os = "macos") && !cfg!(target_os = "ios") {
            assert!(!available);
        }
    }
}
