//! LLM 级联路由 — 根据任务复杂度和步骤选择模型层级
//!
//! 简单任务用 Fast 模型，复杂任务用 Premium 模型，降低整体 token 成本。

use super::model_pool::ModelTier;

/// 级联路由器
pub struct CascadeRouter {
    /// 低复杂度阈值（< 此值用 Fast）
    low_threshold: u32,
    /// 高复杂度阈值（>= 此值用 Premium）
    high_threshold: u32,
    /// 深度推理步数阈值
    deep_step_threshold: u32,
}

impl CascadeRouter {
    pub fn new() -> Self {
        Self {
            low_threshold: 5,
            high_threshold: 8,
            deep_step_threshold: 10,
        }
    }

    /// 根据任务复杂度和当前步数选择模型层级
    pub fn route(&self, task_complexity: u32, step_number: u32) -> ModelTier {
        if step_number > self.deep_step_threshold || task_complexity >= self.high_threshold {
            ModelTier::Premium
        } else if task_complexity < self.low_threshold || step_number <= 2 {
            ModelTier::Fast
        } else {
            ModelTier::Standard
        }
    }

    /// 质量不足时升级模型层级
    pub fn escalate(&self, current_tier: &ModelTier) -> ModelTier {
        match current_tier {
            ModelTier::Fast => ModelTier::Standard,
            ModelTier::Standard => ModelTier::Premium,
            ModelTier::Premium => ModelTier::Premium,
        }
    }
}

impl Default for CascadeRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_task_uses_fast() {
        let router = CascadeRouter::new();
        // complexity=3 < 5, step=1 ≤ 2 → Fast
        assert_eq!(router.route(3, 1), ModelTier::Fast);
        assert_eq!(router.route(4, 2), ModelTier::Fast);
    }

    #[test]
    fn complex_task_uses_premium() {
        let router = CascadeRouter::new();
        // complexity=9 ≥ 8 → Premium
        assert_eq!(router.route(9, 1), ModelTier::Premium);
        // step=11 > 10 → Premium
        assert_eq!(router.route(3, 11), ModelTier::Premium);
    }

    #[test]
    fn quality_escalation() {
        let router = CascadeRouter::new();
        assert_eq!(router.escalate(&ModelTier::Fast), ModelTier::Standard);
        assert_eq!(router.escalate(&ModelTier::Standard), ModelTier::Premium);
        assert_eq!(router.escalate(&ModelTier::Premium), ModelTier::Premium);
    }

    #[test]
    fn tier_recorded_correctly() {
        let router = CascadeRouter::new();
        // 中等复杂度，中间步骤 → Standard
        let tier = router.route(6, 5);
        assert_eq!(tier, ModelTier::Standard);
        let tier_str = format!("{:?}", tier);
        assert_eq!(tier_str, "Standard");
    }
}
