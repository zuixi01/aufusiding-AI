//! Provider trait 定义
//!
//! 所有云端 API 适配器必须实现的统一接口。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// 聊天补全请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

/// 聊天补全响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
}

/// Token 用量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// 云端 Provider trait
#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider 名称
    fn name(&self) -> &str;

    /// 支持的模型列表
    fn available_models(&self) -> Vec<String>;

    /// 发送聊天补全请求
    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatResponse>;

    /// 生成文本嵌入向量
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;

    /// 检查 API 密钥是否有效
    async fn validate_key(&self) -> anyhow::Result<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_serde_roundtrip() {
        let req = ChatRequest {
            model: "mock-chat-1".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hello".to_string(),
            }],
            max_tokens: Some(128),
            temperature: Some(0.7),
            stream: true,
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: ChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.model, "mock-chat-1");
        assert_eq!(decoded.messages.len(), 1);
        assert!(decoded.stream);
    }

    #[test]
    fn chat_response_serde_roundtrip() {
        let resp = ChatResponse {
            content: "ok".to_string(),
            model: "mock-chat-1".to_string(),
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
        };

        let json = serde_json::to_string(&resp).unwrap();
        let decoded: ChatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.content, "ok");
        assert_eq!(decoded.usage.total_tokens, 15);
    }
}
