//! Agent 执行引擎 — ReAct 循环（Reasoning + Acting）
//!
//! 执行流程：思考 → 选择工具 → 执行 → 观察 → 重复，直到产出交付物。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::tools::registry::{ToolRegistry, ToolResult};

use super::context::AgentContext;
use super::model_pool::{ChatMessage, ModelBackend};

/// 执行步骤记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub step_number: u32,
    pub thought: String,
    pub action: Option<ToolAction>,
    pub observation: Option<String>,
    /// 使用的模型层级（级联路由记录）
    pub model_tier: Option<String>,
}

/// 工具调用动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAction {
    pub tool_name: String,
    pub parameters: Value,
    pub result: Option<ToolResult>,
}

/// 执行结果
#[derive(Debug, Clone)]
pub enum ExecutionOutcome {
    /// 成功产出交付物
    Completed {
        output: String,
        steps: Vec<ExecutionStep>,
    },
    /// 达到最大步数限制
    MaxStepsReached { steps: Vec<ExecutionStep> },
    /// 执行出错
    Failed {
        error: String,
        steps: Vec<ExecutionStep>,
    },
}

/// 执行配置
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub max_steps: u32,
    pub max_retries_per_tool: u32,
}

impl ExecutorConfig {
    pub fn default_config() -> Self {
        Self {
            max_steps: 20,
            max_retries_per_tool: 3,
        }
    }
}

/// Agent 执行引擎 — 驱动 ReAct 循环
pub struct AgentExecutor {
    config: ExecutorConfig,
}

impl AgentExecutor {
    pub fn new(config: ExecutorConfig) -> Self {
        Self { config }
    }

    /// 执行 ReAct 循环
    ///
    /// 1. 将上下文发送给 LLM
    /// 2. 解析 LLM 回复：是最终答案还是工具调用
    /// 3. 如果是工具调用，执行工具并将结果加入上下文
    /// 4. 重复直到得到最终答案或达到步数限制
    pub async fn execute(
        &self,
        context: &mut AgentContext,
        model: &dyn ModelBackend,
        tools: &ToolRegistry,
    ) -> ExecutionOutcome {
        let mut steps = Vec::new();
        let mut termination_detector = super::termination::TerminationDetector::new();
        let mut force_final_answer = false;

        for step_num in 1..=self.config.max_steps {
            // 每 3 步压缩一次上下文
            if step_num > 1 && step_num % 3 == 1 {
                context.compress();
            }

            // 如果上一步检测到应终止，注入强制回答 prompt
            if force_final_answer {
                context.push_task(&super::termination::TerminationDetector::force_answer_prompt());
            }

            // 1. 调用 LLM
            let messages = self.build_chat_messages(context, tools);
            let response = match model.chat(&messages).await {
                Ok(r) => r,
                Err(e) => {
                    return ExecutionOutcome::Failed {
                        error: format!("model error: {}", e),
                        steps,
                    };
                }
            };

            // 2. 解析回复
            let parsed = Self::parse_response(&response);
            context.push_response(&response);

            // 如果上一步要求强制回答，将任何回复视为 FinalAnswer
            if force_final_answer {
                let answer = match parsed {
                    ParsedResponse::FinalAnswer(a) => a,
                    ParsedResponse::ToolCall { thought, .. } => thought,
                };
                steps.push(ExecutionStep {
                    step_number: step_num,
                    thought: answer.clone(),
                    action: None,
                    observation: None,
                    model_tier: None,
                });
                return ExecutionOutcome::Completed {
                    output: answer,
                    steps,
                };
            }

            match parsed {
                ParsedResponse::FinalAnswer(answer) => {
                    steps.push(ExecutionStep {
                        step_number: step_num,
                        thought: answer.clone(),
                        action: None,
                        observation: None,
                        model_tier: None,
                    });
                    return ExecutionOutcome::Completed {
                        output: answer,
                        steps,
                    };
                }
                ParsedResponse::ToolCall {
                    thought,
                    tool_name,
                    parameters,
                } => {
                    // 提前终止检测
                    if termination_detector.should_terminate(&thought) {
                        force_final_answer = true;
                    }

                    // 3. 执行工具
                    let result = tools.execute(&tool_name, parameters.clone()).await;
                    let observation = if result.success {
                        serde_json::to_string(&result.output).unwrap_or_default()
                    } else {
                        result.error.clone().unwrap_or_default()
                    };

                    context.push_tool_result(
                        &format!("step-{}", step_num),
                        &serde_json::to_value(&result).unwrap_or(Value::Null),
                    );

                    steps.push(ExecutionStep {
                        step_number: step_num,
                        thought: thought.clone(),
                        action: Some(ToolAction {
                            tool_name,
                            parameters,
                            result: Some(result),
                        }),
                        observation: Some(observation),
                        model_tier: None,
                    });
                }
            }
        }

        ExecutionOutcome::MaxStepsReached { steps }
    }

    /// 构建发送给 LLM 的消息列表
    fn build_chat_messages(
        &self,
        context: &AgentContext,
        tools: &ToolRegistry,
    ) -> Vec<ChatMessage> {
        let mut messages: Vec<ChatMessage> = context
            .messages()
            .iter()
            .map(|m| ChatMessage {
                role: format!("{:?}", m.role).to_lowercase(),
                content: m.content.clone(),
            })
            .collect();

        // 在 system prompt 后注入可用工具信息
        if !tools.is_empty() {
            let tool_desc: Vec<String> = tools
                .descriptors()
                .iter()
                .map(|d| format!("- {}: {}", d.name, d.description))
                .collect();
            let tool_msg = format!(
                "可用工具:\n{}\n\n\
                 如果需要使用工具，回复格式:\n\
                 THOUGHT: <你的思考>\n\
                 ACTION: <tool_name>\n\
                 PARAMS: <json参数>\n\n\
                 如果已有最终答案，回复格式:\n\
                 ANSWER: <最终答案>",
                tool_desc.join("\n")
            );
            messages.insert(
                1,
                ChatMessage {
                    role: "system".into(),
                    content: tool_msg,
                },
            );
        }

        messages
    }

    /// 解析 LLM 回复
    fn parse_response(response: &str) -> ParsedResponse {
        // 检查是否包含 ACTION 标记
        if let Some(action_pos) = response.find("ACTION:") {
            let thought = response[..action_pos]
                .replace("THOUGHT:", "")
                .trim()
                .to_string();

            let after_action = &response[action_pos + 7..];
            let tool_name = after_action.lines().next().unwrap_or("").trim().to_string();

            let parameters = if let Some(params_pos) = response.find("PARAMS:") {
                let params_str = response[params_pos + 7..].trim();
                serde_json::from_str(params_str).unwrap_or(Value::Null)
            } else {
                Value::Null
            };

            ParsedResponse::ToolCall {
                thought,
                tool_name,
                parameters,
            }
        } else if let Some(answer_pos) = response.find("ANSWER:") {
            let answer = response[answer_pos + 7..].trim().to_string();
            ParsedResponse::FinalAnswer(answer)
        } else {
            // 没有标记，视为最终答案
            ParsedResponse::FinalAnswer(response.to_string())
        }
    }
}

enum ParsedResponse {
    FinalAnswer(String),
    ToolCall {
        thought: String,
        tool_name: String,
        parameters: Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::execution::model_pool::MockModelBackend;
    use crate::agent::tools::registry::MockTool;
    use std::sync::Arc;

    fn make_tools() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(MockTool::new(
            "search",
            serde_json::json!({"results": ["item1", "item2"]}),
        )));
        reg
    }

    #[tokio::test]
    async fn execute_direct_answer() {
        let executor = AgentExecutor::new(ExecutorConfig::default_config());
        let model = MockModelBackend::new("test", "gpt-4", "ANSWER: 42");
        let mut ctx = AgentContext::new("你是助手");
        ctx.push_task("1+1=?");
        let tools = ToolRegistry::new();

        let outcome = executor.execute(&mut ctx, &model, &tools).await;
        match outcome {
            ExecutionOutcome::Completed { output, steps } => {
                assert_eq!(output, "42");
                assert_eq!(steps.len(), 1);
            }
            _ => panic!("expected Completed"),
        }
    }

    #[tokio::test]
    async fn execute_with_tool_call() {
        // Model first returns a tool call, then a final answer
        let responses = Arc::new(std::sync::Mutex::new(vec![
            "ANSWER: 搜索结果是 item1".to_string(),
            "THOUGHT: 需要搜索\nACTION: search\nPARAMS: {}".to_string(),
        ]));

        struct SequentialMock {
            responses: Arc<std::sync::Mutex<Vec<String>>>,
        }

        #[async_trait::async_trait]
        impl ModelBackend for SequentialMock {
            fn provider_name(&self) -> &str {
                "mock"
            }
            fn model_name(&self) -> &str {
                "seq"
            }
            async fn chat(&self, _: &[ChatMessage]) -> anyhow::Result<String> {
                let mut r = self.responses.lock().unwrap();
                Ok(r.pop().unwrap_or_else(|| "ANSWER: done".into()))
            }
        }

        let model = SequentialMock { responses };
        let executor = AgentExecutor::new(ExecutorConfig::default_config());
        let mut ctx = AgentContext::new("你是助手");
        ctx.push_task("搜索 item1");
        let tools = make_tools();

        let outcome = executor.execute(&mut ctx, &model, &tools).await;
        match outcome {
            ExecutionOutcome::Completed { steps, .. } => {
                assert_eq!(steps.len(), 2);
                assert!(steps[0].action.is_some());
                assert!(steps[1].action.is_none());
            }
            _ => panic!("expected Completed"),
        }
    }

    #[tokio::test]
    async fn execute_max_steps_reached() {
        // Model always returns tool calls, never a final answer
        let model = MockModelBackend::new(
            "test",
            "gpt-4",
            "THOUGHT: thinking\nACTION: search\nPARAMS: {}",
        );
        let executor = AgentExecutor::new(ExecutorConfig {
            max_steps: 3,
            max_retries_per_tool: 1,
        });
        let mut ctx = AgentContext::new("你是助手");
        ctx.push_task("无限循环");
        let tools = make_tools();

        let outcome = executor.execute(&mut ctx, &model, &tools).await;
        match outcome {
            ExecutionOutcome::MaxStepsReached { steps } => {
                assert_eq!(steps.len(), 3);
            }
            _ => panic!("expected MaxStepsReached"),
        }
    }

    #[test]
    fn parse_response_final_answer() {
        let parsed = AgentExecutor::parse_response("ANSWER: hello world");
        match parsed {
            ParsedResponse::FinalAnswer(a) => assert_eq!(a, "hello world"),
            _ => panic!("expected FinalAnswer"),
        }
    }

    #[test]
    fn parse_response_tool_call() {
        let input = "THOUGHT: 需要搜索\nACTION: search\nPARAMS: {\"q\": \"test\"}";
        let parsed = AgentExecutor::parse_response(input);
        match parsed {
            ParsedResponse::ToolCall {
                thought,
                tool_name,
                parameters,
            } => {
                assert!(thought.contains("搜索"));
                assert_eq!(tool_name, "search");
                assert_eq!(parameters["q"], "test");
            }
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn parse_response_plain_text_is_answer() {
        let parsed = AgentExecutor::parse_response("just some text");
        match parsed {
            ParsedResponse::FinalAnswer(a) => assert_eq!(a, "just some text"),
            _ => panic!("expected FinalAnswer"),
        }
    }
}
