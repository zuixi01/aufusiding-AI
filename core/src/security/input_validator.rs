//! 输入验证器
//!
//! 提供通用输入验证功能，防止各类注入攻击。

use regex::Regex;
use std::sync::LazyLock;

use super::errors::ValidationError;

static DEVICE_ID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9\-_]+$").unwrap()
});

static SQL_INJECTION_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r#"(?i)(\bor\b|\band\b)\s+['"]?\d+['"]?\s*=\s*['"]?\d+"#).unwrap(),
        Regex::new(r"(?i)(union|select|insert|update|delete|drop|create|alter)\s+").unwrap(),
        Regex::new(r"[';]--").unwrap(),
        Regex::new(r"/\*.*\*/").unwrap(),
    ]
});

pub struct InputValidator {
    max_text_length: usize,
    max_array_size: usize,
    allow_control_chars: bool,
}

impl InputValidator {
    /// 创建默认配置的验证器
    pub fn new() -> Self {
        Self {
            max_text_length: 10000,
            max_array_size: 1000,
            allow_control_chars: false,
        }
    }

    /// 创建自定义配置的验证器
    pub fn with_config(max_text_length: usize, max_array_size: usize) -> Self {
        Self {
            max_text_length,
            max_array_size,
            allow_control_chars: false,
        }
    }

    /// 验证文本输入
    pub fn validate_text(&self, text: &str) -> Result<(), ValidationError> {
        // 空输入检查
        if text.is_empty() {
            return Err(ValidationError::EmptyInput);
        }

        // 长度检查
        if text.len() > self.max_text_length {
            return Err(ValidationError::TooLong);
        }

        // 控制字符检查
        if !self.allow_control_chars {
            for ch in text.chars() {
                if ch.is_control() && ch != '\n' && ch != '\t' && ch != '\r' {
                    return Err(ValidationError::ControlCharacter);
                }
            }
        }

        // SQL 注入检测
        for pattern in SQL_INJECTION_PATTERNS.iter() {
            if pattern.is_match(text) {
                tracing::warn!(
                    pattern = pattern.as_str(),
                    input_preview = &text[..text.len().min(100)],
                    "检测到 SQL 注入攻击"
                );
                return Err(ValidationError::FormatError("可能的 SQL 注入".into()));
            }
        }

        Ok(())
    }

    /// 验证设备 ID
    pub fn validate_device_id(&self, id: &str) -> Result<(), ValidationError> {
        if id.is_empty() {
            return Err(ValidationError::EmptyInput);
        }

        if id.len() > 64 {
            return Err(ValidationError::TooLong);
        }

        if !DEVICE_ID_REGEX.is_match(id) {
            return Err(ValidationError::InvalidDeviceId);
        }

        Ok(())
    }

    /// 验证命令名称
    pub fn validate_command(&self, cmd: &str) -> Result<(), ValidationError> {
        if cmd.is_empty() {
            return Err(ValidationError::EmptyInput);
        }

        if cmd.len() > 64 {
            return Err(ValidationError::TooLong);
        }

        // 命令只允许字母、数字和下划线
        if !cmd.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(ValidationError::InvalidCommand);
        }

        Ok(())
    }

    /// 验证数组大小
    pub fn validate_array_size(&self, size: usize) -> Result<(), ValidationError> {
        if size > self.max_array_size {
            return Err(ValidationError::TooLong);
        }
        Ok(())
    }

    /// 验证 JSON 字符串（防止嵌套炸弹）
    pub fn validate_json(&self, json: &str) -> Result<(), ValidationError> {
        // 长度检查
        if json.len() > self.max_text_length {
            return Err(ValidationError::TooLong);
        }

        // 嵌套深度检查（简单实现）
        let max_depth: i32 = 10;
        let mut depth: i32 = 0;
        let mut max_seen: i32 = 0;

        for ch in json.chars() {
            match ch {
                '{' | '[' => {
                    depth += 1;
                    max_seen = max_seen.max(depth);
                }
                '}' | ']' => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
        }

        if max_seen > max_depth {
            return Err(ValidationError::FormatError("JSON 嵌套过深".into()));
        }

        Ok(())
    }
}

impl Default for InputValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocks_sql_injection() {
        let validator = InputValidator::new();

        let attacks = vec![
            "'; DROP TABLE users; --",
            "1' OR '1'='1",
            "admin'--",
            "1 UNION SELECT * FROM passwords",
        ];

        for attack in attacks {
            assert!(validator.validate_text(attack).is_err(), "应阻止: {}", attack);
        }
    }

    #[test]
    fn test_blocks_control_characters() {
        let validator = InputValidator::new();

        let input_with_null = "hello\0world";
        assert!(validator.validate_text(input_with_null).is_err());

        let input_with_bell = "hello\x07world";
        assert!(validator.validate_text(input_with_bell).is_err());
    }

    #[test]
    fn test_allows_normal_text() {
        let validator = InputValidator::new();

        let normal_inputs = vec![
            "Hello, world!",
            "你好，世界！",
            "Line 1\nLine 2\tTabbed",
            "Email: test@example.com",
        ];

        for input in normal_inputs {
            assert!(validator.validate_text(input).is_ok(), "应允许: {}", input);
        }
    }

    #[test]
    fn test_device_id_validation() {
        let validator = InputValidator::new();

        // 有效的设备 ID
        assert!(validator.validate_device_id("light-001").is_ok());
        assert!(validator.validate_device_id("sensor_temp_01").is_ok());
        assert!(validator.validate_device_id("device123").is_ok());

        // 无效的设备 ID
        assert!(validator.validate_device_id("light 001").is_err()); // 空格
        assert!(validator.validate_device_id("light;001").is_err()); // 分号
        assert!(validator.validate_device_id("light/001").is_err()); // 斜杠
        assert!(validator.validate_device_id("").is_err()); // 空
    }

    #[test]
    fn test_command_validation() {
        let validator = InputValidator::new();

        // 有效命令
        assert!(validator.validate_command("turn_on").is_ok());
        assert!(validator.validate_command("set_brightness").is_ok());
        assert!(validator.validate_command("getState").is_ok());

        // 无效命令
        assert!(validator.validate_command("turn-on").is_err()); // 连字符
        assert!(validator.validate_command("turn on").is_err()); // 空格
        assert!(validator.validate_command("turn;on").is_err()); // 分号
        assert!(validator.validate_command("").is_err()); // 空
    }

    #[test]
    fn test_length_limits() {
        let validator = InputValidator::with_config(100, 10);

        let long_text = "a".repeat(101);
        assert!(validator.validate_text(&long_text).is_err());

        let ok_text = "a".repeat(100);
        assert!(validator.validate_text(&ok_text).is_ok());
    }

    #[test]
    fn test_array_size_validation() {
        let validator = InputValidator::with_config(10000, 100);

        assert!(validator.validate_array_size(50).is_ok());
        assert!(validator.validate_array_size(100).is_ok());
        assert!(validator.validate_array_size(101).is_err());
    }

    #[test]
    fn test_json_depth_validation() {
        let validator = InputValidator::new();

        // 正常深度
        let normal_json = r#"{"a": {"b": {"c": 1}}}"#;
        assert!(validator.validate_json(normal_json).is_ok());

        // 过深嵌套
        let deep_json = r#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":{"i":{"j":{"k":1}}}}}}}}}}}"#;
        assert!(validator.validate_json(deep_json).is_err());
    }

    #[test]
    fn test_empty_input() {
        let validator = InputValidator::new();

        assert!(validator.validate_text("").is_err());
        assert!(validator.validate_device_id("").is_err());
        assert!(validator.validate_command("").is_err());
    }
}
