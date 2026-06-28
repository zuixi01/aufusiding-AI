//! 上下文压缩器
//!
//! 当对话历史超出模型上下文窗口时，智能压缩早期对话，保留关键信息。

/// 压缩策略
#[derive(Debug, Clone)]
pub enum CompressionStrategy {
    /// 滑动窗口：保留最近 N 轮
    SlidingWindow(usize),
    /// 摘要压缩：用模型生成早期对话摘要
    Summarize,
    /// 混合：摘要 + 滑动窗口
    Hybrid { summary_threshold: usize },
}

/// 上下文压缩器
pub struct ContextCompressor {
    /// 压缩策略
    pub strategy: CompressionStrategy,
    /// 最大 token 数
    pub max_tokens: usize,
}

impl ContextCompressor {
    /// 创建上下文压缩器
    pub fn new(max_tokens: usize) -> Self {
        Self {
            strategy: CompressionStrategy::Hybrid {
                summary_threshold: 10,
            },
            max_tokens,
        }
    }

    /// 压缩对话上下文，返回压缩后的消息列表
    pub async fn compress(
        &self,
        _messages: &[crate::session::history::Message],
    ) -> anyhow::Result<Vec<crate::session::history::Message>> {
        if _messages.is_empty() {
            return Ok(Vec::new());
        }

        let compressed = match self.strategy {
            CompressionStrategy::SlidingWindow(max_rounds) => {
                let keep = max_rounds.saturating_mul(2);
                let start = _messages.len().saturating_sub(keep);
                _messages[start..].to_vec()
            }
            CompressionStrategy::Summarize => truncate_by_token_budget(_messages, self.max_tokens),
            CompressionStrategy::Hybrid { summary_threshold } => {
                if _messages.len() <= summary_threshold {
                    _messages.to_vec()
                } else {
                    truncate_by_token_budget(_messages, self.max_tokens)
                }
            }
        };

        Ok(compressed)
    }
}

fn truncate_by_token_budget(
    messages: &[crate::session::history::Message],
    token_budget: usize,
) -> Vec<crate::session::history::Message> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut total = 0usize;
    let mut kept_start_index = messages.len();

    for (i, message) in messages.iter().enumerate().rev() {
        let tokens = rough_token_count(&message.content);
        if total + tokens > token_budget {
            break;
        }
        total += tokens;
        kept_start_index = i;
    }

    let dropped = &messages[..kept_start_index];
    let kept = &messages[kept_start_index..];

    if dropped.is_empty() {
        return kept.to_vec();
    }

    // Generate a summary message for the dropped (earlier) messages
    let summary_text = generate_summary(dropped);
    let summary_msg = crate::session::history::Message {
        id: uuid::Uuid::new_v4(),
        session_id: dropped[0].session_id,
        role: crate::session::history::Role::System,
        content: summary_text,
        timestamp: dropped.last().unwrap().timestamp,
    };

    let mut result = vec![summary_msg];
    result.extend_from_slice(kept);
    result
}

/// Generate a concise summary of dropped messages.
/// In production this would call an LLM; here we produce a deterministic summary.
fn generate_summary(messages: &[crate::session::history::Message]) -> String {
    let total = messages.len();
    let speakers: std::collections::HashSet<&str> = messages
        .iter()
        .map(|m| match m.role {
            crate::session::history::Role::User => "user",
            crate::session::history::Role::Assistant => "assistant",
            crate::session::history::Role::System => "system",
            crate::session::history::Role::Tool => "tool",
        })
        .collect();
    let roles: Vec<&str> = speakers.into_iter().collect();

    let mut topics = Vec::new();
    for msg in messages {
        let trimmed = msg.content.trim();
        if !trimmed.is_empty() {
            // Take first 50 chars as a topic hint
            let snippet: String = trimmed.chars().take(50).collect();
            topics.push(snippet);
        }
    }

    let topics_preview = if topics.len() <= 3 {
        topics.join("; ")
    } else {
        let first_three: Vec<&str> = topics.iter().take(3).map(|s| s.as_str()).collect();
        format!("{}; ... and {} more", first_three.join("; "), topics.len() - 3)
    };

    format!(
        "[Summary of {} earlier messages between {}] {}",
        total,
        roles.join(", "),
        topics_preview
    )
}

fn rough_token_count(text: &str) -> usize {
    let word_count = text.split_whitespace().count();
    if word_count == 0 {
        1
    } else {
        word_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::history::{Message, Role};
    use uuid::Uuid;

    fn msg(content: &str) -> Message {
        Message {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            role: Role::User,
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn sliding_window_keeps_last_n() {
        let mut compressor = ContextCompressor::new(9999);
        compressor.strategy = CompressionStrategy::SlidingWindow(2);
        let messages: Vec<Message> = (0..6).map(|i| msg(&format!("msg-{i}"))).collect();
        let result = compressor.compress(&messages).await.unwrap();
        // 2 rounds = 4 messages kept (last 4)
        assert_eq!(result.len(), 4);
        assert!(result[0].content.contains("msg-2"));
    }

    #[tokio::test]
    async fn summarize_strategy_truncates_by_budget() {
        let mut compressor = ContextCompressor::new(3);
        compressor.strategy = CompressionStrategy::Summarize;
        let messages = vec![msg("one two three four five"), msg("alpha beta"), msg("x")];
        let result = compressor.compress(&messages).await.unwrap();
        // budget=3 tokens: "x"(1) + "alpha beta"(2) = 3, fits
        // dropped message gets a summary prepended => 1 summary + 2 kept = 3
        assert_eq!(result.len(), 3);
        assert!(result[0].content.contains("[Summary of"));
        assert_eq!(result[1].content, "alpha beta");
        assert_eq!(result[2].content, "x");
    }

    #[tokio::test]
    async fn hybrid_strategy_switches_at_threshold() {
        let mut compressor = ContextCompressor::new(5);
        compressor.strategy = CompressionStrategy::Hybrid {
            summary_threshold: 3,
        };
        // Below threshold: keep all
        let small = vec![msg("a"), msg("b")];
        let result = compressor.compress(&small).await.unwrap();
        assert_eq!(result.len(), 2);

        // Above threshold: truncate by budget
        let large: Vec<Message> = (0..5).map(|i| msg(&format!("word-{i}"))).collect();
        let result = compressor.compress(&large).await.unwrap();
        assert!(result.len() <= 5);
    }

    #[tokio::test]
    async fn compress_empty_messages() {
        let compressor = ContextCompressor::new(100);
        let result = compressor.compress(&[]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn compress_single_message() {
        let compressor = ContextCompressor::new(100);
        let messages = vec![msg("hello world")];
        let result = compressor.compress(&messages).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn token_count_estimation_reasonable() {
        // English: word count
        assert_eq!(rough_token_count("hello world foo"), 3);
        // Empty: returns 1
        assert_eq!(rough_token_count(""), 1);
        // Single word
        assert_eq!(rough_token_count("hello"), 1);
        // CJK without spaces: treated as 1 word (rough estimate)
        let cjk_count = rough_token_count("你好世界");
        assert!(cjk_count >= 1);
    }
}
