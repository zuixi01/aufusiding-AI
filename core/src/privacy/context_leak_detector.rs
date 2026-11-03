//! 上下文泄露检测器
//!
//! 检测文件路径、环境变量、内网 IP、堆栈跟踪等上下文信息泄露。

use regex::Regex;

use super::pii_detector::{PiiEntity, PiiType};

/// 上下文泄露检测器
pub struct ContextLeakDetector {
    /// 敏感环境变量前缀
    sensitive_env_prefixes: Vec<String>,
    /// 文件路径正则
    file_path_re: Regex,
    /// Windows 文件路径正则
    win_path_re: Regex,
    /// 环境变量正则 (Unix)
    env_var_unix_re: Regex,
    /// 环境变量正则 (Windows)
    env_var_win_re: Regex,
    /// 内网 IPv4 正则
    internal_ip_re: Regex,
    /// 堆栈跟踪正则
    stack_trace_re: Regex,
}

impl Default for ContextLeakDetector {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl ContextLeakDetector {
    pub fn new(custom_env_prefixes: Vec<String>) -> Self {
        let mut prefixes = vec![
            "AWS_".to_string(),
            "AZURE_".to_string(),
            "GCP_".to_string(),
            "GOOGLE_".to_string(),
            "DB_".to_string(),
            "DATABASE_".to_string(),
            "SECRET_".to_string(),
            "PRIVATE_".to_string(),
        ];
        prefixes.extend(custom_env_prefixes);

        Self {
            sensitive_env_prefixes: prefixes,
            file_path_re: Regex::new(
                r"(?:/home/|/Users/|/root/|/etc/|/var/|~/)[\w./_-]+"
            ).unwrap(),
            win_path_re: Regex::new(
                r"[A-Z]:\\(?:Users|Documents and Settings|Windows)\\[\w.\\ -]+"
            ).unwrap(),
            env_var_unix_re: Regex::new(
                r"\$(?:\{)?([A-Z_][A-Z0-9_]*)(?:\})?"
            ).unwrap(),
            env_var_win_re: Regex::new(
                r"%([A-Z_][A-Z0-9_]*)%"
            ).unwrap(),
            internal_ip_re: Regex::new(
                r"(?:10\.\d{1,3}\.\d{1,3}\.\d{1,3}|172\.(?:1[6-9]|2\d|3[01])\.\d{1,3}\.\d{1,3}|192\.168\.\d{1,3}\.\d{1,3})"
            ).unwrap(),
            stack_trace_re: Regex::new(
                r"at\s+(?:/[\w./_-]+|[a-zA-Z]:\\[\w.\\_-]+):\d+"
            ).unwrap(),
        }
    }

    /// 检测文本中的上下文泄露
    pub fn detect(&self, text: &str) -> Vec<PiiEntity> {
        let mut entities = Vec::new();

        // 1. Unix 文件路径
        for m in self.file_path_re.find_iter(text) {
            entities.push(PiiEntity {
                entity_type: PiiType::Custom("FILE_PATH".to_string()),
                start: m.start(),
                end: m.end(),
                text: m.as_str().to_string(),
                confidence: 0.85,
            });
        }

        // 2. Windows 文件路径
        for m in self.win_path_re.find_iter(text) {
            entities.push(PiiEntity {
                entity_type: PiiType::Custom("FILE_PATH".to_string()),
                start: m.start(),
                end: m.end(),
                text: m.as_str().to_string(),
                confidence: 0.85,
            });
        }

        // 3. 敏感环境变量 (Unix $VAR / ${VAR})
        for caps in self.env_var_unix_re.captures_iter(text) {
            if let Some(name_match) = caps.get(1) {
                let var_name = name_match.as_str();
                if self.is_sensitive_env(var_name) {
                    let full = caps.get(0).unwrap();
                    entities.push(PiiEntity {
                        entity_type: PiiType::Custom("ENV_VAR".to_string()),
                        start: full.start(),
                        end: full.end(),
                        text: full.as_str().to_string(),
                        confidence: 0.90,
                    });
                }
            }
        }

        // 4. 敏感环境变量 (Windows %VAR%)
        for caps in self.env_var_win_re.captures_iter(text) {
            if let Some(name_match) = caps.get(1) {
                let var_name = name_match.as_str();
                if self.is_sensitive_env(var_name) {
                    let full = caps.get(0).unwrap();
                    entities.push(PiiEntity {
                        entity_type: PiiType::Custom("ENV_VAR".to_string()),
                        start: full.start(),
                        end: full.end(),
                        text: full.as_str().to_string(),
                        confidence: 0.90,
                    });
                }
            }
        }

        // 5. 内网 IP
        for m in self.internal_ip_re.find_iter(text) {
            entities.push(PiiEntity {
                entity_type: PiiType::Custom("INTERNAL_NET".to_string()),
                start: m.start(),
                end: m.end(),
                text: m.as_str().to_string(),
                confidence: 0.85,
            });
        }

        // 6. 堆栈跟踪
        for m in self.stack_trace_re.find_iter(text) {
            entities.push(PiiEntity {
                entity_type: PiiType::Custom("STACK_TRACE".to_string()),
                start: m.start(),
                end: m.end(),
                text: m.as_str().to_string(),
                confidence: 0.75,
            });
        }

        entities
    }

    /// 判断环境变量名是否敏感
    fn is_sensitive_env(&self, name: &str) -> bool {
        let upper = name.to_uppercase();
        // 直接匹配常见敏感变量
        if matches!(
            upper.as_str(),
            "HOME" | "USER" | "USERNAME" | "USERPROFILE" | "PATH" | "HOSTNAME" | "SSH_AUTH_SOCK"
        ) {
            return true;
        }
        // 前缀匹配
        self.sensitive_env_prefixes
            .iter()
            .any(|p| upper.starts_with(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> ContextLeakDetector {
        ContextLeakDetector::default()
    }

    #[test]
    fn detect_unix_file_path() {
        let entities = detector().detect("配置文件在 /home/user/.ssh/id_rsa 中");
        assert!(entities.iter().any(|e| {
            e.entity_type == PiiType::Custom("FILE_PATH".to_string())
                && e.text.contains("/home/user")
        }));
    }

    #[test]
    fn detect_windows_file_path() {
        let entities = detector().detect(r"文件位于 C:\Users\admin\Documents\secret.txt");
        assert!(entities
            .iter()
            .any(|e| { e.entity_type == PiiType::Custom("FILE_PATH".to_string()) }));
    }

    #[test]
    fn detect_sensitive_env_var() {
        let entities = detector().detect("密钥存储在 $AWS_SECRET_ACCESS_KEY 中");
        assert!(entities
            .iter()
            .any(|e| { e.entity_type == PiiType::Custom("ENV_VAR".to_string()) }));
    }

    #[test]
    fn detect_internal_ip() {
        let entities = detector().detect("服务器地址 192.168.1.100 端口 8080");
        assert!(entities.iter().any(|e| {
            e.entity_type == PiiType::Custom("INTERNAL_NET".to_string())
                && e.text == "192.168.1.100"
        }));
    }

    #[test]
    fn detect_10_network() {
        let entities = detector().detect("内网 10.0.0.1 可达");
        assert!(entities
            .iter()
            .any(|e| { e.entity_type == PiiType::Custom("INTERNAL_NET".to_string()) }));
    }

    #[test]
    fn detect_stack_trace() {
        let entities = detector().detect("error at /home/dev/app/src/main.rs:42");
        assert!(entities
            .iter()
            .any(|e| { e.entity_type == PiiType::Custom("STACK_TRACE".to_string()) }));
    }

    #[test]
    fn no_false_positive_on_clean_text() {
        let entities = detector().detect("今天天气不错，适合写代码。");
        assert!(entities.is_empty());
    }

    #[test]
    fn custom_env_prefix() {
        let det = ContextLeakDetector::new(vec!["MYAPP_".to_string()]);
        let entities = det.detect("配置 $MYAPP_DB_PASSWORD 已设置");
        assert!(entities
            .iter()
            .any(|e| { e.entity_type == PiiType::Custom("ENV_VAR".to_string()) }));
    }
}
