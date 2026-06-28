//! 任务难度评估引擎
//!
//! 根据分类结果和输入文本，评估任务难度等级，
//! 并映射到所需的模型层级。

use super::catalog::{CapabilityDomain, ModelCatalog, ModelTier};
use super::classifier::{ClassificationResult, IntentType};
use serde::{Deserialize, Serialize};

/// 任务难度等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TaskDifficulty {
    /// 闲聊、简单问答、翻译短句
    Low,
    /// 中等编程、知识问答、创意写作
    Medium,
    /// 复杂推理、学术研究、高级编程、数学证明
    High,
}

/// 难度评估结果
#[derive(Debug, Clone)]
pub struct DifficultyAssessment {
    /// 难度等级
    pub difficulty: TaskDifficulty,
    /// 所需最低模型层级
    pub required_tier: ModelTier,
    /// 主要能力维度
    pub domain: CapabilityDomain,
    /// 可读解释
    pub reasoning: String,
}

/// 高难度关键词
const HIGH_KEYWORDS: &[&str] = &[
    "证明",
    "推导",
    "论文",
    "学术",
    "研究",
    "定理",
    "公理",
    "形式化",
    "数学归纳",
];

/// 中等难度关键词
const MEDIUM_KEYWORDS: &[&str] = &[
    "算法",
    "架构",
    "重构",
    "优化",
    "设计模式",
    "并发",
    "分布式",
    "微服务",
];

/// 低难度关键词
const LOW_KEYWORDS: &[&str] = &["你好", "天气", "翻译", "谢谢", "帮我查"];
/// 评估任务难度
pub fn assess_difficulty(
    classification: &ClassificationResult,
    input_text: &str,
) -> DifficultyAssessment {
    let domain = ModelCatalog::intent_to_domain(&classification.primary);
    let complexity = classification.complexity;

    // 基于 complexity + intent 联合判定
    let mut difficulty = base_difficulty(complexity, &classification.primary);

    // 关键词修正
    difficulty = keyword_adjust(difficulty, input_text, complexity);

    let required_tier = difficulty_to_tier(difficulty);

    let reasoning = format!(
        "complexity={}, intent={:?}, domain={:?} → {:?} → {:?}",
        complexity, classification.primary, domain, difficulty, required_tier
    );

    DifficultyAssessment {
        difficulty,
        required_tier,
        domain,
        reasoning,
    }
}

/// 基于 complexity 和 intent 的基础难度判定
fn base_difficulty(complexity: u8, intent: &IntentType) -> TaskDifficulty {
    let is_hard_intent = matches!(intent, IntentType::Reasoning | IntentType::CodeGen);

    if complexity >= 8 {
        return TaskDifficulty::High;
    }
    if complexity >= 5 && is_hard_intent {
        return TaskDifficulty::Medium;
    }
    if complexity >= 5 {
        return TaskDifficulty::Medium;
    }
    if matches!(intent, IntentType::Casual) {
        return TaskDifficulty::Low;
    }
    if matches!(intent, IntentType::Knowledge) && complexity < 5 {
        return TaskDifficulty::Low;
    }
    TaskDifficulty::Medium
}

/// 关键词检测修正难度
fn keyword_adjust(current: TaskDifficulty, text: &str, complexity: u8) -> TaskDifficulty {
    // 高难度关键词：至少 Medium，complexity>=6 则 High
    if HIGH_KEYWORDS.iter().any(|kw| text.contains(kw)) {
        if complexity >= 6 {
            return TaskDifficulty::High;
        }
        if current < TaskDifficulty::Medium {
            return TaskDifficulty::Medium;
        }
    }

    // 中等难度关键词：至少 Medium
    if MEDIUM_KEYWORDS.iter().any(|kw| text.contains(kw)) && current < TaskDifficulty::Medium {
        return TaskDifficulty::Medium;
    }

    // 低难度关键词：仅在 complexity 低时降级
    if LOW_KEYWORDS.iter().any(|kw| text.contains(kw)) && complexity < 4 {
        return TaskDifficulty::Low;
    }

    current
}

/// 难度→层级映射
fn difficulty_to_tier(difficulty: TaskDifficulty) -> ModelTier {
    match difficulty {
        TaskDifficulty::Low => ModelTier::Local,
        TaskDifficulty::Medium => ModelTier::Standard,
        TaskDifficulty::High => ModelTier::Premium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cls(intent: IntentType, complexity: u8) -> ClassificationResult {
        ClassificationResult {
            primary: intent,
            confidence: 0.85,
            complexity,
        }
    }

    #[test]
    fn casual_greeting_is_low() {
        let cls = make_cls(IntentType::Casual, 2);
        let a = assess_difficulty(&cls, "你好");
        assert_eq!(a.difficulty, TaskDifficulty::Low);
        assert_eq!(a.required_tier, ModelTier::Local);
    }

    #[test]
    fn complex_reasoning_is_high() {
        let cls = make_cls(IntentType::Reasoning, 9);
        let a = assess_difficulty(&cls, "证明P≠NP");
        assert_eq!(a.difficulty, TaskDifficulty::High);
        assert_eq!(a.required_tier, ModelTier::Premium);
    }

    #[test]
    fn medium_coding_task() {
        let cls = make_cls(IntentType::CodeGen, 5);
        let a = assess_difficulty(&cls, "写一个排序函数");
        assert_eq!(a.difficulty, TaskDifficulty::Medium);
        assert_eq!(a.required_tier, ModelTier::Standard);
    }

    #[test]
    fn academic_keywords_boost_difficulty() {
        let cls = make_cls(IntentType::Knowledge, 6);
        let a = assess_difficulty(&cls, "请推导贝叶斯定理的学术证明");
        assert_eq!(a.difficulty, TaskDifficulty::High);
    }

    #[test]
    fn difficulty_maps_to_correct_tier() {
        assert_eq!(difficulty_to_tier(TaskDifficulty::Low), ModelTier::Local);
        assert_eq!(
            difficulty_to_tier(TaskDifficulty::Medium),
            ModelTier::Standard
        );
        assert_eq!(difficulty_to_tier(TaskDifficulty::High), ModelTier::Premium);
    }

    #[test]
    fn high_complexity_always_high() {
        // complexity=9 无论意图都是 High
        for intent in [
            IntentType::Casual,
            IntentType::Knowledge,
            IntentType::Creative,
        ] {
            let cls = make_cls(intent, 9);
            let a = assess_difficulty(&cls, "普通文本");
            assert_eq!(
                a.difficulty,
                TaskDifficulty::High,
                "complexity=9 + {:?} 应为 High",
                cls.primary
            );
        }
    }
}
