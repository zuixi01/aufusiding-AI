//! 路由策略
//!
//! 根据意图分类结果、设备资源状况、网络状态等因素，决定使用哪个推理后端。

use super::capability::{CapabilityMatrix, ModelId};
use super::classifier::{ClassificationResult, IntentType};
use super::cost::CostCalculator;
use linked_hash_map::LinkedHashMap;
use serde::{Deserialize, Serialize};

/// 路由策略类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    /// 本地优先：优先使用本地模型，失败才云端
    LocalFirst,
    /// 成本优化：最小化成本
    CostOptimized,
    /// 质量优先：最大化质量
    QualityFirst,
    /// 平衡模式：质量、成本、速度平衡
    Balanced,
    /// 仅本地：强制使用本地模型
    OfflineOnly,
    /// 大管家模式：本地模型自治决策，由 LocalSteward 驱动
    Steward,
}

/// 路由约束上下文
#[derive(Debug, Clone)]
pub struct RoutingConstraints {
    pub network_available: bool,
    pub prefer_local: bool,
    /// 当前请求是否需要云端特性（如复杂工具调用/视觉）
    pub require_cloud_capability: bool,
    /// 是否优先低延迟返回
    pub latency_sensitive: bool,
}

impl Default for RoutingConstraints {
    fn default() -> Self {
        Self {
            network_available: true,
            prefer_local: true,
            require_cloud_capability: false,
            latency_sensitive: false,
        }
    }
}

/// 路由目标
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RouteTarget {
    /// 本地推理（指定模型名）
    Local(String),
    /// 云端 API（指定 provider 名）
    Cloud(String),
    /// 混合模式：本地预处理 + 云端精炼
    Hybrid { local: String, cloud: String },
}

/// 路由决策
#[derive(Debug, Clone)]
pub struct RouteDecision {
    /// 选择的模型
    pub model: ModelId,
    /// 路由目标
    pub target: RouteTarget,
    /// 决策理由
    pub reason: String,
    /// 预估质量得分
    pub quality_score: f64,
    /// 预估成本
    pub estimated_cost: f64,
}

/// 路由决策缓存最大条目数
const ROUTE_CACHE_MAX: usize = 32;

/// 路由决策缓存键
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct RouteCacheKey {
    intent: IntentType,
    complexity: u8,
    tokens_bucket: u64,
}

/// 智能路由器
pub struct Router {
    capability_matrix: CapabilityMatrix,
    cost_calculator: CostCalculator,
    strategy: RoutingStrategy,
    route_cache: LinkedHashMap<RouteCacheKey, RouteDecision>,
}

impl Router {
    /// 创建新的路由器
    pub fn new(strategy: RoutingStrategy) -> Self {
        Self {
            capability_matrix: CapabilityMatrix::new(),
            cost_calculator: CostCalculator::new(),
            strategy,
            route_cache: LinkedHashMap::new(),
        }
    }

    /// 选择最优模型（带缓存）
    pub fn select_model(
        &mut self,
        classification: &ClassificationResult,
        input_tokens: u64,
    ) -> RouteDecision {
        // 构建缓存键：intent + complexity + token 桶（按 256 分桶减少缓存 miss）
        let cache_key = RouteCacheKey {
            intent: classification.primary.clone(),
            complexity: classification.complexity,
            tokens_bucket: input_tokens / 256,
        };

        // 缓存命中
        if let Some(cached) = self.route_cache.get_refresh(&cache_key) {
            return cached.clone();
        }

        let intent = &classification.primary;
        let candidates = self.capability_matrix.all_models();

        let mut scored_models: Vec<(ModelId, f64)> = candidates
            .iter()
            .map(|model| {
                let score = self.calculate_score(*model, intent, input_tokens);
                (*model, score)
            })
            .collect();

        // 按得分排序
        scored_models.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let best_model = scored_models[0].0;
        let quality = self.capability_matrix.predict_quality(best_model, intent);
        let cost = self
            .cost_calculator
            .estimate_total_cost(best_model, input_tokens, intent);

        let decision = RouteDecision {
            model: best_model,
            target: self.model_to_target(best_model),
            reason: self.generate_reason(best_model, &self.strategy, quality, cost),
            quality_score: quality,
            estimated_cost: cost,
        };

        // 写入缓存，超容量时淘汰最久未使用
        if self.route_cache.len() >= ROUTE_CACHE_MAX {
            self.route_cache.pop_front();
        }
        self.route_cache.insert(cache_key, decision.clone());

        decision
    }

    /// 计算模型得分
    fn calculate_score(&self, model: ModelId, intent: &IntentType, input_tokens: u64) -> f64 {
        let quality = self.capability_matrix.predict_quality(model, intent);
        let cost = self
            .cost_calculator
            .estimate_total_cost(model, input_tokens, intent);
        let speed = self.get_speed_score(model);

        match self.strategy {
            RoutingStrategy::LocalFirst => {
                if model.is_local() {
                    quality + 10.0 // 本地模型加分
                } else {
                    quality - cost * 0.5
                }
            }
            RoutingStrategy::CostOptimized => {
                let normalized_cost = (cost / 0.01).min(1.0);
                quality * 0.3 - normalized_cost * 0.7
            }
            RoutingStrategy::QualityFirst => {
                let normalized_cost = (cost / 0.01).min(1.0);
                quality * 0.9 - normalized_cost * 0.1
            }
            RoutingStrategy::Balanced => {
                let normalized_cost = (cost / 0.01).min(1.0);
                0.6 * quality - 0.3 * normalized_cost + 0.1 * speed
            }
            RoutingStrategy::OfflineOnly => {
                if model.is_local() {
                    quality
                } else {
                    -1000.0 // 云端模型极低分
                }
            }
            RoutingStrategy::Steward => {
                // Steward 模式下 Router::select_model 不应被调用，
                // 由 LocalSteward 直接决策。此处作为 fallback 使用 Balanced 逻辑。
                let normalized_cost = (cost / 0.01).min(1.0);
                0.6 * quality - 0.3 * normalized_cost + 0.1 * speed
            }
        }
    }

    /// 获取速度得分
    fn get_speed_score(&self, model: ModelId) -> f64 {
        match model {
            ModelId::Qwen3B => 1.0,
            ModelId::Qwen7B => 0.8,
            ModelId::Qwen14B => 0.6,
            ModelId::QwenTurbo => 0.55,
            ModelId::QwenMax => 0.4,
            ModelId::DeepSeekChat => 0.45,
            ModelId::GPT4oMini => 0.5,
            ModelId::GPT4o => 0.3,
            ModelId::Claude35Sonnet => 0.4,
            ModelId::ClaudeOpus => 0.2,
            ModelId::GeminiFlash => 0.55,
            ModelId::GeminiPro => 0.35,
        }
    }

    /// 将模型 ID 转换为路由目标
    fn model_to_target(&self, model: ModelId) -> RouteTarget {
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

    /// 生成决策理由
    fn generate_reason(
        &self,
        model: ModelId,
        strategy: &RoutingStrategy,
        quality: f64,
        cost: f64,
    ) -> String {
        let model_name = format!("{:?}", model);
        match strategy {
            RoutingStrategy::LocalFirst => {
                if model.is_local() {
                    format!("本地优先策略：选择 {} (质量: {:.2})", model_name, quality)
                } else {
                    format!(
                        "本地模型不足，使用云端 {} (质量: {:.2}, 成本: ${:.4})",
                        model_name, quality, cost
                    )
                }
            }
            RoutingStrategy::CostOptimized => {
                format!(
                    "成本优化：选择 {} (质量: {:.2}, 成本: ${:.4})",
                    model_name, quality, cost
                )
            }
            RoutingStrategy::QualityFirst => {
                format!("质量优先：选择 {} (质量: {:.2})", model_name, quality)
            }
            RoutingStrategy::Balanced => {
                format!(
                    "平衡策略：选择 {} (质量: {:.2}, 成本: ${:.4})",
                    model_name, quality, cost
                )
            }
            RoutingStrategy::OfflineOnly => {
                format!("离线模式：选择本地 {} (质量: {:.2})", model_name, quality)
            }
            RoutingStrategy::Steward => {
                format!(
                    "大管家模式 fallback：选择 {} (质量: {:.2}, 成本: ${:.4})",
                    model_name, quality, cost
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_classification(intent: IntentType, complexity: u8) -> ClassificationResult {
        ClassificationResult {
            primary: intent,
            confidence: 0.8,
            complexity,
        }
    }

    #[test]
    fn test_local_first_strategy() {
        let mut router = Router::new(RoutingStrategy::LocalFirst);
        let classification = create_classification(IntentType::Casual, 3);

        let decision = router.select_model(&classification, 100);

        // 本地优先应该选择本地模型
        assert!(decision.model.is_local());
        println!("LocalFirst: {:?} - {}", decision.model, decision.reason);
    }

    #[test]
    fn test_quality_first_strategy() {
        let mut router = Router::new(RoutingStrategy::QualityFirst);
        let classification = create_classification(IntentType::Reasoning, 8);

        let decision = router.select_model(&classification, 100);

        // 质量优先应该选择高质量模型（可能是云端）
        assert!(
            decision.quality_score > 0.85,
            "质量优先应选高质量模型，实际: {:.3}",
            decision.quality_score
        );
        println!("QualityFirst: {:?} - {}", decision.model, decision.reason);
    }

    #[test]
    fn test_cost_optimized_strategy() {
        let mut router = Router::new(RoutingStrategy::CostOptimized);
        let classification = create_classification(IntentType::Knowledge, 5);

        let decision = router.select_model(&classification, 100);

        // 成本优化应该选择本地模型（成本为0）
        assert!(decision.model.is_local());
        assert!(decision.estimated_cost < 0.001);
        println!("CostOptimized: {:?} - {}", decision.model, decision.reason);
    }

    #[test]
    fn test_balanced_strategy() {
        let mut router = Router::new(RoutingStrategy::Balanced);
        let classification = create_classification(IntentType::CodeGen, 6);

        let decision = router.select_model(&classification, 100);

        // 平衡策略应该在质量和成本之间权衡
        println!(
            "Balanced: {:?} - {} (quality: {:.2}, cost: ${:.4})",
            decision.model, decision.reason, decision.quality_score, decision.estimated_cost
        );
        assert!(decision.quality_score > 0.5);
    }

    #[test]
    fn test_offline_only_strategy() {
        let mut router = Router::new(RoutingStrategy::OfflineOnly);
        let classification = create_classification(IntentType::Reasoning, 9);

        let decision = router.select_model(&classification, 100);

        // 离线模式必须选择本地模型
        assert!(decision.model.is_local());
        println!("OfflineOnly: {:?} - {}", decision.model, decision.reason);
    }

    #[test]
    fn test_different_intents() {
        let mut router = Router::new(RoutingStrategy::Balanced);

        // 测试不同意图的路由决策
        let intents = vec![
            IntentType::Casual,
            IntentType::Knowledge,
            IntentType::CodeGen,
            IntentType::Creative,
            IntentType::Reasoning,
        ];

        for intent in intents {
            let classification = create_classification(intent.clone(), 5);
            let decision = router.select_model(&classification, 100);
            println!(
                "{:?} -> {:?} (quality: {:.2})",
                intent, decision.model, decision.quality_score
            );
        }
    }

    #[test]
    fn test_token_count_affects_cost() {
        let mut router = Router::new(RoutingStrategy::CostOptimized);
        let classification = create_classification(IntentType::Knowledge, 5);

        let decision_small = router.select_model(&classification, 100);
        let decision_large = router.select_model(&classification, 10000);

        // 大量 token 应该影响成本
        println!("Small tokens: ${:.6}", decision_small.estimated_cost);
        println!("Large tokens: ${:.6}", decision_large.estimated_cost);
    }

    #[test]
    fn test_route_target_mapping() {
        let router = Router::new(RoutingStrategy::Balanced);

        assert_eq!(
            router.model_to_target(ModelId::Qwen3B),
            RouteTarget::Local("qwen2.5-3b".to_string())
        );

        assert_eq!(
            router.model_to_target(ModelId::GPT4o),
            RouteTarget::Cloud("gpt-4o".to_string())
        );
    }

    #[test]
    fn test_route_cache_consistency() {
        let mut router = Router::new(RoutingStrategy::Balanced);
        let classification = create_classification(IntentType::Knowledge, 5);

        let decision1 = router.select_model(&classification, 100);
        let decision2 = router.select_model(&classification, 100);

        // 相同输入应返回相同决策（缓存命中）
        assert_eq!(decision1.model, decision2.model);
        assert_eq!(decision1.target, decision2.target);
        assert!((decision1.quality_score - decision2.quality_score).abs() < 0.001);
    }
}
