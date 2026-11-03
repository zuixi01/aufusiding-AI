//! 复杂度评估器 — 决定请求走原有路径还是 Agent 集群
//!
//! 根据意图分类结果和输入文本，判断任务复杂度，
//! 简单任务走 ChatOrchestrator 原路径，复杂任务启动 Agent 集群。

use crate::agent::types::Role;
use crate::router::classifier::{ClassificationResult, IntentType};

/// 任务路由决策
#[derive(Debug, Clone)]
pub enum TaskRoute {
    /// 简单任务，走原有单次 LLM 路径
    Simple { reason: String },
    /// 复杂任务，启动 Agent 集群
    Complex {
        reason: String,
        suggested_roles: Vec<Role>,
    },
}

/// 复杂度评估器
pub struct ComplexityAssessor {
    /// 复杂度阈值（1-10），>= 此值视为复杂任务
    pub complexity_threshold: u8,
}

impl ComplexityAssessor {
    pub fn new() -> Self {
        Self {
            complexity_threshold: 7,
        }
    }

    pub fn with_threshold(mut self, threshold: u8) -> Self {
        self.complexity_threshold = threshold.clamp(1, 10);
        self
    }

    /// 评估任务复杂度，决定路由
    pub fn assess(&self, classification: &ClassificationResult, input: &str) -> TaskRoute {
        // 1. 高复杂度直接走集群
        if classification.complexity >= self.complexity_threshold {
            return TaskRoute::Complex {
                reason: format!(
                    "复杂度 {} >= 阈值 {}",
                    classification.complexity, self.complexity_threshold
                ),
                suggested_roles: self.suggest_roles(classification, input),
            };
        }

        // 2. 多步骤关键词检测
        if self.has_multi_step_indicators(input) {
            return TaskRoute::Complex {
                reason: "检测到多步骤任务指标".into(),
                suggested_roles: self.suggest_roles(classification, input),
            };
        }

        // 3. 其他走简单路径
        TaskRoute::Simple {
            reason: format!(
                "复杂度 {} < 阈值 {}",
                classification.complexity, self.complexity_threshold
            ),
        }
    }

    /// 检测多步骤任务指标
    fn has_multi_step_indicators(&self, input: &str) -> bool {
        let indicators = [
            "设计并实现",
            "分析并优化",
            "先分析",
            "然后实现",
            "第一步",
            "第二步",
            "需要多个",
            "完整方案",
            "端到端",
            "全流程",
        ];
        indicators.iter().any(|kw| input.contains(kw))
    }

    /// 根据意图和输入建议所需角色
    fn suggest_roles(&self, classification: &ClassificationResult, input: &str) -> Vec<Role> {
        let mut roles = Vec::new();

        match classification.primary {
            IntentType::CodeGen => {
                roles.push(Role::Engineer);
                roles.push(Role::QA);
                if input.contains("安全") || input.contains("认证") || input.contains("加密")
                {
                    roles.push(Role::Security);
                }
                if classification.complexity >= 8 {
                    roles.push(Role::Architect);
                }
            }
            IntentType::Reasoning => {
                roles.push(Role::Architect);
                roles.push(Role::Researcher);
            }
            IntentType::ToolUse => {
                roles.push(Role::Engineer);
                roles.push(Role::DevOps);
            }
            _ => {
                roles.push(Role::Engineer);
            }
        }

        // 复杂任务总是需要 PM 做需求拆解
        if classification.complexity >= 8 && !roles.contains(&Role::ProductManager) {
            roles.push(Role::ProductManager);
        }

        roles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_classification(primary: IntentType, complexity: u8) -> ClassificationResult {
        ClassificationResult {
            primary,
            confidence: 0.8,
            complexity,
        }
    }

    #[test]
    fn simple_casual_routes_simple() {
        let assessor = ComplexityAssessor::new();
        let c = make_classification(IntentType::Casual, 2);
        let route = assessor.assess(&c, "你好");
        assert!(matches!(route, TaskRoute::Simple { .. }));
    }

    #[test]
    fn high_complexity_routes_complex() {
        let assessor = ComplexityAssessor::new();
        let c = make_classification(IntentType::CodeGen, 8);
        let route = assessor.assess(&c, "实现一个分布式缓存系统");
        match route {
            TaskRoute::Complex {
                suggested_roles, ..
            } => {
                assert!(suggested_roles.contains(&Role::Engineer));
                assert!(suggested_roles.contains(&Role::Architect));
            }
            _ => panic!("expected Complex"),
        }
    }

    #[test]
    fn multi_step_keywords_trigger_complex() {
        let assessor = ComplexityAssessor::new();
        let c = make_classification(IntentType::CodeGen, 5); // below threshold
        let route = assessor.assess(&c, "设计并实现用户认证模块");
        assert!(matches!(route, TaskRoute::Complex { .. }));
    }

    #[test]
    fn security_keywords_add_security_role() {
        let assessor = ComplexityAssessor::new();
        let c = make_classification(IntentType::CodeGen, 8);
        let route = assessor.assess(&c, "实现安全认证系统");
        match route {
            TaskRoute::Complex {
                suggested_roles, ..
            } => {
                assert!(suggested_roles.contains(&Role::Security));
            }
            _ => panic!("expected Complex"),
        }
    }

    #[test]
    fn custom_threshold_works() {
        let assessor = ComplexityAssessor::new().with_threshold(3);
        let c = make_classification(IntentType::Knowledge, 4);
        let route = assessor.assess(&c, "解释量子计算");
        assert!(matches!(route, TaskRoute::Complex { .. }));
    }

    #[test]
    fn reasoning_suggests_architect_and_researcher() {
        let assessor = ComplexityAssessor::new();
        let c = make_classification(IntentType::Reasoning, 8);
        let route = assessor.assess(&c, "分析系统瓶颈");
        match route {
            TaskRoute::Complex {
                suggested_roles, ..
            } => {
                assert!(suggested_roles.contains(&Role::Architect));
                assert!(suggested_roles.contains(&Role::Researcher));
            }
            _ => panic!("expected Complex"),
        }
    }
}
