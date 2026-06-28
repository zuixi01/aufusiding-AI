//! 数据分级引擎
//!
//! 四级敏感度分类 (L0-L3)，用于安全策略决策。
//! 复用 Anonymizer 的 PII 检测模式。

use serde::{Deserialize, Serialize};

use crate::privacy::sensitive_dictionary::SensitiveDictionary;

/// 数据敏感度级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SensitivityLevel {
    /// L0: 公开信息 (天气、新闻、时间)
    Public = 0,
    /// L1: 普通信息 (日常聊天、知识问答)
    Normal = 1,
    /// L2: 敏感信息 (邮箱、手机、身份证、健康数据)
    Sensitive = 2,
    /// L3: 机密信息 (密码、密钥、银行卡、金融数据)
    Confidential = 3,
}

/// 检测到的敏感模式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPattern {
    pub pattern_type: String,
    pub confidence: f32,
}

/// 数据分级结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataClassification {
    pub level: SensitivityLevel,
    pub detected_patterns: Vec<DetectedPattern>,
    pub confidence: f32,
}

/// 数据分级器
pub struct DataClassifier;

// L3 机密关键词
const CONFIDENTIAL_KEYWORDS: &[&str] = &[
    "password", "passwd", "密码", "secret", "密钥",
    "api_key", "apikey", "api key", "token", "access_token",
    "private_key", "私钥", "银行卡", "信用卡", "借记卡",
    "cvv", "pin码", "pin code",
];

// L3 机密模式前缀
const CONFIDENTIAL_PREFIXES: &[&str] = &["sk-", "pk-", "ghp_", "gho_", "Bearer "];

// L2 敏感关键词
const SENSITIVE_KEYWORDS: &[&str] = &[
    "健康", "病历", "medical", "health", "诊断", "处方",
    "身份证", "护照", "passport", "驾照", "社保",
];

// L0 公开话题关键词
const PUBLIC_KEYWORDS: &[&str] = &[
    "天气", "weather", "新闻", "news", "时间", "time",
    "日期", "date", "你好", "hello", "hi", "嗨",
];

impl DataClassifier {
    /// 对文本进行敏感度分级（向后兼容：仅使用内置关键词）
    pub fn classify(text: &str) -> DataClassification {
        if text.trim().is_empty() {
            return DataClassification {
                level: SensitivityLevel::Public,
                detected_patterns: vec![],
                confidence: 1.0,
            };
        }

        let mut patterns = Vec::new();

        // L3 检测：机密信息
        patterns.extend(Self::detect_confidential(text));

        // L2 检测：敏感信息 (PII + 敏感关键词)
        patterns.extend(Self::detect_sensitive(text));

        // 确定最高级别
        let level = if patterns.iter().any(|p| {
            matches!(
                p.pattern_type.as_str(),
                "PASSWORD" | "API_KEY" | "SECRET" | "BANK_CARD" | "CONFIDENTIAL_PREFIX"
            )
        }) {
            SensitivityLevel::Confidential
        } else if !patterns.is_empty() {
            SensitivityLevel::Sensitive
        } else if Self::is_public_topic(text) {
            SensitivityLevel::Public
        } else {
            SensitivityLevel::Normal
        };

        let confidence = if patterns.is_empty() {
            0.8
        } else {
            (0.7 + 0.1 * patterns.len() as f32).min(1.0)
        };

        DataClassification {
            level,
            detected_patterns: patterns,
            confidence,
        }
    }

    /// 词典增强分级：内置关键词 + 外部词典
    ///
    /// 词典命中的实体按其 sensitivity 级别提升分级结果。
    pub fn classify_with_dict(text: &str, dict: &SensitiveDictionary) -> DataClassification {
        // 先执行内置分级
        let mut base = Self::classify(text);

        // 词典检测
        let dict_entities = dict.detect(text);
        if dict_entities.is_empty() {
            return base;
        }

        // 从词典实体中提取最高 sensitivity（存储在 confidence 的整数部分约定中）
        // 词典实体的 PiiType::Custom(category) 携带分类信息
        // 通过词典规则的 sensitivity 映射到 L0-L3
        let mut max_dict_level = 0u8;
        for entity in &dict_entities {
            base.detected_patterns.push(DetectedPattern {
                pattern_type: format!("DICT_{}", entity.entity_type),
                confidence: entity.confidence,
            });
            // 词典规则的 sensitivity 编码在 confidence 的十分位
            // 约定：confidence >= 0.9 → L3, >= 0.7 → L2, >= 0.5 → L1
            let level = if entity.confidence >= 0.9 {
                3u8
            } else if entity.confidence >= 0.7 {
                2u8
            } else if entity.confidence >= 0.5 {
                1u8
            } else {
                0u8
            };
            max_dict_level = max_dict_level.max(level);
        }

        // 提升分级（只升不降）
        let dict_sensitivity = match max_dict_level {
            3 => SensitivityLevel::Confidential,
            2 => SensitivityLevel::Sensitive,
            1 => SensitivityLevel::Normal,
            _ => SensitivityLevel::Public,
        };
        if dict_sensitivity > base.level {
            base.level = dict_sensitivity;
        }

        // 重新计算置信度
        base.confidence = if base.detected_patterns.is_empty() {
            0.8
        } else {
            (0.7 + 0.1 * base.detected_patterns.len() as f32).min(1.0)
        };

        base
    }

    /// 检测 L3 机密模式
    fn detect_confidential(text: &str) -> Vec<DetectedPattern> {
        let lower = text.to_lowercase();
        let mut patterns = Vec::new();

        for kw in CONFIDENTIAL_KEYWORDS {
            if lower.contains(kw) {
                let ptype = if *kw == "password" || *kw == "passwd" || *kw == "密码" {
                    "PASSWORD"
                } else if kw.contains("api") || kw.contains("token") || kw.contains("key") || kw.contains("密钥") || kw.contains("私钥") {
                    "API_KEY"
                } else if kw.contains("银行") || kw.contains("信用") || kw.contains("借记") || kw.contains("cvv") || kw.contains("pin") {
                    "BANK_CARD"
                } else {
                    "SECRET"
                };
                patterns.push(DetectedPattern {
                    pattern_type: ptype.to_string(),
                    confidence: 0.9,
                });
                break; // 一个 L3 命中即可
            }
        }

        // 检测密钥前缀
        for prefix in CONFIDENTIAL_PREFIXES {
            if text.contains(prefix) {
                patterns.push(DetectedPattern {
                    pattern_type: "CONFIDENTIAL_PREFIX".to_string(),
                    confidence: 0.95,
                });
                break;
            }
        }

        // 检测银行卡号模式 (16-19位连续数字)
        if Self::has_bank_card_pattern(text) && patterns.iter().all(|p| p.pattern_type != "BANK_CARD") {
            patterns.push(DetectedPattern {
                pattern_type: "BANK_CARD".to_string(),
                confidence: 0.85,
            });
        }

        patterns
    }

    /// 检测 L2 敏感模式 (PII + 敏感关键词)
    fn detect_sensitive(text: &str) -> Vec<DetectedPattern> {
        let lower = text.to_lowercase();
        let mut patterns = Vec::new();

        // 邮箱检测
        if text.contains('@') && Self::has_email_pattern(text) {
            patterns.push(DetectedPattern {
                pattern_type: "EMAIL".to_string(),
                confidence: 0.9,
            });
        }

        // 中国手机号检测 (1xx开头11位)
        if Self::has_chinese_phone(text) {
            patterns.push(DetectedPattern {
                pattern_type: "PHONE".to_string(),
                confidence: 0.9,
            });
        }

        // 身份证号检测 (18位)
        if Self::has_id_card(text) {
            patterns.push(DetectedPattern {
                pattern_type: "ID_CARD".to_string(),
                confidence: 0.9,
            });
        }

        // 敏感关键词
        for kw in SENSITIVE_KEYWORDS {
            if lower.contains(kw) {
                patterns.push(DetectedPattern {
                    pattern_type: "SENSITIVE_KEYWORD".to_string(),
                    confidence: 0.8,
                });
                break;
            }
        }

        patterns
    }

    /// 是否为公开话题
    fn is_public_topic(text: &str) -> bool {
        let lower = text.to_lowercase();
        PUBLIC_KEYWORDS.iter().any(|kw| lower.contains(kw))
            && text.len() < 50
    }

    /// 邮箱模式检测
    fn has_email_pattern(text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            if c == '@' && i > 0 {
                let has_local = chars[..i].iter().any(|c| c.is_alphanumeric());
                let has_domain = chars[i + 1..].contains(&'.');
                if has_local && has_domain {
                    return true;
                }
            }
        }
        false
    }

    /// 中国手机号检测
    fn has_chinese_phone(text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        for i in 0..chars.len() {
            if chars[i] == '1' && i + 10 < chars.len() {
                let next = chars[i + 1];
                if ('3'..='9').contains(&next) {
                    let digits: String = chars[i..i + 11].iter().collect();
                    if digits.chars().all(|c| c.is_ascii_digit()) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 身份证号检测 (18位)
    fn has_id_card(text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i + 17 < chars.len() {
            if chars[i].is_ascii_digit() {
                let candidate: String = chars[i..i + 18].iter().collect();
                let first_17_digits = candidate[..17].chars().all(|c| c.is_ascii_digit());
                let last_valid = candidate.chars().last().map(|c| c.is_ascii_digit() || c == 'X' || c == 'x').unwrap_or(false);
                if first_17_digits && last_valid {
                    return true;
                }
            }
            i += 1;
        }
        false
    }

    /// 银行卡号检测 (16-19位连续数字，排除18位身份证号)
    fn has_bank_card_pattern(text: &str) -> bool {
        let mut digit_count = 0;
        let mut start_idx = 0;
        let chars: Vec<char> = text.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            if c.is_ascii_digit() {
                if digit_count == 0 {
                    start_idx = i;
                }
                digit_count += 1;
            } else if c == ' ' || c == '-' {
                // 允许空格和连字符分隔
            } else {
                if digit_count >= 16 && digit_count != 18 {
                    return true;
                }
                // 18位可能是身份证，跳过
                digit_count = 0;
                let _ = start_idx;
            }
        }
        digit_count >= 16 && digit_count != 18
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_empty_text_as_public() {
        let result = DataClassifier::classify("");
        assert_eq!(result.level, SensitivityLevel::Public);
        assert!(result.detected_patterns.is_empty());
    }

    #[test]
    fn classify_public_topic() {
        let result = DataClassifier::classify("今天天气怎么样");
        assert_eq!(result.level, SensitivityLevel::Public);
    }

    #[test]
    fn classify_normal_chat() {
        let result = DataClassifier::classify("帮我写一个 Rust 排序算法");
        assert_eq!(result.level, SensitivityLevel::Normal);
    }

    #[test]
    fn classify_email_as_sensitive() {
        let result = DataClassifier::classify("我的邮箱是 test@example.com");
        assert_eq!(result.level, SensitivityLevel::Sensitive);
        assert!(result.detected_patterns.iter().any(|p| p.pattern_type == "EMAIL"));
    }

    #[test]
    fn classify_phone_as_sensitive() {
        let result = DataClassifier::classify("联系电话 13912345678");
        assert_eq!(result.level, SensitivityLevel::Sensitive);
        assert!(result.detected_patterns.iter().any(|p| p.pattern_type == "PHONE"));
    }

    #[test]
    fn classify_id_card_as_sensitive() {
        let result = DataClassifier::classify("身份证号 110101199001011234");
        assert_eq!(result.level, SensitivityLevel::Sensitive);
        assert!(result.detected_patterns.iter().any(|p| p.pattern_type == "ID_CARD"));
    }

    #[test]
    fn classify_password_as_confidential() {
        let result = DataClassifier::classify("我的密码是 abc123456");
        assert_eq!(result.level, SensitivityLevel::Confidential);
        assert!(result.detected_patterns.iter().any(|p| p.pattern_type == "PASSWORD"));
    }

    #[test]
    fn classify_api_key_as_confidential() {
        let result = DataClassifier::classify("api_key=sk-1234567890abcdef");
        assert_eq!(result.level, SensitivityLevel::Confidential);
    }

    #[test]
    fn classify_bank_card_as_confidential() {
        let result = DataClassifier::classify("银行卡号 6222021234567890123");
        assert_eq!(result.level, SensitivityLevel::Confidential);
    }

    #[test]
    fn classify_mixed_returns_highest() {
        // 同时包含邮箱(L2)和密码(L3)，应返回 L3
        let result = DataClassifier::classify("邮箱 test@mail.com 密码 secret123");
        assert_eq!(result.level, SensitivityLevel::Confidential);
        assert!(result.detected_patterns.len() >= 2);
    }

    #[test]
    fn classify_with_dict_elevates_level() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // 创建临时词典 TOML
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, r#"
[metadata]
name = "test"
category = "MILITARY"
sensitivity = 3

[[keywords]]
terms = ["导弹", "核武器"]
"#).unwrap();

        let dict = SensitiveDictionary::load_from_paths(&[f.path().to_path_buf()]).unwrap();
        let result = DataClassifier::classify_with_dict("这里有导弹发射基地", &dict);
        // 词典命中 L3 关键词，应提升到 Confidential
        assert_eq!(result.level, SensitivityLevel::Confidential);
        assert!(result.detected_patterns.iter().any(|p| p.pattern_type.contains("DICT_")));
    }

    #[test]
    fn classify_with_empty_dict_same_as_classify() {
        let dict = SensitiveDictionary::empty();
        let base = DataClassifier::classify("今天天气怎么样");
        let with_dict = DataClassifier::classify_with_dict("今天天气怎么样", &dict);
        assert_eq!(base.level, with_dict.level);
    }
}
