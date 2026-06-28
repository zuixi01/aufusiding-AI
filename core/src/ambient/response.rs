//! 响应格式化器
//!
//! 将 Agent/编排器输出转为精简 TTS 友好摘要。

/// 响应格式化器 — 将长文本转为语音友好的精简摘要
pub struct ResponseFormatter {
    /// 最大语音输出字符数
    max_chars: usize,
    /// 是否添加确认前缀（"好的，"）
    add_confirmation_prefix: bool,
}

impl ResponseFormatter {
    pub fn new() -> Self {
        Self {
            max_chars: 200,
            add_confirmation_prefix: true,
        }
    }

    pub fn with_max_chars(mut self, max: usize) -> Self {
        self.max_chars = max;
        self
    }

    pub fn with_confirmation_prefix(mut self, enabled: bool) -> Self {
        self.add_confirmation_prefix = enabled;
        self
    }

    /// 格式化响应为 TTS 友好文本
    pub fn format(&self, raw_response: &str, is_action_result: bool) -> String {
        if raw_response.is_empty() {
            return if is_action_result {
                "已完成。".to_string()
            } else {
                "抱歉，我没有获取到结果。".to_string()
            };
        }

        let cleaned = self.clean_for_speech(raw_response);
        let truncated = self.truncate(&cleaned);

        if is_action_result && self.add_confirmation_prefix {
            format!("好的，{}", truncated)
        } else {
            truncated
        }
    }

    /// 清理文本使其适合语音输出
    fn clean_for_speech(&self, text: &str) -> String {
        let mut result = text.to_string();
        // 移除 Markdown 格式
        result = result.replace("**", "");
        result = result.replace("##", "");
        result = result.replace("- ", "，");
        // 移除 URL
        result = regex::Regex::new(r"https?://\S+")
            .map(|re| re.replace_all(&result, "链接已省略").to_string())
            .unwrap_or(result);
        // 移除多余空白
        result = result.split_whitespace().collect::<Vec<_>>().join(" ");
        result.trim().to_string()
    }

    /// 截断到最大长度，在句子边界截断
    fn truncate(&self, text: &str) -> String {
        if text.chars().count() <= self.max_chars {
            return text.to_string();
        }

        let chars: Vec<char> = text.chars().take(self.max_chars).collect();
        let s: String = chars.into_iter().collect();

        // 尝试在句子边界截断
        if let Some(pos) = s.rfind(['。', '！', '？', '；']) {
            let truncated: String = s.chars().take(pos + 1).collect();
            return truncated;
        }
        // 在逗号处截断
        if let Some(pos) = s.rfind('，') {
            let truncated: String = s.chars().take(pos + 1).collect();
            return format!("{}等等。", truncated);
        }

        format!("{}...", s)
    }
}
