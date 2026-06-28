//! 危机感知
//!
//! 检测用户输入中的危机信号（自杀倾向、暴力等），及时介入。

use serde::{Deserialize, Serialize};

/// 危机类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CrisisType {
    /// 自杀倾向
    Suicide,
    /// 自残
    SelfHarm,
    /// 暴力倾向
    Violence,
    /// 严重心理危机
    MentalHealth,
    /// 无危机
    None,
}

/// 危机检测结果
#[derive(Debug, Clone)]
pub struct CrisisDetection {
    /// 危机类型
    pub crisis_type: CrisisType,
    /// 置信度 (0.0 - 1.0)
    pub confidence: f32,
    /// 触发关键词
    pub keywords: Vec<String>,
}

/// 危机事件日志
#[derive(Debug, Clone)]
pub struct CrisisLogEntry {
    pub crisis_type: CrisisType,
    pub confidence: f32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 危机检测器
pub struct CrisisDetector {
    /// 检测阈值
    threshold: f32,
    /// 事件日志
    log: Vec<CrisisLogEntry>,
}

struct KeywordSet {
    crisis_type: CrisisType,
    keywords: Vec<&'static str>,
}

fn keyword_sets() -> Vec<KeywordSet> {
    vec![
        KeywordSet {
            crisis_type: CrisisType::Suicide,
            keywords: vec![
                "不想活了",
                "想死",
                "自杀",
                "结束生命",
                "活不下去",
                "suicide",
                "kill myself",
                "end my life",
                "want to die",
            ],
        },
        KeywordSet {
            crisis_type: CrisisType::SelfHarm,
            keywords: vec![
                "想伤害自己",
                "自残",
                "割腕",
                "自我伤害",
                "self-harm",
                "hurt myself",
                "cut myself",
            ],
        },
        KeywordSet {
            crisis_type: CrisisType::Violence,
            keywords: vec![
                "想杀人",
                "杀了他",
                "暴力",
                "伤害别人",
                "kill someone",
                "hurt others",
                "violence",
            ],
        },
        KeywordSet {
            crisis_type: CrisisType::MentalHealth,
            keywords: vec![
                "严重焦虑",
                "严重抑郁",
                "精神崩溃",
                "心理崩溃",
                "极度绝望",
                "severe anxiety",
                "severe depression",
                "mental breakdown",
            ],
        },
    ]
}

impl CrisisDetector {
    /// 创建危机检测器
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,
            log: Vec::new(),
        }
    }

    /// 检测文本中的危机信号
    pub async fn detect(&self, text: &str) -> anyhow::Result<CrisisDetection> {
        let lower = text.to_lowercase();
        let mut best_type = CrisisType::None;
        let mut best_confidence = 0.0f32;
        let mut matched_keywords = Vec::new();

        for set in keyword_sets() {
            let total = set.keywords.len() as f32;
            let mut hits = Vec::new();
            for kw in &set.keywords {
                if lower.contains(kw) {
                    hits.push(kw.to_string());
                }
            }
            let confidence = hits.len() as f32 / total;
            if confidence > best_confidence {
                best_confidence = confidence;
                best_type = set.crisis_type.clone();
                matched_keywords = hits;
            }
        }

        if best_confidence < self.threshold {
            return Ok(CrisisDetection {
                crisis_type: CrisisType::None,
                confidence: best_confidence,
                keywords: Vec::new(),
            });
        }

        Ok(CrisisDetection {
            crisis_type: best_type,
            confidence: best_confidence,
            keywords: matched_keywords,
        })
    }

    /// 生成危机响应
    pub fn generate_response(&self, detection: &CrisisDetection) -> String {
        match detection.crisis_type {
            CrisisType::Suicide => "检测到可能的自杀倾向。请立即拨打求助热线：\n\
                 - 全国24小时心理危机干预热线：400-161-9995\n\
                 - 北京心理危机研究与干预中心：010-82951332\n\
                 - National Suicide Prevention Lifeline: 988\n\
                 你并不孤单，请寻求专业帮助。"
                .to_string(),
            CrisisType::SelfHarm => "检测到可能的自我伤害倾向。请联系专业人士：\n\
                 - 全国24小时心理危机干预热线：400-161-9995\n\
                 - Crisis Text Line: Text HOME to 741741\n\
                 请不要伤害自己，寻求帮助是勇敢的选择。"
                .to_string(),
            CrisisType::Violence => {
                "检测到可能的暴力倾向。如果你或他人处于危险中，请立即拨打 110。\n\
                 如需心理支持，请联系：400-161-9995"
                    .to_string()
            }
            CrisisType::MentalHealth => "检测到严重心理健康问题。建议寻求专业帮助：\n\
                 - 全国24小时心理危机干预热线：400-161-9995\n\
                 - 希望24热线：400-161-9995\n\
                 你的感受很重要，专业人士可以帮助你。"
                .to_string(),
            CrisisType::None => String::new(),
        }
    }

    /// 记录危机事件
    pub async fn log_crisis(&mut self, detection: &CrisisDetection) -> anyhow::Result<()> {
        self.log.push(CrisisLogEntry {
            crisis_type: detection.crisis_type.clone(),
            confidence: detection.confidence,
            timestamp: chrono::Utc::now(),
        });
        Ok(())
    }

    /// 获取日志条目（用于测试）
    pub fn log_entries(&self) -> &[CrisisLogEntry] {
        &self.log
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_creates_detector_with_threshold() {
        let detector = CrisisDetector::new(0.1);
        assert_eq!(detector.threshold, 0.1);
        assert!(detector.log.is_empty());
    }

    #[tokio::test]
    async fn detect_suicide_keywords() {
        let detector = CrisisDetector::new(0.05);
        let result = detector.detect("我不想活了").await.unwrap();
        assert_eq!(result.crisis_type, CrisisType::Suicide);
        assert!(result.confidence > 0.0);
    }

    #[tokio::test]
    async fn detect_self_harm_keywords() {
        let detector = CrisisDetector::new(0.05);
        let result = detector.detect("想伤害自己").await.unwrap();
        assert_eq!(result.crisis_type, CrisisType::SelfHarm);
    }

    #[tokio::test]
    async fn detect_violence_keywords() {
        let detector = CrisisDetector::new(0.05);
        let result = detector.detect("想杀人").await.unwrap();
        assert_eq!(result.crisis_type, CrisisType::Violence);
    }

    #[tokio::test]
    async fn detect_mental_health_keywords() {
        let detector = CrisisDetector::new(0.05);
        let result = detector.detect("我有严重焦虑").await.unwrap();
        assert_eq!(result.crisis_type, CrisisType::MentalHealth);
    }

    #[tokio::test]
    async fn detect_no_crisis() {
        let detector = CrisisDetector::new(0.05);
        let result = detector.detect("今天天气真好").await.unwrap();
        assert_eq!(result.crisis_type, CrisisType::None);
    }

    #[tokio::test]
    async fn confidence_above_threshold_triggers() {
        let detector = CrisisDetector::new(0.05);
        let result = detector.detect("我想死，不想活了").await.unwrap();
        assert!(result.confidence >= 0.05);
        assert_ne!(result.crisis_type, CrisisType::None);
    }

    #[tokio::test]
    async fn confidence_below_threshold_returns_none() {
        // threshold very high, single keyword won't reach it
        let detector = CrisisDetector::new(0.99);
        let result = detector.detect("想死").await.unwrap();
        assert_eq!(result.crisis_type, CrisisType::None);
    }

    #[tokio::test]
    async fn generate_response_suicide() {
        let detector = CrisisDetector::new(0.05);
        let detection = CrisisDetection {
            crisis_type: CrisisType::Suicide,
            confidence: 0.5,
            keywords: vec!["自杀".to_string()],
        };
        let response = detector.generate_response(&detection);
        assert!(response.contains("求助热线"));
        assert!(response.contains("988"));
    }

    #[tokio::test]
    async fn generate_response_returns_resources() {
        let detector = CrisisDetector::new(0.05);
        for crisis_type in [
            CrisisType::Suicide,
            CrisisType::SelfHarm,
            CrisisType::Violence,
            CrisisType::MentalHealth,
        ] {
            let detection = CrisisDetection {
                crisis_type,
                confidence: 0.5,
                keywords: vec![],
            };
            let response = detector.generate_response(&detection);
            assert!(!response.is_empty());
        }
    }

    #[tokio::test]
    async fn log_crisis_records_event() {
        let mut detector = CrisisDetector::new(0.05);
        let detection = CrisisDetection {
            crisis_type: CrisisType::Suicide,
            confidence: 0.5,
            keywords: vec!["自杀".to_string()],
        };
        detector.log_crisis(&detection).await.unwrap();
        assert_eq!(detector.log_entries().len(), 1);
        assert_eq!(detector.log_entries()[0].crisis_type, CrisisType::Suicide);
    }
}
