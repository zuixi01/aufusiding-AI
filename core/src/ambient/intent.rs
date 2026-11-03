//! 意图分发器
//!
//! 将 STT 识别文本路由到不同的执行后端：
//! ChatOrchestrator / AgentOrchestrator / IoT / Plugin / Workflow
//!
//! IoT 分支接入语义设备理解管道：parse → resolve → map → execute

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::iot::control::GenericController;
use crate::iot::semantic::command_mapper::DeviceCommandMapper;
use crate::iot::semantic::intent_parser::{DeviceIntent, DeviceIntentParser};
use crate::iot::semantic::profile::SemanticProfileStore;
use crate::iot::semantic::resolver::DeviceResolver;
use crate::providers::traits::{ChatMessage, ChatRequest, Provider};
use crate::security::PromptGuard;

/// 分发目标
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchTarget {
    /// 通用聊天（闲聊、问答）
    Chat,
    /// 复杂多步任务（调研、分析）
    Agent,
    /// IoT 设备控制
    Iot,
    /// 插件能力（浏览器、导航等）
    Plugin,
    /// 工作流触发
    Workflow,
    /// 系统命令（音量、设置等）
    System,
}

/// 分发结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchResult {
    /// 分发目标
    pub target: DispatchTarget,
    /// 执行结果文本
    pub response: String,
    /// 是否为动作执行结果（vs 纯文本回答）
    pub is_action: bool,
    /// 是否需要后续交互
    pub needs_followup: bool,
}

/// 意图分发器 — 将识别文本路由到合适的执行后端
///
/// IoT 分支使用语义管道：DeviceIntentParser → DeviceResolver → DeviceCommandMapper → GenericController
pub struct IntentDispatcher {
    /// IoT 关键词
    iot_keywords: Vec<String>,
    /// 系统命令关键词
    system_keywords: Vec<String>,
    /// 工作流触发关键词
    workflow_keywords: Vec<String>,
    /// 语义解析器
    intent_parser: DeviceIntentParser,
    /// 语义档案库
    profile_store: Arc<SemanticProfileStore>,
    /// 设备控制器（可选，无控制器时仅返回解析结果）
    controller: Option<Arc<GenericController>>,
    /// 当前房间上下文
    current_room: Option<String>,
    /// LLM Provider（可选，用于 Chat/Agent 分支）
    provider: Option<Arc<dyn Provider>>,
    /// 系统提示词
    system_prompt: Option<String>,
    /// LLM 调用超时（毫秒）
    llm_timeout_ms: u64,
    /// Prompt 注入防护
    prompt_guard: Option<PromptGuard>,
}

impl IntentDispatcher {
    pub fn new() -> Self {
        Self {
            iot_keywords: vec![
                "打开".into(),
                "关闭".into(),
                "开灯".into(),
                "关灯".into(),
                "调高".into(),
                "调低".into(),
                "温度".into(),
                "亮度".into(),
                "空调".into(),
                "窗帘".into(),
                "电视".into(),
                "音箱".into(),
                "投屏".into(),
                "灯".into(),
                "风扇".into(),
                "加湿器".into(),
            ],
            system_keywords: vec![
                "音量".into(),
                "静音".into(),
                "设置".into(),
                "配置".into(),
                "连接".into(),
                "断开".into(),
                "重启".into(),
                "关机".into(),
            ],
            workflow_keywords: vec![
                "每天".into(),
                "定时".into(),
                "自动".into(),
                "提醒我".into(),
                "闹钟".into(),
                "日程".into(),
            ],
            intent_parser: DeviceIntentParser::new(),
            profile_store: Arc::new(SemanticProfileStore::new()),
            controller: None,
            current_room: None,
            provider: None,
            system_prompt: None,
            llm_timeout_ms: 10000,
            prompt_guard: None,
        }
    }

    /// 创建带语义设备支持的分发器
    pub fn with_semantic(
        profile_store: Arc<SemanticProfileStore>,
        controller: Arc<GenericController>,
    ) -> Self {
        let mut dispatcher = Self::new();
        dispatcher.profile_store = profile_store;
        dispatcher.controller = Some(controller);
        dispatcher
    }

    /// 设置 LLM Provider
    pub fn with_provider(mut self, provider: Arc<dyn Provider>) -> Self {
        self.provider = Some(provider);
        self
    }

    /// 设置系统提示词
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    /// 设置 LLM 超时
    pub fn with_llm_timeout(mut self, timeout_ms: u64) -> Self {
        self.llm_timeout_ms = timeout_ms;
        self
    }

    pub fn with_prompt_guard(mut self, guard: PromptGuard) -> Self {
        self.prompt_guard = Some(guard);
        self
    }

    /// 设置当前房间上下文
    pub fn set_current_room(&mut self, room: Option<String>) {
        self.current_room = room;
    }

    /// 分类用户意图
    pub fn classify(&self, text: &str) -> DispatchTarget {
        let lower = text.to_lowercase();

        // 系统命令优先
        if self
            .system_keywords
            .iter()
            .any(|kw| lower.contains(kw.as_str()))
        {
            return DispatchTarget::System;
        }

        // IoT 设备控制 — 同时检查关键词和语义解析
        if self
            .iot_keywords
            .iter()
            .any(|kw| lower.contains(kw.as_str()))
        {
            return DispatchTarget::Iot;
        }
        if self.intent_parser.parse(text).is_some() {
            return DispatchTarget::Iot;
        }

        // 工作流触发
        if self
            .workflow_keywords
            .iter()
            .any(|kw| lower.contains(kw.as_str()))
        {
            return DispatchTarget::Workflow;
        }

        // 复杂任务检测
        let agent_indicators = ["调研", "分析", "帮我查", "搜索", "对比", "总结"];
        if agent_indicators.iter().any(|kw| lower.contains(kw)) {
            return DispatchTarget::Agent;
        }

        DispatchTarget::Chat
    }

    /// 尝试语义解析设备意图
    pub fn parse_device_intent(&self, text: &str) -> Option<DeviceIntent> {
        self.intent_parser.parse(text)
    }

    /// 调用 LLM Provider
    async fn call_llm(&self, text: &str, is_agent: bool) -> Option<String> {
        let provider = self.provider.as_ref()?;
        let mut messages = Vec::new();

        if let Some(sys) = &self.system_prompt {
            messages.push(ChatMessage {
                role: "system".into(),
                content: sys.clone(),
            });
        }

        if is_agent {
            messages.push(ChatMessage {
                role: "system".into(),
                content: "你是一个任务执行助手，请分步骤完成用户的复杂任务。".into(),
            });
        }

        messages.push(ChatMessage {
            role: "user".into(),
            content: text.into(),
        });

        let model = provider.available_models().into_iter().next()?;
        let request = ChatRequest {
            model,
            messages,
            max_tokens: Some(512),
            temperature: Some(0.7),
            stream: false,
        };

        match tokio::time::timeout(
            Duration::from_millis(self.llm_timeout_ms),
            provider.chat(&request),
        )
        .await
        {
            Ok(Ok(response)) => Some(response.content),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "LLM 调用失败");
                None
            }
            Err(_) => {
                tracing::warn!("LLM 调用超时 ({}ms)", self.llm_timeout_ms);
                None
            }
        }
    }

    /// 执行语义设备控制管道：parse → resolve → map → execute
    async fn execute_semantic_iot(&self, text: &str) -> DispatchResult {
        // 1. 解析意图
        let intent = match self.intent_parser.parse(text) {
            Some(i) => i,
            None => {
                return DispatchResult {
                    target: DispatchTarget::Iot,
                    response: format!("理解了您想控制设备，但无法解析具体指令：{}", text),
                    is_action: false,
                    needs_followup: true,
                }
            }
        };

        // 2. 解析设备
        let resolver = DeviceResolver::new(&self.profile_store);
        let resolved = match resolver.resolve(
            &intent.device_ref,
            self.current_room.as_deref(),
            Some(&intent.action),
        ) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult {
                    target: DispatchTarget::Iot,
                    response: format!("找不到匹配的设备：{}", e),
                    is_action: false,
                    needs_followup: true,
                }
            }
        };

        // 3. 映射命令
        let current_state = if let Some(ctrl) = &self.controller {
            ctrl.execute(&resolved.device_id, "get_state", serde_json::json!({}))
                .await
                .ok()
        } else {
            None
        };

        let actions = match DeviceCommandMapper::map(
            &resolved.device_id,
            &resolved.profile.device_type,
            &intent.action,
            &intent.parameters,
            &resolved.profile.affordances,
            current_state.as_ref(),
        ) {
            Ok(a) => a,
            Err(e) => {
                return DispatchResult {
                    target: DispatchTarget::Iot,
                    response: format!("无法生成设备命令：{}", e),
                    is_action: false,
                    needs_followup: false,
                }
            }
        };

        // 4. 执行命令
        if let Some(ctrl) = &self.controller {
            let mut results = Vec::new();
            for action in &actions {
                match ctrl
                    .execute(&action.device_id, &action.command, action.params.clone())
                    .await
                {
                    Ok(_) => {
                        results.push(format!("{}：已执行 {}", action.device_id, action.command))
                    }
                    Err(e) => results.push(format!("{}：执行失败 - {}", action.device_id, e)),
                }
            }
            let desc = DeviceCommandMapper::describe_action(
                &resolved.profile.friendly_name,
                &intent.action,
                &intent.parameters,
            );
            DispatchResult {
                target: DispatchTarget::Iot,
                response: desc,
                is_action: true,
                needs_followup: false,
            }
        } else {
            // 无控制器，返回解析结果
            let desc = DeviceCommandMapper::describe_action(
                &resolved.profile.friendly_name,
                &intent.action,
                &intent.parameters,
            );
            DispatchResult {
                target: DispatchTarget::Iot,
                response: format!("已解析：{}（未连接控制器）", desc),
                is_action: false,
                needs_followup: false,
            }
        }
    }

    /// 分发并执行
    pub async fn dispatch(&self, text: &str) -> DispatchResult {
        // Prompt 注入防护
        if let Some(ref guard) = self.prompt_guard {
            if let Err(e) = guard.sanitize(text) {
                return DispatchResult {
                    target: DispatchTarget::System,
                    response: format!("输入被安全策略拦截: {}", e),
                    is_action: false,
                    needs_followup: false,
                };
            }
        }

        let target = self.classify(text);
        let is_action = matches!(
            target,
            DispatchTarget::Iot | DispatchTarget::System | DispatchTarget::Workflow
        );

        // IoT 走语义管道
        if target == DispatchTarget::Iot {
            return self.execute_semantic_iot(text).await;
        }

        // 其他目标：Chat/Agent 尝试调用 LLM
        let response = match &target {
            DispatchTarget::Chat => match self.call_llm(text, false).await {
                Some(reply) => reply,
                None => format!("收到您的问题：{}", text),
            },
            DispatchTarget::Agent => match self.call_llm(text, true).await {
                Some(reply) => reply,
                None => format!("正在处理复杂任务：{}", text),
            },
            DispatchTarget::Plugin => format!("正在调用插件：{}", text),
            DispatchTarget::Workflow => format!("正在设置工作流：{}", text),
            DispatchTarget::System => format!("正在执行系统命令：{}", text),
            DispatchTarget::Iot => format!("正在执行 IoT 命令：{}", text),
        };

        DispatchResult {
            target,
            response,
            is_action,
            needs_followup: false,
        }
    }
}
