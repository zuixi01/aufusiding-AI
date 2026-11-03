//! 隐私保护
//!
//! 语音数据本地处理保证 + 敏感信息脱敏。

/// 隐私保护器
pub struct PrivacyGuard {
    /// 敏感信息模式（正则）
    patterns: Vec<(String, String)>, // (pattern_name, replacement)
}

impl PrivacyGuard {
    pub fn new() -> Self {
        Self {
            patterns: vec![
                ("phone".into(), "[电话号码]".into()),
                ("id_card".into(), "[身份证号]".into()),
                ("bank_card".into(), "[银行卡号]".into()),
                ("email".into(), "[邮箱]".into()),
            ],
        }
    }

    /// 脱敏文本
    pub fn sanitize(&self, text: &str) -> String {
        let mut result = text.to_string();

        // 手机号脱敏（1开头的11位数字）
        if let Ok(re) = regex::Regex::new(r"1[3-9]\d{9}") {
            result = re.replace_all(&result, "[电话号码]").to_string();
        }

        // 身份证号脱敏（18位）
        if let Ok(re) = regex::Regex::new(r"\d{17}[\dXx]") {
            result = re.replace_all(&result, "[身份证号]").to_string();
        }

        // 银行卡号脱敏（16-19位数字）
        if let Ok(re) = regex::Regex::new(r"\d{16,19}") {
            result = re.replace_all(&result, "[银行卡号]").to_string();
        }

        // 邮箱脱敏
        if let Ok(re) = regex::Regex::new(r"[\w.+-]+@[\w-]+\.[\w.]+") {
            result = re.replace_all(&result, "[邮箱]").to_string();
        }

        result
    }

    /// 检查文本是否包含敏感信息
    pub fn contains_sensitive(&self, text: &str) -> bool {
        // 手机号
        if regex::Regex::new(r"1[3-9]\d{9}")
            .map(|re| re.is_match(text))
            .unwrap_or(false)
        {
            return true;
        }
        // 身份证
        if regex::Regex::new(r"\d{17}[\dXx]")
            .map(|re| re.is_match(text))
            .unwrap_or(false)
        {
            return true;
        }
        false
    }

    /// 模式数
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }
}
