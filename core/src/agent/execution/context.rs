//! Agent 上下文管理 — 对话历史、工具调用结果

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 上下文消息角色
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRole {
    System,
    User,
    Assistant,
    Tool,
}

/// 上下文消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    pub role: ContextRole,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl ContextMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ContextRole::System,
            content: content.into(),
            tool_call_id: None,
            timestamp: Utc::now(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ContextRole::User,
            content: content.into(),
            tool_call_id: None,
            timestamp: Utc::now(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ContextRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            timestamp: Utc::now(),
        }
    }

    pub fn tool(content: impl Into<String>, call_id: impl Into<String>) -> Self {
        Self {
            role: ContextRole::Tool,
            content: content.into(),
            tool_call_id: Some(call_id.into()),
            timestamp: Utc::now(),
        }
    }
}

/// Agent 上下文 — 管理对话历史和工具调用
#[derive(Debug, Clone)]
pub struct AgentContext {
    messages: Vec<ContextMessage>,
    max_messages: usize,
    /// token 预算上限
    pub token_budget: usize,
    /// 达到此值触发压缩
    pub compress_threshold: usize,
}

impl AgentContext {
    pub fn new(system_prompt: impl Into<String>) -> Self {
        let mut ctx = Self {
            messages: Vec::new(),
            max_messages: 100,
            token_budget: 4096,
            compress_threshold: 3072,
        };
        ctx.messages.push(ContextMessage::system(system_prompt));
        ctx
    }

    pub fn with_max_messages(mut self, max: usize) -> Self {
        self.max_messages = max;
        self
    }

    /// 添加消息
    pub fn push(&mut self, message: ContextMessage) {
        self.messages.push(message);
        self.trim();
    }

    /// 添加用户消息（任务指令）
    pub fn push_task(&mut self, task_description: &str) {
        self.push(ContextMessage::user(task_description));
    }

    /// 添加助手回复
    pub fn push_response(&mut self, content: &str) {
        self.push(ContextMessage::assistant(content));
    }

    /// 添加工具调用结果（超过 500 字符自动截断）
    pub fn push_tool_result(&mut self, call_id: &str, result: &Value) {
        let mut content = serde_json::to_string(result).unwrap_or_default();
        if content.len() > 500 {
            content.truncate(500);
            content.push_str(" [truncated]");
        }
        self.push(ContextMessage::tool(content, call_id));
    }

    /// 获取所有消息（供 LLM 调用）
    pub fn messages(&self) -> &[ContextMessage] {
        &self.messages
    }

    /// 消息数量
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// 获取最后一条助手消息
    pub fn last_assistant_message(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == ContextRole::Assistant)
            .map(|m| m.content.as_str())
    }

    /// 裁剪历史（保留 system prompt + 最近消息）
    fn trim(&mut self) {
        if self.messages.len() > self.max_messages {
            // 保留第一条 system prompt + 最近的消息
            let keep = self.max_messages - 1;
            let start = self.messages.len() - keep;
            let system = self.messages[0].clone();
            self.messages = std::iter::once(system)
                .chain(self.messages[start..].iter().cloned())
                .collect();
        }
    }

    /// 估算当前上下文 token 数
    pub fn estimated_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| rough_token_count(&m.content))
            .sum()
    }

    /// 压缩上下文：保留 system prompt + 最近 4 条，中间消息合并摘要
    pub fn compress(&mut self) {
        if self.estimated_tokens() <= self.compress_threshold {
            return;
        }
        if self.messages.len() <= 5 {
            return;
        }

        let system = self.messages[0].clone();
        let recent_start = self.messages.len().saturating_sub(4);
        let recent: Vec<ContextMessage> = self.messages[recent_start..].to_vec();

        // 中间消息合并摘要
        let middle = &self.messages[1..recent_start];
        let mut summaries = Vec::new();
        for chunk in middle.chunks(2) {
            let summary_content: String = chunk
                .iter()
                .map(|m| {
                    let truncated: String = m.content.chars().take(50).collect();
                    if m.content.chars().count() > 50 {
                        format!("[{:?}] {}...", m.role, truncated)
                    } else {
                        format!("[{:?}] {}", m.role, truncated)
                    }
                })
                .collect::<Vec<_>>()
                .join(" | ");
            summaries.push(ContextMessage::system(format!("[摘要] {}", summary_content)));
        }

        self.messages = std::iter::once(system)
            .chain(summaries)
            .chain(recent)
            .collect();
    }
}

/// 粗略 token 估算
fn rough_token_count(text: &str) -> usize {
    let count = text.split_whitespace().count();
    if count == 0 { 1 } else { count }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_starts_with_system_prompt() {
        let ctx = AgentContext::new("你是架构师");
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx.messages()[0].role, ContextRole::System);
        assert_eq!(ctx.messages()[0].content, "你是架构师");
    }

    #[test]
    fn push_messages() {
        let mut ctx = AgentContext::new("system");
        ctx.push_task("设计缓存系统");
        ctx.push_response("好的，我来设计");
        assert_eq!(ctx.len(), 3);
    }

    #[test]
    fn push_tool_result() {
        let mut ctx = AgentContext::new("system");
        ctx.push_tool_result("call-1", &serde_json::json!({"result": "ok"}));
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx.messages()[1].role, ContextRole::Tool);
        assert_eq!(ctx.messages()[1].tool_call_id, Some("call-1".into()));
    }

    #[test]
    fn last_assistant_message() {
        let mut ctx = AgentContext::new("system");
        assert!(ctx.last_assistant_message().is_none());
        ctx.push_response("first");
        ctx.push_task("question");
        ctx.push_response("second");
        assert_eq!(ctx.last_assistant_message(), Some("second"));
    }

    #[test]
    fn trim_preserves_system_prompt() {
        let mut ctx = AgentContext::new("system").with_max_messages(5);
        for i in 0..10 {
            ctx.push_task(&format!("msg-{}", i));
        }
        assert_eq!(ctx.len(), 5);
        assert_eq!(ctx.messages()[0].role, ContextRole::System);
        assert_eq!(ctx.messages()[0].content, "system");
    }

    #[test]
    fn context_message_constructors() {
        let s = ContextMessage::system("sys");
        assert_eq!(s.role, ContextRole::System);
        let u = ContextMessage::user("usr");
        assert_eq!(u.role, ContextRole::User);
        let a = ContextMessage::assistant("ast");
        assert_eq!(a.role, ContextRole::Assistant);
        let t = ContextMessage::tool("res", "id-1");
        assert_eq!(t.role, ContextRole::Tool);
        assert_eq!(t.tool_call_id, Some("id-1".into()));
    }

    #[test]
    fn compress_preserves_system_prompt() {
        let mut ctx = AgentContext::new("system prompt");
        ctx.compress_threshold = 5;
        // 添加足够多的消息触发压缩
        for i in 0..20 {
            ctx.push_task(&format!("message number {} with some extra words here", i));
        }
        ctx.compress();
        assert_eq!(ctx.messages()[0].role, ContextRole::System);
        assert_eq!(ctx.messages()[0].content, "system prompt");
    }

    #[test]
    fn compress_reduces_token_count() {
        let mut ctx = AgentContext::new("system");
        ctx.compress_threshold = 10;
        ctx.token_budget = 4096;
        // 添加大量长消息确保压缩有效
        for i in 0..40 {
            ctx.push_task(&format!(
                "this is a very long message with many words for task number {} to ensure we exceed the threshold significantly",
                i
            ));
        }
        let before = ctx.estimated_tokens();
        ctx.compress();
        let after = ctx.estimated_tokens();
        assert!(after < before, "tokens should decrease: {} < {}", after, before);
    }

    #[test]
    fn tool_result_truncation() {
        let mut ctx = AgentContext::new("system");
        let long_value = serde_json::json!({"data": "x".repeat(600)});
        ctx.push_tool_result("call-1", &long_value);
        let content = &ctx.messages()[1].content;
        assert!(content.len() <= 520); // 500 + " [truncated]"
        assert!(content.ends_with("[truncated]"));
    }

    #[test]
    fn no_compress_below_threshold() {
        let mut ctx = AgentContext::new("system");
        ctx.compress_threshold = 99999;
        ctx.push_task("short");
        ctx.push_response("reply");
        let len_before = ctx.len();
        ctx.compress();
        assert_eq!(ctx.len(), len_before);
    }
}
