//! LocalSteward — 本地模型自治路由决策引擎
//!
//! 让本地模型成为"大管家"：自主判断能否处理、不行就升级云端、
//! 从云端回复中学习、逐步扩大自己的能力边界。
//! 核心理念：不聪明，但心细、记忆力好、知道自己几斤几两。

use super::capability::{CapabilityMatrix, ModelId};
use super::catalog::{CapabilityDomain, ModelCatalog, ModelTier};
use super::classifier::{ClassificationResult, IntentType};
use super::difficulty::assess_difficulty;
use super::strategy::{RouteDecision, RouteTarget};
use chrono::{DateTime, Utc};

/// 大管家配置
#[derive(Debug, Clone)]
pub struct StewardConfig {
    /// 基础升级阈值
    pub base_threshold: f64,
    /// complexity >= 7 时的阈值
    pub high_complexity_threshold: f64,
    /// 能力矩阵权重
    pub capability_weight: f64,
    /// 历史记忆权重
    pub memory_weight: f64,
    /// 分类器置信度权重
    pub confidence_weight: f64,
    /// 每级复杂度惩罚
    pub complexity_penalty: f64,
    /// 学习率
    pub learning_rate: f64,
    /// 最大历史记录数
    pub max_history: usize,
    /// 经济效益权重
    pub economic_weight: f64,
    /// 是否启用目录路由（v2 算法）
    pub use_catalog_routing: bool,
}

impl Default for StewardConfig {
    fn default() -> Self {
        Self {
            base_threshold: 0.45,
            high_complexity_threshold: 0.75,
            capability_weight: 0.50,
            memory_weight: 0.25,
            confidence_weight: 0.25,
            complexity_penalty: 0.08,
            learning_rate: 0.15,
            max_history: 500,
            economic_weight: 0.20,
            use_catalog_routing: true,
        }
    }
}

/// 大管家决策结果
#[derive(Debug, Clone)]
pub struct StewardDecision {
    /// 路由决策
    pub route: RouteDecision,
    /// 是否升级到云端
    pub escalated: bool,
    /// 决策推理过程
    pub reasoning: StewardReasoning,
    /// 升级云端时的预处理查询
    pub preprocessed_query: Option<String>,
}

/// 决策推理详情
#[derive(Debug, Clone)]
pub struct StewardReasoning {
    /// 能力矩阵得分
    pub local_capability: f64,
    /// 历史同类查询本地成功率
    pub memory_match: f64,
    /// 分类器置信度
    pub classifier_confidence: f64,
    /// 复杂度惩罚
    pub complexity_penalty: f64,
    /// 最终本地置信度
    pub final_score: f64,
    /// 使用的阈值
    pub threshold: f64,
    /// 可读解释
    pub explanation: String,
}

/// 交互记录
#[derive(Debug, Clone)]
pub struct InteractionRecord {
    /// 意图类型
    pub intent: IntentType,
    /// 查询摘要（前80字符）
    pub query_digest: String,
    /// 使用的模型
    pub model: ModelId,
    /// 是否本地处理
    pub handled_locally: bool,
    /// 启发式质量评分
    pub quality_score: f64,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

/// 本地模型自治路由决策引擎
pub struct LocalSteward {
    config: StewardConfig,
    capability_matrix: CapabilityMatrix,
    catalog: ModelCatalog,
    history: Vec<InteractionRecord>,
}

impl LocalSteward {
    /// 创建新的大管家
    pub fn new(config: StewardConfig) -> Self {
        Self {
            config,
            capability_matrix: CapabilityMatrix::new(),
            catalog: ModelCatalog::new(),
            history: Vec::new(),
        }
    }

    /// 创建使用默认配置的大管家
    pub fn with_defaults() -> Self {
        Self::new(StewardConfig::default())
    }

    /// 核心决策：判断本地处理还是升级云端
    pub fn decide(
        &self,
        classification: &ClassificationResult,
        input_tokens: u64,
        memory_context: Option<&str>,
    ) -> StewardDecision {
        if self.config.use_catalog_routing {
            self.decide_v2(classification, input_tokens, memory_context)
        } else {
            self.decide_v1(classification, input_tokens, memory_context)
        }
    }

    /// V1 算法（原有逻辑）
    fn decide_v1(
        &self,
        classification: &ClassificationResult,
        input_tokens: u64,
        memory_context: Option<&str>,
    ) -> StewardDecision {
        let intent = &classification.primary;

        // 1. 能力矩阵预测
        let best_local = self.select_local_model(classification.complexity, intent);
        let local_cap = self.capability_matrix.predict_quality(best_local, intent);

        // 2. 历史记忆匹配
        let memory_match = self.compute_memory_match(intent);

        // 3. 分类器置信度
        let conf = classification.confidence as f64;

        // 4. 复杂度惩罚（complexity <= 3 无惩罚）
        let penalty = if classification.complexity > 3 {
            self.config.complexity_penalty * (classification.complexity.saturating_sub(3) as f64)
        } else {
            0.0
        };

        // 5. 阈值选择
        let threshold = if classification.complexity >= 7 {
            self.config.high_complexity_threshold
        } else {
            self.config.base_threshold
        };

        // 6. 最终得分
        let final_score = self.config.capability_weight * local_cap
            + self.config.memory_weight * memory_match
            + self.config.confidence_weight * conf
            - penalty;

        let escalated = final_score < threshold;
        let (model, target, reason_prefix) = if escalated {
            let cloud_model = self.select_best_cloud(intent);
            let target = model_to_target(cloud_model);
            (cloud_model, target, "升级云端")
        } else {
            let target = model_to_target(best_local);
            (best_local, target, "本地处理")
        };

        let quality = self.capability_matrix.predict_quality(model, intent);
        let explanation = format!(
            "{}：local_cap={:.2}, mem={:.2}, conf={:.2}, penalty={:.2}, score={:.2}, threshold={:.2}",
            reason_prefix, local_cap, memory_match, conf, penalty, final_score, threshold
        );

        let preprocessed_query = if escalated {
            Some(self.preprocess_for_cloud(&classification.primary, memory_context, input_tokens))
        } else {
            None
        };

        StewardDecision {
            route: RouteDecision {
                model,
                target,
                reason: explanation.clone(),
                quality_score: quality,
                estimated_cost: if model.is_local() { 0.0 } else { 0.001 },
            },
            escalated,
            reasoning: StewardReasoning {
                local_capability: local_cap,
                memory_match,
                classifier_confidence: conf,
                complexity_penalty: penalty,
                final_score,
                threshold,
                explanation,
            },
            preprocessed_query,
        }
    }

    /// 按复杂度选择最佳本地模型
    fn select_local_model(&self, complexity: u8, intent: &IntentType) -> ModelId {
        let candidate = match complexity {
            0..=4 => ModelId::Qwen3B,
            5..=7 => ModelId::Qwen7B,
            _ => ModelId::Qwen14B,
        };

        // 能力矩阵修正：如果候选模型对该意图得分 < 0.5，升级
        if self.capability_matrix.get_capability(candidate, intent) < 0.5 {
            match candidate {
                ModelId::Qwen3B => {
                    if self
                        .capability_matrix
                        .get_capability(ModelId::Qwen7B, intent)
                        >= 0.5
                    {
                        return ModelId::Qwen7B;
                    }
                    ModelId::Qwen14B
                }
                ModelId::Qwen7B => ModelId::Qwen14B,
                _ => ModelId::Qwen14B,
            }
        } else {
            candidate
        }
    }
    /// 选择最佳云端模型
    fn select_best_cloud(&self, intent: &IntentType) -> ModelId {
        let cloud_models = [
            ModelId::QwenTurbo,
            ModelId::QwenMax,
            ModelId::DeepSeekChat,
            ModelId::GPT4oMini,
            ModelId::GPT4o,
            ModelId::Claude35Sonnet,
            ModelId::ClaudeOpus,
            ModelId::GeminiFlash,
            ModelId::GeminiPro,
        ];
        cloud_models
            .iter()
            .max_by(|a, b| {
                let sa = self.capability_matrix.predict_quality(**a, intent);
                let sb = self.capability_matrix.predict_quality(**b, intent);
                sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .unwrap_or(ModelId::Claude35Sonnet)
    }

    /// V2 算法：基于目录的精密路由（难度评估 + 经济效益）
    fn decide_v2(
        &self,
        classification: &ClassificationResult,
        input_tokens: u64,
        memory_context: Option<&str>,
    ) -> StewardDecision {
        let intent = &classification.primary;
        let assessment = assess_difficulty(classification, "");
        let required_tier = assessment.required_tier;
        let domain = assessment.domain;

        // 候选模型筛选：按层级范围 + 能力达标
        // Low → 只看 Local + Budget fallback
        // Medium → Standard + Premium fallback
        // High → Premium only
        let max_tier = match required_tier {
            ModelTier::Local => ModelTier::Budget,
            ModelTier::Budget => ModelTier::Standard,
            ModelTier::Standard => ModelTier::Premium,
            ModelTier::Premium => ModelTier::Premium,
        };
        let candidates: Vec<_> = self
            .catalog
            .enabled_models()
            .into_iter()
            .filter(|p| p.tier >= required_tier && p.tier <= max_tier)
            .filter(|p| p.domain_scores.get(&domain).copied().unwrap_or(0.0) >= 0.5)
            .collect();

        // 如果无候选，fallback 到 v1
        if candidates.is_empty() {
            return self.decide_v1(classification, input_tokens, memory_context);
        }

        // 对每个候选评分
        let memory_match = self.compute_memory_match(intent);
        let conf = classification.confidence as f64;
        let complexity_pen = if classification.complexity > 3 {
            self.config.complexity_penalty * (classification.complexity.saturating_sub(3) as f64)
        } else {
            0.0
        };

        let scored: Vec<_> = candidates
            .iter()
            .map(|p| {
                let capability = p.domain_scores.get(&domain).copied().unwrap_or(0.0);
                let economic = self.compute_economic_score(p, input_tokens, &domain);
                let score = self.config.capability_weight * capability
                    + self.config.memory_weight * memory_match
                    + self.config.confidence_weight * conf
                    + self.config.economic_weight * economic
                    - complexity_pen;
                (p, score)
            })
            .collect();

        let (best, best_score) = scored
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .expect("scored is non-empty because candidates is non-empty");

        let escalated = !best.is_local;
        let target = if best.is_local {
            RouteTarget::Local(best.id.clone())
        } else {
            RouteTarget::Cloud(best.id.clone())
        };

        // 找到对应的 ModelId（用于 RouteDecision 兼容）
        let model_id = self.catalog_id_to_model_id(&best.id);
        let quality = best.domain_scores.get(&domain).copied().unwrap_or(0.0);
        let cost_est = if best.is_local {
            0.0
        } else {
            (input_tokens as f64 * best.input_price_per_m) / 1_000_000.0
        };

        let reason_prefix = if escalated {
            "升级云端"
        } else {
            "本地处理"
        };
        let explanation = format!(
            "{}(v2)[{:?}→{:?}]: model={}, cap={:.2}, eco={:.2}, score={:.2}",
            reason_prefix,
            assessment.difficulty,
            required_tier,
            best.id,
            quality,
            self.compute_economic_score(best, input_tokens, &domain),
            best_score
        );

        let preprocessed_query = if escalated {
            Some(self.preprocess_for_cloud(intent, memory_context, input_tokens))
        } else {
            None
        };

        StewardDecision {
            route: RouteDecision {
                model: model_id,
                target,
                reason: explanation.clone(),
                quality_score: quality,
                estimated_cost: cost_est,
            },
            escalated,
            reasoning: StewardReasoning {
                local_capability: quality,
                memory_match,
                classifier_confidence: conf,
                complexity_penalty: complexity_pen,
                final_score: *best_score,
                threshold: 0.0, // v2 不使用阈值
                explanation,
            },
            preprocessed_query,
        }
    }

    /// 经济效益评分
    fn compute_economic_score(
        &self,
        profile: &super::catalog::ModelProfile,
        input_tokens: u64,
        domain: &CapabilityDomain,
    ) -> f64 {
        if profile.is_local {
            return 1.0; // 免费 = 最高经济效益
        }
        let quality = profile.domain_scores.get(domain).copied().unwrap_or(0.0);
        let cost = (input_tokens as f64 * profile.input_price_per_m) / 1_000_000.0;
        quality / (cost * 1000.0 + 1.0)
    }

    /// catalog id → ModelId 映射
    fn catalog_id_to_model_id(&self, id: &str) -> ModelId {
        match id {
            "qwen-3b" => ModelId::Qwen3B,
            "qwen-7b" => ModelId::Qwen7B,
            "qwen-14b" => ModelId::Qwen14B,
            "qwen-turbo" => ModelId::QwenTurbo,
            "qwen-max" => ModelId::QwenMax,
            "deepseek-chat" => ModelId::DeepSeekChat,
            "gpt-4o-mini" => ModelId::GPT4oMini,
            "gpt-4o" => ModelId::GPT4o,
            "claude-sonnet" => ModelId::Claude35Sonnet,
            "claude-opus" => ModelId::ClaudeOpus,
            "gemini-flash" => ModelId::GeminiFlash,
            "gemini-pro" => ModelId::GeminiPro,
            _ => ModelId::Qwen7B, // fallback
        }
    }

    /// 计算历史记忆匹配率：同意图中本地成功且质量 > 0.6 的比例
    fn compute_memory_match(&self, intent: &IntentType) -> f64 {
        let same_intent: Vec<&InteractionRecord> = self
            .history
            .iter()
            .filter(|r| &r.intent == intent)
            .collect();

        if same_intent.is_empty() {
            return 0.0;
        }

        let success_count = same_intent
            .iter()
            .filter(|r| r.handled_locally && r.quality_score > 0.6)
            .count();

        success_count as f64 / same_intent.len() as f64
    }

    /// 升级云端时，准备预处理上下文
    fn preprocess_for_cloud(
        &self,
        intent: &IntentType,
        memory_context: Option<&str>,
        _input_tokens: u64,
    ) -> String {
        let mut parts = Vec::new();

        parts.push(format!("[意图] {:?}", intent));

        if let Some(mem) = memory_context {
            if !mem.is_empty() {
                parts.push(format!("[记忆上下文]\n{}", mem));
            }
        }

        parts.join("\n")
    }

    /// 评估回复质量（启发式）
    pub fn assess_response_quality(&self, _input: &str, reply: &str) -> f64 {
        if reply.trim().is_empty() {
            return 0.0;
        }

        let mut score: f64 = 0.0;

        // 回复长度适中 (50-500字) → +0.3
        let len = reply.chars().count();
        if (50..=500).contains(&len) {
            score += 0.3;
        } else if len > 10 {
            score += 0.15;
        }

        // 无明显重复/乱码 → +0.3
        if !has_repetition(reply) {
            score += 0.3;
        }

        // 基础质量分（非空回复）→ +0.4
        score += 0.4;

        score.min(1.0)
    }

    /// 记录交互结果并更新能力矩阵
    pub fn record_outcome(
        &mut self,
        intent: IntentType,
        model: ModelId,
        handled_locally: bool,
        quality: f64,
        query: &str,
    ) {
        let digest = query.chars().take(80).collect::<String>();

        self.history.push(InteractionRecord {
            intent: intent.clone(),
            query_digest: digest,
            model,
            handled_locally,
            quality_score: quality,
            timestamp: Utc::now(),
        });

        // 更新能力矩阵（EMA 学习）
        let old = self.capability_matrix.get_capability(model, &intent);
        let new_score =
            (1.0 - self.config.learning_rate) * old + self.config.learning_rate * quality;
        self.capability_matrix
            .update_capability(model, intent, new_score);

        // 裁剪历史
        if self.history.len() > self.config.max_history {
            let excess = self.history.len() - self.config.max_history;
            self.history.drain(..excess);
        }
    }

    /// 获取能力矩阵引用（用于测试和调试）
    pub fn capability_matrix(&self) -> &CapabilityMatrix {
        &self.capability_matrix
    }

    /// 获取历史记录长度
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}

/// 模型 ID 到路由目标的映射
fn model_to_target(model: ModelId) -> RouteTarget {
    match model {
        ModelId::Qwen3B => RouteTarget::Local("qwen2.5-3b".to_string()),
        ModelId::Qwen7B => RouteTarget::Local("qwen2.5-7b".to_string()),
        ModelId::Qwen14B => RouteTarget::Local("qwen2.5-14b".to_string()),
        ModelId::QwenTurbo => RouteTarget::Cloud("qwen-turbo".to_string()),
        ModelId::QwenMax => RouteTarget::Cloud("qwen-max".to_string()),
        ModelId::DeepSeekChat => RouteTarget::Cloud("deepseek-chat".to_string()),
        ModelId::GPT4oMini => RouteTarget::Cloud("gpt-4o-mini".to_string()),
        ModelId::GPT4o => RouteTarget::Cloud("gpt-4o".to_string()),
        ModelId::Claude35Sonnet => RouteTarget::Cloud("claude-3.5-sonnet".to_string()),
        ModelId::ClaudeOpus => RouteTarget::Cloud("claude-opus".to_string()),
        ModelId::GeminiFlash => RouteTarget::Cloud("gemini-flash".to_string()),
        ModelId::GeminiPro => RouteTarget::Cloud("gemini-pro".to_string()),
    }
}

/// 检测文本是否有明显重复
fn has_repetition(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 20 {
        return false;
    }
    // 检查连续 10 字符重复
    for window_size in [10, 20] {
        if chars.len() < window_size * 2 {
            continue;
        }
        for i in 0..chars.len() - window_size * 2 {
            let a: String = chars[i..i + window_size].iter().collect();
            let b: String = chars[i + window_size..i + window_size * 2].iter().collect();
            if a == b {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn casual_classification(complexity: u8) -> ClassificationResult {
        ClassificationResult {
            primary: IntentType::Casual,
            confidence: 0.85,
            complexity,
        }
    }

    fn reasoning_classification(complexity: u8, confidence: f32) -> ClassificationResult {
        ClassificationResult {
            primary: IntentType::Reasoning,
            confidence,
            complexity,
        }
    }
    #[test]
    fn decide_routes_casual_locally() {
        let steward = LocalSteward::with_defaults();
        let cls = casual_classification(3);
        let decision = steward.decide(&cls, 50, None);
        assert!(!decision.escalated, "闲聊应本地处理");
        assert!(decision.route.model.is_local());
    }

    #[test]
    fn decide_escalates_complex_reasoning() {
        let steward = LocalSteward::with_defaults();
        let cls = reasoning_classification(9, 0.9);
        let decision = steward.decide(&cls, 200, None);
        assert!(decision.escalated, "高复杂度推理应升级云端");
        assert!(!decision.route.model.is_local());
    }

    #[test]
    fn decide_uses_memory_match() {
        let mut steward = LocalSteward::with_defaults();
        // 填充历史：Casual 意图本地成功
        for _ in 0..10 {
            steward.record_outcome(IntentType::Casual, ModelId::Qwen3B, true, 0.8, "你好呀");
        }
        let cls = casual_classification(3);
        let decision = steward.decide(&cls, 50, None);
        assert!(!decision.escalated);
        // 有历史成功记录，memory_match 应 > 0
        assert!(decision.reasoning.memory_match > 0.0);
    }

    #[test]
    fn decide_low_confidence_escalates() {
        let steward = LocalSteward::with_defaults();
        // 低置信度 + 中等复杂度的推理
        let cls = reasoning_classification(6, 0.2);
        let decision = steward.decide(&cls, 100, None);
        // 低置信度降低 final_score，更可能升级
        assert!(decision.escalated, "低置信度应倾向升级");
    }

    #[test]
    fn decide_high_complexity_raises_threshold() {
        // 此测试验证 V1 逻辑中高复杂度提升阈值
        let mut config = StewardConfig::default();
        config.use_catalog_routing = false;
        let steward = LocalSteward::new(config);
        let low = casual_classification(3);
        let high = ClassificationResult {
            primary: IntentType::Casual,
            confidence: 0.85,
            complexity: 8,
        };
        let d_low = steward.decide(&low, 50, None);
        let d_high = steward.decide(&high, 50, None);
        assert!(d_high.reasoning.threshold > d_low.reasoning.threshold);
    }
    #[test]
    fn select_local_model_by_complexity() {
        let steward = LocalSteward::with_defaults();
        assert_eq!(
            steward.select_local_model(2, &IntentType::Casual),
            ModelId::Qwen3B
        );
        assert_eq!(
            steward.select_local_model(6, &IntentType::Knowledge),
            ModelId::Qwen7B
        );
        assert_eq!(
            steward.select_local_model(9, &IntentType::Reasoning),
            ModelId::Qwen14B
        );
    }

    #[test]
    fn record_outcome_updates_capability() {
        let mut steward = LocalSteward::with_defaults();
        let old = steward
            .capability_matrix
            .get_capability(ModelId::Qwen3B, &IntentType::Casual);

        steward.record_outcome(IntentType::Casual, ModelId::Qwen3B, true, 1.0, "测试");

        let new = steward
            .capability_matrix
            .get_capability(ModelId::Qwen3B, &IntentType::Casual);
        // learning_rate=0.15, quality=1.0 → new > old (if old < 1.0)
        assert!(new > old || (old - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn record_outcome_trims_history() {
        let config = StewardConfig {
            max_history: 5,
            ..StewardConfig::default()
        };
        let mut steward = LocalSteward::new(config);

        for i in 0..10 {
            steward.record_outcome(
                IntentType::Casual,
                ModelId::Qwen3B,
                true,
                0.7,
                &format!("query {}", i),
            );
        }
        assert_eq!(steward.history_len(), 5);
    }

    #[test]
    fn preprocess_adds_memory_context() {
        let steward = LocalSteward::with_defaults();
        let cls = reasoning_classification(9, 0.9);
        let decision = steward.decide(&cls, 200, Some("用户喜欢 Rust 编程"));
        assert!(decision.escalated);
        let query = decision.preprocessed_query.unwrap();
        assert!(query.contains("记忆上下文"));
        assert!(query.contains("Rust"));
    }

    #[test]
    fn learning_shifts_boundary_over_time() {
        let mut steward = LocalSteward::with_defaults();

        // 初始：Knowledge + complexity=5 可能升级
        let cls = ClassificationResult {
            primary: IntentType::Knowledge,
            confidence: 0.7,
            complexity: 5,
        };
        let initial = steward.decide(&cls, 100, None);

        // 多次本地高质量处理后，能力矩阵提升
        for _ in 0..20 {
            steward.record_outcome(
                IntentType::Knowledge,
                ModelId::Qwen7B,
                true,
                0.95,
                "知识问答测试",
            );
        }

        let after_learning = steward.decide(&cls, 100, None);
        // 学习后 final_score 应该更高
        assert!(
            after_learning.reasoning.final_score >= initial.reasoning.final_score,
            "学习后本地置信度应提升: before={:.3}, after={:.3}",
            initial.reasoning.final_score,
            after_learning.reasoning.final_score
        );
    }

    #[test]
    fn assess_response_quality_scoring() {
        let steward = LocalSteward::with_defaults();

        // 适中长度、无重复 → 高分
        let good = "这是一个关于 Rust 所有权系统的详细解释。".repeat(5);
        let score = steward.assess_response_quality("问题", &good);
        assert!(score >= 0.7);

        // 空回复 → 低分
        let empty_score = steward.assess_response_quality("问题", "");
        assert!(empty_score < 0.3);
    }

    // ─── V2 算法测试 ───────────────────────────────────

    #[test]
    fn v2_low_difficulty_routes_local() {
        let steward = LocalSteward::with_defaults();
        let cls = casual_classification(2);
        let d = steward.decide(&cls, 50, None);
        assert!(!d.escalated, "低难度应本地处理");
        assert!(d.route.model.is_local());
    }

    #[test]
    fn v2_high_difficulty_routes_premium() {
        let steward = LocalSteward::with_defaults();
        let cls = reasoning_classification(9, 0.9);
        let d = steward.decide(&cls, 200, None);
        assert!(d.escalated, "高难度应升级云端");
        assert!(!d.route.model.is_local());
    }

    #[test]
    fn v2_medium_routes_standard() {
        let steward = LocalSteward::with_defaults();
        let cls = ClassificationResult {
            primary: IntentType::CodeGen,
            confidence: 0.8,
            complexity: 5,
        };
        let d = steward.decide(&cls, 100, None);
        // Medium → Standard 层，应选云端模型
        assert!(d.escalated);
    }

    #[test]
    fn v2_prefers_cheaper_at_same_tier() {
        let steward = LocalSteward::with_defaults();
        // Standard 层数学任务：deepseek-chat (0.14$/M) vs qwen-max (2.00$/M)
        let cls = ClassificationResult {
            primary: IntentType::Reasoning,
            confidence: 0.8,
            complexity: 6,
        };
        let d = steward.decide(&cls, 1000, None);
        // DeepSeek 数学能力高且便宜，经济效益更优
        // 验证选择的不是最贵的模型
        assert!(d.route.estimated_cost < 0.01, "应选经济效益高的模型");
    }

    #[test]
    fn v2_coding_prefers_high_capability() {
        let steward = LocalSteward::with_defaults();
        let cls = ClassificationResult {
            primary: IntentType::CodeGen,
            confidence: 0.9,
            complexity: 8,
        };
        let d = steward.decide(&cls, 200, None);
        // Premium 层编程：应选高能力模型（非本地）
        assert!(d.escalated, "高难度编程应升级云端");
        assert!(!d.route.model.is_local());
        assert!(
            d.route.quality_score >= 0.85,
            "编程模型能力应 >= 0.85，实际: {:.2}",
            d.route.quality_score
        );
    }

    #[test]
    fn v2_fallback_when_no_enabled() {
        let mut config = StewardConfig::default();
        config.use_catalog_routing = true;
        let steward = LocalSteward::new(config);
        // 即使目录路由开启，也不会 panic
        let cls = casual_classification(2);
        let d = steward.decide(&cls, 50, None);
        assert!(d.route.quality_score > 0.0);
    }

    #[test]
    fn v2_backward_compat_flag() {
        let mut config = StewardConfig::default();
        config.use_catalog_routing = false;
        let steward = LocalSteward::new(config);
        let cls = casual_classification(3);
        let d = steward.decide(&cls, 50, None);
        // V1 逻辑：闲聊本地处理
        assert!(!d.escalated);
        assert!(d.route.model.is_local());
    }
}
