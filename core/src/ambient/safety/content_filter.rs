//! 内容过滤器
//!
//! 儿童模式内容过滤规则。

/// 内容过滤器
pub struct ContentFilter {
    /// 屏蔽关键词
    blocked_keywords: Vec<String>,
}

impl ContentFilter {
    pub fn new() -> Self {
        Self {
            blocked_keywords: vec![
                "暴力".into(),
                "赌博".into(),
                "色情".into(),
                "毒品".into(),
                "恐怖".into(),
                "自杀".into(),
                "武器".into(),
            ],
        }
    }

    /// 添加屏蔽关键词
    pub fn add_keyword(&mut self, keyword: &str) {
        if !self.blocked_keywords.contains(&keyword.to_string()) {
            self.blocked_keywords.push(keyword.to_string());
        }
    }

    /// 移除屏蔽关键词
    pub fn remove_keyword(&mut self, keyword: &str) {
        self.blocked_keywords.retain(|k| k != keyword);
    }

    /// 检查文本是否包含屏蔽内容
    pub fn contains_blocked(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        self.blocked_keywords.iter().any(|kw| lower.contains(kw))
    }

    /// 过滤文本，替换屏蔽词为 ***
    pub fn filter_text(&self, text: &str) -> String {
        let mut result = text.to_string();
        for kw in &self.blocked_keywords {
            if result.contains(kw) {
                result = result.replace(kw, "***");
            }
        }
        result
    }

    /// 屏蔽关键词数
    pub fn keyword_count(&self) -> usize {
        self.blocked_keywords.len()
    }
}
