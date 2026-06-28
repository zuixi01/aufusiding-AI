/// 反思与意图模块
///
/// 实现 Daemon Mind 的反思层，负责：
/// - 对话质量评估
/// - 策略调整建议
/// - 意图识别与追踪
///
/// 支持 LLM 驱动的反思和基于规则的 fallback。
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// 反思结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reflection {
    /// 对话质量评分 [0.0, 1.0]
    pub quality_score: f64,
    /// 识别的问题
    pub issues: Vec<String>,
    /// 改进建议
    pub suggestions: Vec<String>,
    /// 策略调整
    pub strategy_adjustments: HashMap<String, f64>,
}

/// 意图类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IntentType {
    /// 信息查询
    InformationSeeking,
    /// 任务执行
    TaskExecution,
    /// 创意生成
    CreativeGeneration,
    /// 问题解决
    ProblemSolving,
    /// 闲聊
    Casual,
    /// 未知
    Unknown,
}

/// 用户意图
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIntent {
    /// 意图类型
    pub intent_type: IntentType,
    /// 置信度 [0.0, 1.0]
    pub confidence: f64,
    /// 关键实体
    pub entities: Vec<String>,
    /// 隐含目标
    pub implicit_goals: Vec<String>,
}

/// 反思与意图模块
pub struct ReflectionModule {
    /// 历史反思记录
    reflection_history: Vec<Reflection>,
    /// 当前意图
    current_intent: Option<UserIntent>,
    /// 质量阈值
    quality_threshold: f64,
    /// LLM Provider（可选，用于 LLM 驱动反思）
    llm_provider: Option<Arc<dyn crate::providers::traits::Provider>>,
}

impl ReflectionModule {
    /// 创建新的反思模块
    pub fn new() -> Self {
        Self {
            reflection_history: Vec::new(),
            current_intent: None,
            quality_threshold: 0.6,
            llm_provider: None,
        }
    }

    /// 设置 LLM Provider（启用 LLM 驱动反思）
    pub fn with_llm_provider(
        mut self,
        provider: Arc<dyn crate::providers::traits::Provider>,
    ) -> Self {
        self.llm_provider = Some(provider);
        self
    }

    /// 反思对话质量
    pub async fn reflect_on_conversation(
        &mut self,
        messages: &[String],
        user_satisfaction: Option<f64>,
    ) -> Reflection {
        // 优先使用 LLM 反思
        if let Some(provider) = &self.llm_provider {
            match self
                .reflect_with_llm(provider, messages, user_satisfaction)
                .await
            {
                Ok(reflection) => {
                    self.reflection_history.push(reflection.clone());
                    return reflection;
                }
                Err(e) => {
                    tracing::warn!("LLM 反思失败，fallback 到规则反思: {}", e);
                }
            }
        }

        // Fallback: 基于规则的反思
        self.reflect_with_rules(messages, user_satisfaction)
    }

    /// LLM 驱动的对话反思
    async fn reflect_with_llm(
        &self,
        provider: &Arc<dyn crate::providers::traits::Provider>,
        messages: &[String],
        user_satisfaction: Option<f64>,
    ) -> anyhow::Result<Reflection> {
        use crate::providers::traits::{ChatMessage, ChatRequest};

        let conversation = messages.join("\n");
        let sat_info = user_satisfaction
            .map(|s| format!("用户满意度: {:.2}", s))
            .unwrap_or_else(|| "用户满意度: 未知".to_string());

        let prompt = format!(
            r#"分析以下对话的质量，提供反思和改进建议。

对话内容:
{}

{}

请以 JSON 格式返回分析结果，包含：
1. quality_score: 质量评分 (0.0-1.0)
2. issues: 识别的问题列表
3. suggestions: 改进建议列表

示例格式:
{{
  "quality_score": 0.75,
  "issues": ["回答过于简短", "缺少具体例子"],
  "suggestions": ["提供更详细的解释", "增加实际案例"]
}}

只返回 JSON，不要有其他内容。"#,
            conversation, sat_info
        );

        let request = ChatRequest {
            model: String::new(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            max_tokens: Some(300),
            temperature: Some(0.3),
            stream: false,
        };

        let response = provider.chat(&request).await?;

        // 解析 JSON 响应
        #[derive(Deserialize)]
        struct LlmReflection {
            quality_score: f64,
            issues: Vec<String>,
            suggestions: Vec<String>,
        }

        let content = response.content.trim();

        // 尝试直接解析
        let parsed: LlmReflection = match serde_json::from_str(content) {
            Ok(p) => p,
            Err(_) => {
                // 尝试提取 JSON（可能有额外文本）
                if let Some(start) = content.find('{') {
                    if let Some(end) = content.rfind('}') {
                        serde_json::from_str(&content[start..=end])?
                    } else {
                        anyhow::bail!("No closing brace in LLM response")
                    }
                } else {
                    anyhow::bail!("No JSON found in LLM response")
                }
            }
        };

        let mut strategy_adjustments = HashMap::new();
        if parsed.quality_score < self.quality_threshold {
            strategy_adjustments.insert("creativity".to_string(), 0.2);
            strategy_adjustments.insert("verbosity".to_string(), -0.1);
        }

        Ok(Reflection {
            quality_score: parsed.quality_score.clamp(0.0, 1.0),
            issues: parsed.issues,
            suggestions: parsed.suggestions,
            strategy_adjustments,
        })
    }

    /// 基于规则的对话反思（fallback）
    fn reflect_with_rules(
        &mut self,
        messages: &[String],
        user_satisfaction: Option<f64>,
    ) -> Reflection {
        let mut issues = Vec::new();
        let mut suggestions = Vec::new();
        let mut strategy_adjustments = HashMap::new();

        // 1. 评估对话长度
        let msg_count = messages.len();
        let length_score = if msg_count < 3 {
            issues.push("对话过短，可能未充分理解用户需求".to_string());
            suggestions.push("尝试通过追问获取更多上下文".to_string());
            0.3
        } else if msg_count > 20 {
            issues.push("对话过长，可能效率不高".to_string());
            suggestions.push("考虑更直接的回答方式".to_string());
            0.6
        } else {
            0.7
        };

        // 2. 评估响应质量（基于用户满意度）
        let satisfaction_score = user_satisfaction.unwrap_or(0.7);

        // 3. 检测重复模式
        let repetition_score = self.detect_repetition(messages);
        if repetition_score < 0.7 {
            issues.push("检测到重复回答模式".to_string());
            suggestions.push("尝试不同的表达方式或策略".to_string());
        }

        // 4. 综合质量评分
        let quality_score =
            (length_score * 0.3 + satisfaction_score * 0.5 + repetition_score * 0.2)
                .clamp(0.0, 1.0);

        // 5. 生成策略调整建议
        if quality_score < self.quality_threshold {
            strategy_adjustments.insert("creativity".to_string(), 0.2);
            strategy_adjustments.insert("verbosity".to_string(), -0.1);
        }

        // 如果用户满意度低，降低质量分数
        if let Some(sat) = user_satisfaction {
            if sat < 0.5 {
                issues.push("用户满意度较低".to_string());
                suggestions.push("重新评估回答策略".to_string());
            }
        }

        let reflection = Reflection {
            quality_score,
            issues,
            suggestions,
            strategy_adjustments,
        };

        self.reflection_history.push(reflection.clone());
        reflection
    }

    /// 识别用户意图
    pub fn identify_intent(&mut self, user_message: &str) -> UserIntent {
        let message_lower = user_message.to_lowercase();

        // 简单的关键词匹配（实际应使用 NLP 模型）
        // 优先级：创意 > 任务 > 问题 > 信息 > 闲聊
        let (intent_type, confidence) = if message_lower.contains("创作")
            || message_lower.contains("生成")
            || message_lower.contains("写一")
            || message_lower.contains("诗")
            || message_lower.contains("故事")
        {
            (IntentType::CreativeGeneration, 0.75)
        } else if message_lower.contains("帮我")
            || message_lower.contains("执行")
            || message_lower.contains("运行")
        {
            (IntentType::TaskExecution, 0.85)
        } else if message_lower.contains("问题")
            || message_lower.contains("错误")
            || message_lower.contains("解决")
        {
            (IntentType::ProblemSolving, 0.8)
        } else if message_lower.contains("什么")
            || message_lower.contains("为什么")
            || message_lower.contains("如何")
            || message_lower.contains("?")
            || message_lower.contains("？")
        {
            (IntentType::InformationSeeking, 0.8)
        } else if message_lower.len() < 20 {
            (IntentType::Casual, 0.6)
        } else {
            (IntentType::Unknown, 0.3)
        };

        // 提取实体（简化版）
        let entities = self.extract_entities(user_message);

        // 推断隐含目标
        let implicit_goals = self.infer_implicit_goals(&intent_type, &entities);

        let intent = UserIntent {
            intent_type,
            confidence,
            entities,
            implicit_goals,
        };

        self.current_intent = Some(intent.clone());
        intent
    }

    /// 检测重复模式
    fn detect_repetition(&self, messages: &[String]) -> f64 {
        if messages.len() < 3 {
            return 1.0;
        }

        let mut unique_patterns = std::collections::HashSet::new();
        for msg in messages {
            // 简化：使用前20个字符作为模式
            let pattern = msg.chars().take(20).collect::<String>();
            unique_patterns.insert(pattern);
        }

        let uniqueness = unique_patterns.len() as f64 / messages.len() as f64;
        uniqueness.clamp(0.0, 1.0)
    }

    /// 提取实体（简化版）
    fn extract_entities(&self, text: &str) -> Vec<String> {
        // 简化实现：提取大写开头的词和数字
        text.split_whitespace()
            .filter(|word| {
                word.chars().next().is_some_and(|c| c.is_uppercase())
                    || word.chars().all(|c| c.is_numeric())
            })
            .map(|s| s.to_string())
            .collect()
    }

    /// 推断隐含目标
    fn infer_implicit_goals(&self, intent_type: &IntentType, entities: &[String]) -> Vec<String> {
        let mut goals = Vec::new();

        match intent_type {
            IntentType::InformationSeeking => {
                goals.push("获取准确信息".to_string());
                if !entities.is_empty() {
                    goals.push(format!("了解 {} 的详细信息", entities[0]));
                }
            }
            IntentType::TaskExecution => {
                goals.push("高效完成任务".to_string());
                goals.push("确保结果正确".to_string());
            }
            IntentType::CreativeGeneration => {
                goals.push("生成高质量内容".to_string());
                goals.push("保持创意性".to_string());
            }
            IntentType::ProblemSolving => {
                goals.push("找到问题根源".to_string());
                goals.push("提供可行解决方案".to_string());
            }
            IntentType::Casual => {
                goals.push("保持友好对话".to_string());
            }
            IntentType::Unknown => {
                goals.push("澄清用户意图".to_string());
            }
        }

        goals
    }

    /// 获取当前意图
    pub fn get_current_intent(&self) -> Option<&UserIntent> {
        self.current_intent.as_ref()
    }

    /// 获取反思历史
    pub fn get_reflection_history(&self) -> &[Reflection] {
        &self.reflection_history
    }

    /// 获取平均质量分数
    pub fn get_average_quality(&self) -> f64 {
        if self.reflection_history.is_empty() {
            return 0.0;
        }

        let sum: f64 = self
            .reflection_history
            .iter()
            .map(|r| r.quality_score)
            .sum();
        sum / self.reflection_history.len() as f64
    }

    /// 清空历史
    pub fn clear_history(&mut self) {
        self.reflection_history.clear();
        self.current_intent = None;
    }
}

impl Default for ReflectionModule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_reflection_module() {
        let module = ReflectionModule::new();
        assert_eq!(module.reflection_history.len(), 0);
        assert!(module.current_intent.is_none());
    }

    #[tokio::test]
    async fn test_reflect_on_short_conversation() {
        let mut module = ReflectionModule::new();
        let messages = vec!["Hello".to_string(), "Hi".to_string()];

        let reflection = module.reflect_on_conversation(&messages, None).await;

        assert!(reflection.quality_score < 0.7);
        assert!(!reflection.issues.is_empty());
        assert!(!reflection.suggestions.is_empty());
    }

    #[tokio::test]
    async fn test_reflect_on_good_conversation() {
        let mut module = ReflectionModule::new();
        let messages = vec![
            "用户问题".to_string(),
            "助手回答1".to_string(),
            "用户追问".to_string(),
            "助手回答2".to_string(),
            "用户满意".to_string(),
        ];

        let reflection = module.reflect_on_conversation(&messages, Some(0.9)).await;

        assert!(reflection.quality_score > 0.7);
    }

    #[test]
    fn test_identify_information_seeking_intent() {
        let mut module = ReflectionModule::new();
        let intent = module.identify_intent("什么是 Rust 语言？");

        assert_eq!(intent.intent_type, IntentType::InformationSeeking);
        assert!(intent.confidence > 0.7);
    }

    #[test]
    fn test_identify_task_execution_intent() {
        let mut module = ReflectionModule::new();
        let intent = module.identify_intent("帮我运行测试");

        assert_eq!(intent.intent_type, IntentType::TaskExecution);
        assert!(intent.confidence > 0.7);
    }

    #[test]
    fn test_identify_creative_intent() {
        let mut module = ReflectionModule::new();
        let intent = module.identify_intent("帮我写一首诗");

        assert_eq!(intent.intent_type, IntentType::CreativeGeneration);
        assert!(intent.confidence > 0.7);
    }

    #[test]
    fn test_identify_problem_solving_intent() {
        let mut module = ReflectionModule::new();
        let intent = module.identify_intent("这个错误怎么解决？");

        assert_eq!(intent.intent_type, IntentType::ProblemSolving);
        assert!(intent.confidence > 0.7);
    }

    #[test]
    fn test_detect_repetition() {
        let module = ReflectionModule::new();
        let messages = vec![
            "Hello world".to_string(),
            "Hello world".to_string(),
            "Hello world".to_string(),
        ];

        let score = module.detect_repetition(&messages);
        assert!(score < 0.5); // 高重复度
    }

    #[test]
    fn test_extract_entities() {
        let module = ReflectionModule::new();
        let entities = module.extract_entities("Rust 语言和 Python 123");

        assert!(entities.contains(&"Rust".to_string()));
        assert!(entities.contains(&"Python".to_string()));
        assert!(entities.contains(&"123".to_string()));
    }

    #[test]
    fn test_infer_implicit_goals() {
        let module = ReflectionModule::new();
        let entities = vec!["Rust".to_string()];
        let goals = module.infer_implicit_goals(&IntentType::InformationSeeking, &entities);

        assert!(!goals.is_empty());
        assert!(goals.iter().any(|g| g.contains("Rust")));
    }

    #[tokio::test]
    async fn test_get_average_quality() {
        let mut module = ReflectionModule::new();

        let messages1 = vec!["msg1".to_string(), "msg2".to_string(), "msg3".to_string()];
        module.reflect_on_conversation(&messages1, Some(0.8)).await;

        let messages2 = vec!["msg4".to_string(), "msg5".to_string(), "msg6".to_string()];
        module.reflect_on_conversation(&messages2, Some(0.6)).await;

        let avg = module.get_average_quality();
        assert!(avg > 0.6 && avg < 0.8);
    }

    #[tokio::test]
    async fn test_clear_history() {
        let mut module = ReflectionModule::new();
        module.identify_intent("测试消息");

        let messages = vec!["msg1".to_string()];
        module.reflect_on_conversation(&messages, None).await;

        module.clear_history();

        assert_eq!(module.reflection_history.len(), 0);
        assert!(module.current_intent.is_none());
    }

    // --- LLM 驱动反思测试 ---

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_llm_reflection() {
        use crate::providers::ollama::OllamaProvider;
        use std::sync::Arc;

        let provider = Arc::new(OllamaProvider::new());
        let mut module = ReflectionModule::new().with_llm_provider(provider);

        let messages = vec![
            "用户: 你好".to_string(),
            "助手: 你好！有什么可以帮助你的吗？".to_string(),
            "用户: 介绍一下 Rust".to_string(),
            "助手: Rust 是一门系统编程语言".to_string(),
        ];

        let reflection = module.reflect_on_conversation(&messages, Some(0.8)).await;

        println!("LLM 反思结果:");
        println!("  质量评分: {}", reflection.quality_score);
        println!("  问题: {:?}", reflection.issues);
        println!("  建议: {:?}", reflection.suggestions);

        assert!(reflection.quality_score >= 0.0 && reflection.quality_score <= 1.0);
        assert_eq!(module.reflection_history.len(), 1);
    }

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_llm_reflection_low_quality() {
        use crate::providers::ollama::OllamaProvider;
        use std::sync::Arc;

        let provider = Arc::new(OllamaProvider::new());
        let mut module = ReflectionModule::new().with_llm_provider(provider);

        let messages = vec![
            "用户: 帮我解决这个问题".to_string(),
            "助手: 好的".to_string(),
        ];

        let reflection = module.reflect_on_conversation(&messages, Some(0.3)).await;

        println!("低质量对话反思:");
        println!("  质量评分: {}", reflection.quality_score);
        println!("  问题: {:?}", reflection.issues);

        assert!(reflection.quality_score < 0.7);
        assert!(!reflection.issues.is_empty() || !reflection.suggestions.is_empty());
    }

    #[tokio::test]
    async fn test_fallback_to_rules_when_no_llm() {
        // 没有 LLM provider，应该 fallback 到规则反思
        let mut module = ReflectionModule::new();

        let messages = vec!["msg1".to_string(), "msg2".to_string(), "msg3".to_string()];

        let reflection = module.reflect_on_conversation(&messages, Some(0.7)).await;

        assert!(reflection.quality_score >= 0.0 && reflection.quality_score <= 1.0);
        assert_eq!(module.reflection_history.len(), 1);
    }
}
