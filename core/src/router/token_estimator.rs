//! Token 估算器
//!
//! 提供轻量、快速的 token 数估算，用于路由决策与上下文预算。

use crate::providers::traits::ChatMessage;

pub struct TokenEstimator;

impl TokenEstimator {
    pub fn estimate_text(text: &str) -> usize {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return 1;
        }

        let whitespace_words = trimmed.split_whitespace().count();
        let cjk_chars = trimmed.chars().filter(|c| is_cjk(*c)).count();

        let base = if whitespace_words > 0 {
            whitespace_words
        } else {
            trimmed.chars().count().div_ceil(2)
        };

        base.max(cjk_chars.div_ceil(2)).max(1)
    }

    pub fn estimate_messages(messages: &[ChatMessage]) -> usize {
        messages
            .iter()
            .map(|m| Self::estimate_text(&m.content) + 1)
            .sum::<usize>()
            .max(1)
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0xF900..=0xFAFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_text_for_english_sentence() {
        let tokens = TokenEstimator::estimate_text("hello world from prometheus");
        assert!(tokens >= 4);
    }

    #[test]
    fn estimate_text_for_cjk_sentence() {
        let tokens = TokenEstimator::estimate_text("请帮我总结今天的会议要点");
        assert!(tokens >= 4);
    }

    #[test]
    fn estimate_messages_accumulates_all_contents() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: "hello world".to_string(),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "sure, here is a short response".to_string(),
            },
        ];
        let tokens = TokenEstimator::estimate_messages(&messages);
        assert!(tokens >= 6);
    }
}
