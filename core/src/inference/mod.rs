//! 本地推理模块
//!
//! 抽象推理后端接口，支持 llama.cpp 和 MLX 两种本地推理引擎。
//! 包含 Prompt 缓存以降低重复推理成本。

pub mod backend;
pub mod cache;
pub mod http_backend;
pub mod llama;
pub mod mlx;

pub use backend::{InferenceBackend, InferenceRequest, InferenceResponse, MockInferenceBackend};
pub use cache::{CacheStats, PromptCache};
pub use http_backend::{HttpInferenceBackend, HttpInferenceConfig};
