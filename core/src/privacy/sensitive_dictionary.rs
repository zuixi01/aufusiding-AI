//! 领域敏感词典加载引擎
//!
//! 支持 TOML 格式外部词典，使用 Aho-Corasick 多模匹配关键词，正则匹配模式。
//! 返回标准 `PiiEntity` 以统一汇入脱敏管线。

use std::path::Path;

use aho_corasick::AhoCorasick;
use regex::Regex;
use serde::Deserialize;

use super::pii_detector::{PiiEntity, PiiType};

/// TOML 词典文件顶层结构
#[derive(Debug, Deserialize)]
struct DictionaryFile {
    metadata: DictionaryMetadata,
    #[serde(default)]
    keywords: Vec<KeywordGroup>,
    #[serde(default)]
    patterns: Vec<PatternEntry>,
}

/// 词典元数据
#[derive(Debug, Deserialize)]
struct DictionaryMetadata {
    #[allow(dead_code)]
    name: String,
    category: String,
    sensitivity: u8,
}

/// 关键词组
#[derive(Debug, Deserialize)]
struct KeywordGroup {
    terms: Vec<String>,
}

/// 正则模式条目
#[derive(Debug, Deserialize)]
struct PatternEntry {
    regex: String,
    #[serde(default)]
    #[allow(dead_code)]
    description: String,
}

/// 关键词元数据（与 Aho-Corasick 索引对应）
struct KeywordMeta {
    #[allow(dead_code)]
    category: String,
    #[allow(dead_code)]
    sensitivity: u8,
    pii_type: PiiType,
    confidence: f32,
}

/// 编译后的正则规则
struct CompiledDictionaryRule {
    regex: Regex,
    #[allow(dead_code)]
    category: String,
    #[allow(dead_code)]
    sensitivity: u8,
    pii_type: PiiType,
    confidence: f32,
}

/// 领域敏感词典集合
pub struct SensitiveDictionary {
    /// Aho-Corasick 自动机（关键词多模匹配）
    aho_corasick: Option<AhoCorasick>,
    /// 关键词元数据（索引与 aho_corasick 模式对应）
    keyword_meta: Vec<KeywordMeta>,
    /// 编译后的正则规则
    regex_rules: Vec<CompiledDictionaryRule>,
}

impl SensitiveDictionary {
    /// 从多个 TOML 文件加载词典
    pub fn load_from_paths(paths: &[impl AsRef<Path>]) -> anyhow::Result<Self> {
        let mut all_keywords: Vec<String> = Vec::new();
        let mut all_meta: Vec<KeywordMeta> = Vec::new();
        let mut all_regex: Vec<CompiledDictionaryRule> = Vec::new();

        for path in paths {
            let content = std::fs::read_to_string(path.as_ref())?;
            let file: DictionaryFile = toml::from_str(&content)?;
            let category = &file.metadata.category;
            let sensitivity = file.metadata.sensitivity;
            let confidence = Self::sensitivity_to_confidence(sensitivity);
            let pii_type = PiiType::Custom(category.to_uppercase());

            // 收集关键词
            for group in &file.keywords {
                for term in &group.terms {
                    all_keywords.push(term.clone());
                    all_meta.push(KeywordMeta {
                        category: category.clone(),
                        sensitivity,
                        pii_type: pii_type.clone(),
                        confidence,
                    });
                }
            }

            // 编译正则
            for pat in &file.patterns {
                match Regex::new(&pat.regex) {
                    Ok(regex) => {
                        all_regex.push(CompiledDictionaryRule {
                            regex,
                            category: category.clone(),
                            sensitivity,
                            pii_type: pii_type.clone(),
                            confidence,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            pattern = %pat.regex,
                            error = %e,
                            "Failed to compile dictionary regex, skipping"
                        );
                    }
                }
            }
        }

        let aho_corasick = if all_keywords.is_empty() {
            None
        } else {
            Some(AhoCorasick::new(&all_keywords)?)
        };

        Ok(Self {
            aho_corasick,
            keyword_meta: all_meta,
            regex_rules: all_regex,
        })
    }

    /// 从目录加载所有 .toml 词典
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Self> {
        let mut paths: Vec<std::path::PathBuf> = Vec::new();
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "toml").unwrap_or(false) {
                    paths.push(path);
                }
            }
        }
        Self::load_from_paths(&paths)
    }

    /// 创建空词典
    pub fn empty() -> Self {
        Self {
            aho_corasick: None,
            keyword_meta: Vec::new(),
            regex_rules: Vec::new(),
        }
    }

    /// 检测文本中的敏感词典命中
    pub fn detect(&self, text: &str) -> Vec<PiiEntity> {
        let mut entities = Vec::new();

        // 1. Aho-Corasick 关键词匹配
        if let Some(ref ac) = self.aho_corasick {
            for mat in ac.find_iter(text) {
                let meta = &self.keyword_meta[mat.pattern().as_usize()];
                entities.push(PiiEntity {
                    entity_type: meta.pii_type.clone(),
                    start: mat.start(),
                    end: mat.end(),
                    text: text[mat.start()..mat.end()].to_string(),
                    confidence: meta.confidence,
                });
            }
        }

        // 2. 正则规则匹配
        for rule in &self.regex_rules {
            for mat in rule.regex.find_iter(text) {
                entities.push(PiiEntity {
                    entity_type: rule.pii_type.clone(),
                    start: mat.start(),
                    end: mat.end(),
                    text: mat.as_str().to_string(),
                    confidence: rule.confidence,
                });
            }
        }

        entities
    }

    /// 词典是否为空
    pub fn is_empty(&self) -> bool {
        self.keyword_meta.is_empty() && self.regex_rules.is_empty()
    }

    fn sensitivity_to_confidence(sensitivity: u8) -> f32 {
        match sensitivity {
            3 => 0.95,
            2 => 0.85,
            1 => 0.75,
            _ => 0.70,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_test_toml(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn load_and_detect_keywords() {
        let dir = tempfile::tempdir().unwrap();
        let toml_content = r#"
[metadata]
name = "测试军事"
category = "military"
sensitivity = 3

[[keywords]]
terms = ["导弹", "核武器", "军事基地"]

[[patterns]]
regex = "(?:歼|运|轰)-\\d{1,2}[A-Z]?"
description = "军用飞机型号"
"#;
        let path = write_test_toml(dir.path(), "military.toml", toml_content);
        let dict = SensitiveDictionary::load_from_paths(&[path]).unwrap();

        let entities = dict.detect("这里有一个导弹发射基地和歼-20战斗机");
        assert!(entities.iter().any(|e| e.text == "导弹"));
        assert!(entities.iter().any(|e| e.text == "歼-20"));
        assert!(entities.iter().all(|e| e.confidence >= 0.90));
    }

    #[test]
    fn empty_dictionary_detects_nothing() {
        let dict = SensitiveDictionary::empty();
        assert!(dict.detect("导弹 核武器").is_empty());
        assert!(dict.is_empty());
    }

    #[test]
    fn load_from_dir_works() {
        let dir = tempfile::tempdir().unwrap();
        let content = r#"
[metadata]
name = "金融"
category = "financial"
sensitivity = 2

[[keywords]]
terms = ["交易流水", "账户余额"]
"#;
        write_test_toml(dir.path(), "financial.toml", content);
        let dict = SensitiveDictionary::load_from_dir(dir.path()).unwrap();
        let entities = dict.detect("请查看交易流水记录");
        assert!(!entities.is_empty());
    }
}
