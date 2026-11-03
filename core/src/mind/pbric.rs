//! PBRIC 认知循环
//!
//! Perceive → Believe → Reason → Intend → Commit 五阶段认知循环。
//! v2: 集成 BeliefModule、ReflectionModule、ThoughtLogger。

use super::belief::{BeliefModule, Evidence};
use super::reflection::ReflectionModule;
use super::thought_log::{ThinkingPhase, ThoughtLogger};

/// PBRIC 循环阶段
#[derive(Debug, Clone, PartialEq)]
pub enum PbricPhase {
    Perceive,
    Believe,
    Reason,
    Intend,
    Commit,
}

/// 单阶段输出
#[derive(Debug, Clone)]
pub struct PhaseOutput {
    pub phase: PbricPhase,
    pub summary: String,
    pub confidence: f64,
}

/// 推理结果
#[derive(Debug, Clone)]
pub struct ReasoningResult {
    pub analysis: String,
    pub confidence: f64,
    pub alternatives: Vec<String>,
}

/// 意图计划
#[derive(Debug, Clone)]
pub struct IntentPlan {
    pub action: String,
    pub rationale: String,
    pub confidence: f64,
}

/// PBRIC 循环引擎 v2
pub struct PbricLoop {
    current_phase: PbricPhase,
    latest_input: Option<String>,
    belief: BeliefModule,
    reflection: ReflectionModule,
    thought_logger: ThoughtLogger,
    session_id: String,
    phase_outputs: Vec<PhaseOutput>,
    reasoning_result: Option<ReasoningResult>,
    intent_plan: Option<IntentPlan>,
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

impl PbricLoop {
    pub fn new() -> Self {
        Self {
            current_phase: PbricPhase::Perceive,
            latest_input: None,
            belief: BeliefModule::new(),
            reflection: ReflectionModule::new(),
            thought_logger: ThoughtLogger::new(10000),
            session_id: "default".to_string(),
            phase_outputs: Vec::new(),
            reasoning_result: None,
            intent_plan: None,
        }
    }

    pub fn with_session(mut self, session_id: &str) -> Self {
        self.session_id = session_id.to_string();
        self
    }

    /// 执行一轮完整循环
    pub async fn run_cycle(&mut self, input: &str) -> anyhow::Result<String> {
        self.phase_outputs.clear();
        self.reasoning_result = None;
        self.intent_plan = None;
        self.thought_logger.begin_chain(&self.session_id, input);

        self.current_phase = PbricPhase::Perceive;
        let out_perceive = self.perceive(input);
        self.phase_outputs.push(out_perceive);

        self.current_phase = PbricPhase::Believe;
        let out_believe = self.believe();
        self.phase_outputs.push(out_believe);

        self.current_phase = PbricPhase::Reason;
        let out_reason = self.reason();
        self.phase_outputs.push(out_reason);

        self.current_phase = PbricPhase::Intend;
        let out_intend = self.intend();
        self.phase_outputs.push(out_intend);

        self.current_phase = PbricPhase::Commit;
        let result = self.commit();

        self.thought_logger.end_chain(Some(&result));
        Ok(result)
    }

    /// 感知阶段：接收并规范化输入
    fn perceive(&mut self, input: &str) -> PhaseOutput {
        let cleaned = input.trim().to_string();
        self.latest_input = Some(cleaned.clone());

        let summary = format!("感知输入：{} 字符", cleaned.len());
        self.thought_logger.log_thought(
            &self.session_id,
            ThinkingPhase::Perceive,
            &summary,
            &format!("接收原始输入并规范化：「{}」", truncate(&cleaned, 100)),
            input,
            None,
            0.95,
            1,
        );
        PhaseOutput {
            phase: PbricPhase::Perceive,
            summary,
            confidence: 0.95,
        }
    }

    /// 信念阶段：基于输入更新信念状态
    fn believe(&mut self) -> PhaseOutput {
        let input = self.latest_input.clone().unwrap_or_default();

        // 识别用户意图并更新偏好
        let intent = self.reflection.identify_intent(&input);
        let intent_name = format!("{:?}", intent.intent_type);

        self.belief.update_belief(Evidence::UserFeedback {
            preference: format!("intent_{}", intent_name),
            value: intent.confidence,
        });

        let confidence = self.belief.get_confidence();
        let summary = format!("信念更新：意图={}, 置信度={:.2}", intent_name, confidence);

        self.thought_logger.log_thought(
            &self.session_id,
            ThinkingPhase::Believe,
            &summary,
            &format!(
                "识别意图「{}」(置信度 {:.2})，更新信念状态",
                intent_name, intent.confidence
            ),
            &input,
            None,
            confidence,
            2,
        );
        PhaseOutput {
            phase: PbricPhase::Believe,
            summary,
            confidence,
        }
    }

    /// 推理阶段：分析输入并生成推理结果
    fn reason(&mut self) -> PhaseOutput {
        let input = self.latest_input.clone().unwrap_or_default();
        let intent = self.reflection.get_current_intent();

        // 基于意图类型选择推理策略
        let (analysis, confidence, alternatives) = match intent {
            Some(i) => {
                let base_conf = i.confidence;
                let goals: Vec<String> = i.implicit_goals.clone();
                let analysis = format!(
                    "分析「{}」→ 目标: {}",
                    truncate(&input, 50),
                    if goals.is_empty() {
                        "通用响应".into()
                    } else {
                        goals.join(", ")
                    }
                );
                let alts = vec![
                    "直接回答".to_string(),
                    "追问澄清".to_string(),
                    "提供示例".to_string(),
                ];
                (analysis, base_conf * 0.9, alts)
            }
            None => (
                "无明确意图，采用通用推理".to_string(),
                0.5,
                vec!["通用响应".to_string()],
            ),
        };

        self.reasoning_result = Some(ReasoningResult {
            analysis: analysis.clone(),
            confidence,
            alternatives: alternatives.clone(),
        });

        let summary = format!(
            "推理完成：置信度={:.2}, 备选方案={}",
            confidence,
            alternatives.len()
        );
        self.thought_logger.log_thought(
            &self.session_id,
            ThinkingPhase::Reason,
            &summary,
            &analysis,
            &input,
            Some(&alternatives[0]),
            confidence,
            5,
        );
        PhaseOutput {
            phase: PbricPhase::Reason,
            summary,
            confidence,
        }
    }

    /// 意图阶段：基于推理结果形成行动计划
    fn intend(&mut self) -> PhaseOutput {
        let input = self.latest_input.clone().unwrap_or_default();
        let reasoning = self.reasoning_result.clone();

        let plan = match reasoning {
            Some(r) => {
                let action = r
                    .alternatives
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "通用响应".to_string());
                IntentPlan {
                    action: action.clone(),
                    rationale: r.analysis,
                    confidence: r.confidence,
                }
            }
            None => IntentPlan {
                action: "通用响应".to_string(),
                rationale: "无推理结果，采用默认策略".to_string(),
                confidence: 0.3,
            },
        };

        let summary = format!(
            "意图形成：行动={}, 置信度={:.2}",
            plan.action, plan.confidence
        );
        self.thought_logger.log_thought(
            &self.session_id,
            ThinkingPhase::Intend,
            &summary,
            &plan.rationale,
            &input,
            Some(&plan.action),
            plan.confidence,
            3,
        );

        let confidence = plan.confidence;
        self.intent_plan = Some(plan);
        PhaseOutput {
            phase: PbricPhase::Intend,
            summary,
            confidence,
        }
    }

    /// 承诺阶段：执行行动并生成输出
    fn commit(&mut self) -> String {
        let input = self
            .latest_input
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("empty");

        let plan = self.intent_plan.clone();
        let result = match plan {
            Some(p) => format!("commit:{}|action:{}", input, p.action),
            None => format!("commit:{}", input),
        };

        self.thought_logger.log_thought(
            &self.session_id,
            ThinkingPhase::Commit,
            &format!("执行承诺：{}", truncate(&result, 80)),
            "基于意图计划生成最终输出",
            input,
            Some(&result),
            0.9,
            2,
        );
        result
    }

    /// 获取当前阶段
    pub fn current_phase(&self) -> &PbricPhase {
        &self.current_phase
    }

    /// 获取最近一轮的阶段输出
    pub fn phase_outputs(&self) -> &[PhaseOutput] {
        &self.phase_outputs
    }

    /// 获取信念模块引用
    pub fn belief(&self) -> &BeliefModule {
        &self.belief
    }

    /// 获取思维日志引用
    pub fn thought_logger(&self) -> &ThoughtLogger {
        &self.thought_logger
    }
}

impl Default for PbricLoop {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_cycle_completes_and_returns_commit_result() {
        let mut pbric = PbricLoop::new();
        let result = pbric.run_cycle("请分析今天任务").await.unwrap();
        assert!(result.contains("commit"));
    }

    #[tokio::test]
    async fn run_cycle_records_all_phases() {
        let mut pbric = PbricLoop::new();
        pbric.run_cycle("你好").await.unwrap();

        assert_eq!(pbric.phase_outputs().len(), 4);
        assert_eq!(pbric.phase_outputs()[0].phase, PbricPhase::Perceive);
        assert_eq!(pbric.phase_outputs()[1].phase, PbricPhase::Believe);
        assert_eq!(pbric.phase_outputs()[2].phase, PbricPhase::Reason);
        assert_eq!(pbric.phase_outputs()[3].phase, PbricPhase::Intend);
    }

    #[tokio::test]
    async fn run_cycle_updates_belief() {
        let mut pbric = PbricLoop::new();
        pbric.run_cycle("什么是 Rust？").await.unwrap();

        // 信念模块应该有偏好更新
        let state = pbric.belief().get_state();
        assert!(!state.preferences.is_empty());
    }

    #[tokio::test]
    async fn run_cycle_logs_thoughts() {
        let mut pbric = PbricLoop::new().with_session("test_session");
        pbric.run_cycle("帮我写代码").await.unwrap();

        // 思维日志应该记录了所有阶段
        let logger = pbric.thought_logger();
        assert!(logger.len() >= 5);
        assert_eq!(logger.chain_count(), 1);
    }

    #[tokio::test]
    async fn run_cycle_includes_action_in_commit() {
        let mut pbric = PbricLoop::new();
        let result = pbric.run_cycle("帮我执行测试").await.unwrap();

        // commit 结果应包含 action
        assert!(result.contains("action:"));
    }

    #[tokio::test]
    async fn multiple_cycles_accumulate_beliefs() {
        let mut pbric = PbricLoop::new();
        pbric.run_cycle("什么是 Rust？").await.unwrap();
        pbric.run_cycle("帮我写一首诗").await.unwrap();
        pbric.run_cycle("这个错误怎么解决？").await.unwrap();

        let state = pbric.belief().get_state();
        // 多次循环应该积累多个偏好
        assert!(state.preferences.len() >= 2);
    }

    #[tokio::test]
    async fn phase_confidence_propagates() {
        let mut pbric = PbricLoop::new();
        pbric.run_cycle("帮我运行测试").await.unwrap();

        let outputs = pbric.phase_outputs();
        // Perceive 置信度最高
        assert!(outputs[0].confidence >= 0.9);
        // 所有阶段置信度都在合理范围
        for out in outputs {
            assert!(out.confidence >= 0.0 && out.confidence <= 1.0);
        }
    }

    #[test]
    fn test_with_session() {
        let pbric = PbricLoop::new().with_session("my_session");
        assert_eq!(pbric.session_id, "my_session");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world!", 5), "hello...");
    }
}
