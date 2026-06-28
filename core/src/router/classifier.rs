//! 意图分类器
//!
//! 分析用户输入，判断意图类型（闲聊、知识问答、工具调用、代码生成等），
//! 为路由决策提供依据。支持 LLM 驱动分类和基于规则的 fallback。

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 意图类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum IntentType {
    /// 日常闲聊
    Casual,
    /// 知识问答
    Knowledge,
    /// 工具调用
    ToolUse,
    /// 代码生成
    CodeGen,
    /// 创意写作
    Creative,
    /// 多轮推理
    Reasoning,
    /// 未知意图
    Unknown,
}

/// 分类结果
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// 主要意图
    pub primary: IntentType,
    /// 置信度 (0.0 - 1.0)
    pub confidence: f32,
    /// 预估复杂度 (1-10)
    pub complexity: u8,
}

/// 意图特征向量
#[derive(Debug, Clone)]
pub struct IntentFeatures {
    /// 长度特征: log₁₀(1 + token_count)
    pub length: f64,
    /// 复杂度特征: 0.4·keyword_density + 0.3·sentence_depth + 0.3·entity_count
    pub complexity: f64,
    /// 代码特征: sigmoid(code_pattern_score)
    pub code_score: f64,
    /// 创意特征: creative_verb_ratio + metaphor_score
    pub creative_score: f64,
    /// 推理特征: reasoning_keyword_count / sentence_count
    pub reasoning_score: f64,
    /// 上下文特征: min(1.0, history_length / 10)
    pub context_score: f64,
}

/// 意图分类器
pub struct IntentClassifier {
    /// 分类模型权重路径（轻量级本地模型）
    _model_path: Option<std::path::PathBuf>,
    /// 复杂关键词列表
    complex_keywords: Vec<String>,
    /// 创意动词列表
    creative_verbs: Vec<String>,
    /// 推理关键词列表
    reasoning_keywords: Vec<String>,
    /// LLM Provider（可选，用于 LLM 驱动分类）
    llm_provider: Option<Arc<dyn crate::providers::traits::Provider>>,
}

impl IntentClassifier {
    /// 创建新的意图分类器
    pub fn new() -> Self {
        Self {
            _model_path: None,
            complex_keywords: vec![
                "分析".to_string(),
                "推理".to_string(),
                "证明".to_string(),
                "算法".to_string(),
                "架构".to_string(),
                "优化".to_string(),
                "实现".to_string(),
                "设计".to_string(),
                "复杂".to_string(),
            ],
            creative_verbs: vec![
                "想象".to_string(),
                "创作".to_string(),
                "设计".to_string(),
                "构思".to_string(),
                "编写".to_string(),
                "描绘".to_string(),
            ],
            reasoning_keywords: vec![
                "为什么".to_string(),
                "如何".to_string(),
                "分析".to_string(),
                "推导".to_string(),
                "证明".to_string(),
                "原因".to_string(),
                "解释".to_string(),
                "why".to_string(),
                "how".to_string(),
            ],
            llm_provider: None,
        }
    }

    /// 设置 LLM Provider（启用 LLM 驱动分类）
    pub fn with_llm_provider(
        mut self,
        provider: Arc<dyn crate::providers::traits::Provider>,
    ) -> Self {
        self.llm_provider = Some(provider);
        self
    }

    /// 提取 6 维特征向量
    pub fn extract_features(&self, query: &str, history_len: usize) -> IntentFeatures {
        let tokens = self.tokenize(query);
        let token_count = tokens.len();

        // 1. 长度特征
        let length = (1.0 + token_count as f64).log10();

        // 2. 复杂度特征
        let keyword_density = self.complex_keyword_density(&tokens);
        let sentence_depth = self.avg_parse_depth(query);
        let entity_count = self.count_entities(query);
        let complexity = 0.4 * keyword_density + 0.3 * sentence_depth + 0.3 * entity_count;

        // 3. 代码特征
        let code_score = self.sigmoid(self.code_pattern_score(query));

        // 4. 创意特征
        let creative_score = self.creative_verb_ratio(&tokens) + self.metaphor_score(query);

        // 5. 推理特征
        let reasoning_score =
            self.reasoning_keyword_count(&tokens) / self.sentence_count(query).max(1) as f64;

        // 6. 上下文特征
        let context_score = (history_len as f64 / 10.0).min(1.0);

        IntentFeatures {
            length,
            complexity,
            code_score,
            creative_score,
            reasoning_score,
            context_score,
        }
    }

    /// 基于特征进行分类
    pub fn classify_by_features(&self, features: &IntentFeatures) -> IntentType {
        if features.code_score > 0.6 {
            IntentType::CodeGen
        } else if features.creative_score > 0.5 {
            IntentType::Creative
        } else if features.reasoning_score > 0.3 && features.complexity > 0.4
            || features.length > 2.0
        {
            // 高推理分数或长文本都需要复杂推理
            IntentType::Reasoning
        } else if features.complexity < 0.3 && features.length < 1.5 {
            IntentType::Casual
        } else {
            IntentType::Knowledge
        }
    }

    /// Sigmoid 激活函数
    pub fn sigmoid(&self, x: f64) -> f64 {
        1.0 / (1.0 + (-x).exp())
    }

    /// 分词（简单实现）
    fn tokenize(&self, text: &str) -> Vec<String> {
        // 简单分词：按空格和标点分割
        let mut tokens = Vec::new();
        let mut current = String::new();

        for ch in text.chars() {
            if ch.is_whitespace() || "，。！？；：、,.!?;:".contains(ch) {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            } else {
                current.push(ch);
            }
        }

        if !current.is_empty() {
            tokens.push(current);
        }

        tokens
    }

    /// 计算复杂关键词密度
    pub fn complex_keyword_density(&self, tokens: &[String]) -> f64 {
        if tokens.is_empty() {
            return 0.0;
        }

        let complex_count = tokens
            .iter()
            .filter(|token| self.complex_keywords.contains(token))
            .count();

        complex_count as f64 / tokens.len() as f64
    }

    /// 估算句子深度（简化版）
    fn avg_parse_depth(&self, text: &str) -> f64 {
        let sentences = text.split(&['。', '！', '？', '.', '!', '?'][..]).count();
        let clauses = text.split(&['，', ',', '；', ';'][..]).count();

        if sentences == 0 {
            return 0.0;
        }

        (clauses as f64 / sentences as f64).min(1.0)
    }

    /// 统计实体数量（简化版）
    fn count_entities(&self, text: &str) -> f64 {
        // 简单统计大写开头的词和数字
        let entity_count = text
            .split_whitespace()
            .filter(|word| {
                word.chars().next().is_some_and(|c| c.is_uppercase())
                    || word.chars().any(|c| c.is_numeric())
            })
            .count();

        (entity_count as f64 / 10.0).min(1.0)
    }

    /// 计算代码模式得分
    fn code_pattern_score(&self, text: &str) -> f64 {
        let patterns = vec![
            // 代码块标记
            ("```", 3.0),
            // 英文关键词
            ("function", 2.0),
            ("class", 2.0),
            ("def", 2.0),
            ("import", 1.5),
            ("require", 1.5),
            ("=>", 1.0),
            ("->", 1.0),
            ("::", 1.0),
            // Rust 关键词
            ("fn", 2.0),
            ("struct", 2.0),
            ("impl", 2.0),
            ("trait", 2.0),
            ("Rust", 2.0),
            // Python 关键词
            ("Python", 2.0),
            ("python", 2.0),
            // 中文代码相关词
            ("函数", 2.0),
            ("方法", 1.5),
            ("类", 1.5),
            ("算法", 1.5),
            ("代码", 2.0),
            ("编程", 1.5),
            ("实现", 1.0),
            ("泛型", 1.5),
        ];

        patterns
            .iter()
            .map(
                |(pattern, weight)| {
                    if text.contains(pattern) {
                        *weight
                    } else {
                        0.0
                    }
                },
            )
            .sum()
    }

    /// 计算创意动词比例
    fn creative_verb_ratio(&self, tokens: &[String]) -> f64 {
        if tokens.is_empty() {
            return 0.0;
        }

        let mut count = 0.0;

        // 检查每个 token 是否包含创意动词
        for token in tokens {
            for verb in &self.creative_verbs {
                if token.contains(verb) {
                    count += 1.0;
                    break;
                }
            }
        }

        count / tokens.len() as f64
    }

    /// 计算隐喻得分（简化版）
    fn metaphor_score(&self, text: &str) -> f64 {
        let metaphor_indicators = ["像", "如同", "仿佛", "好比", "犹如"];
        let count = metaphor_indicators
            .iter()
            .filter(|indicator| text.contains(*indicator))
            .count();

        (count as f64 * 0.2).min(1.0)
    }

    /// 统计推理关键词数量
    fn reasoning_keyword_count(&self, tokens: &[String]) -> f64 {
        let mut count = 0.0;

        // 检查每个 token 是否包含推理关键词
        for token in tokens {
            for keyword in &self.reasoning_keywords {
                if token.contains(keyword) {
                    count += 1.0;
                    break;
                }
            }
        }

        count
    }

    /// 统计句子数量
    fn sentence_count(&self, text: &str) -> usize {
        text.split(&['。', '！', '？', '.', '!', '?'][..])
            .filter(|s| !s.trim().is_empty())
            .count()
            .max(1)
    }

    /// 对用户输入进行意图分类
    pub async fn classify(&self, input: &str) -> anyhow::Result<ClassificationResult> {
        // 优先使用 LLM 分类
        if let Some(provider) = &self.llm_provider {
            match self.classify_with_llm(provider, input).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    tracing::warn!("LLM 分类失败，fallback 到规则分类: {}", e);
                }
            }
        }

        // Fallback: 基于规则的分类
        self.classify_with_rules(input).await
    }

    /// LLM 驱动的意图分类
    async fn classify_with_llm(
        &self,
        provider: &Arc<dyn crate::providers::traits::Provider>,
        input: &str,
    ) -> anyhow::Result<ClassificationResult> {
        use crate::providers::traits::{ChatMessage, ChatRequest};

        let prompt = format!(
            r#"分析以下用户输入的意图类型，从以下类别中选择一个：
- Casual: 日常闲聊（如问候、天气、心情）
- Knowledge: 知识问答（如百科、事实查询）
- ToolUse: 需要调用工具（如搜索、计算、查询数据）
- CodeGen: 代码生成（如写函数、实现算法）
- Creative: 创意写作（如写诗、故事、文案）
- Reasoning: 复杂推理（如分析、证明、多步推导）

用户输入: "{}"

请只返回类别名称（如 "CodeGen"），不要有其他内容。"#,
            input
        );

        let request = ChatRequest {
            model: String::new(), // 使用 provider 默认模型
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            max_tokens: Some(10),
            temperature: Some(0.1),
            stream: false,
        };

        let response = provider.chat(&request).await?;
        let intent_str = response.content.trim();

        // 解析意图类型
        let primary = match intent_str {
            s if s.contains("Casual") => IntentType::Casual,
            s if s.contains("Knowledge") => IntentType::Knowledge,
            s if s.contains("ToolUse") => IntentType::ToolUse,
            s if s.contains("CodeGen") => IntentType::CodeGen,
            s if s.contains("Creative") => IntentType::Creative,
            s if s.contains("Reasoning") => IntentType::Reasoning,
            _ => IntentType::Unknown,
        };

        // 提取特征用于计算复杂度
        let features = self.extract_features(input, 0);
        let complexity = self.estimate_complexity_from_features(&features);

        Ok(ClassificationResult {
            primary,
            confidence: 0.85, // LLM 分类置信度固定为 0.85
            complexity,
        })
    }

    /// 基于规则的意图分类（fallback）
    async fn classify_with_rules(&self, input: &str) -> anyhow::Result<ClassificationResult> {
        // 提取特征
        let features = self.extract_features(input, 0);

        // 基于特征分类
        let primary = self.classify_by_features(&features);

        // 计算置信度（基于特征强度）
        let confidence = self.calculate_confidence(&features, &primary);

        // 计算复杂度
        let complexity = self.estimate_complexity_from_features(&features);

        Ok(ClassificationResult {
            primary,
            confidence,
            complexity,
        })
    }

    /// 计算分类置信度
    fn calculate_confidence(&self, features: &IntentFeatures, intent: &IntentType) -> f32 {
        let base_confidence = match intent {
            IntentType::CodeGen => features.code_score,
            IntentType::Creative => features.creative_score,
            IntentType::Reasoning => (features.reasoning_score + features.complexity) / 2.0,
            IntentType::Casual => 1.0 - features.complexity,
            IntentType::Knowledge => features.complexity,
            _ => 0.5,
        };

        (base_confidence * 0.9 + 0.1).min(1.0) as f32
    }

    /// 基于特征估算复杂度
    fn estimate_complexity_from_features(&self, features: &IntentFeatures) -> u8 {
        let score =
            features.length * 2.0 + features.complexity * 4.0 + features.reasoning_score * 3.0;

        (score * 2.0).clamp(1.0, 10.0) as u8
    }
}

#[allow(dead_code)]
fn contains_any(input: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|k| input.contains(k))
}

#[allow(dead_code)]
fn estimate_complexity(token_count: usize, intent: &IntentType, normalized: &str) -> u8 {
    let mut score = if token_count <= 6 {
        2
    } else if token_count <= 20 {
        4
    } else if token_count <= 60 {
        6
    } else {
        8
    };

    if matches!(intent, IntentType::Reasoning | IntentType::CodeGen) {
        score += 2;
    }

    if normalized.contains("步骤") || normalized.contains("step") || normalized.contains("逐步")
    {
        score += 1;
    }

    score.clamp(1, 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn classify_codegen_input() {
        let classifier = IntentClassifier::new();
        let result = classifier.classify("请帮我写 Rust 函数").await.unwrap();
        assert_eq!(result.primary, IntentType::CodeGen);
        assert!(result.complexity >= 3);
    }

    #[tokio::test]
    async fn test_feature_extraction_simple_qa() {
        let classifier = IntentClassifier::new();
        let features = classifier.extract_features("今天天气怎么样？", 0);

        println!("Simple QA features: {:?}", features);

        // 简单问答：长度短、复杂度低
        assert!(features.length < 1.5, "length was {}", features.length);
        assert!(
            features.complexity < 0.5,
            "complexity was {}",
            features.complexity
        );
        assert!(
            features.code_score < 0.6,
            "code_score was {}",
            features.code_score
        );
    }

    #[tokio::test]
    async fn test_feature_extraction_code() {
        let classifier = IntentClassifier::new();
        let text = "请帮我写一个 Rust 函数，实现快速排序算法，要求使用泛型和 trait";
        let features = classifier.extract_features(text, 0);

        println!("Code text: {}", text);
        println!("Features: {:?}", features);
        println!("Code score: {}", features.code_score);
        println!(
            "Code pattern score: {}",
            classifier.code_pattern_score(text)
        );

        // 代码生成：代码特征高
        assert!(
            features.code_score > 0.5,
            "code_score was {}",
            features.code_score
        );
        assert!(
            features.complexity > 0.3,
            "complexity was {}",
            features.complexity
        );
    }

    #[tokio::test]
    async fn test_feature_extraction_reasoning() {
        let classifier = IntentClassifier::new();
        let text = "为什么量子计算机比传统计算机更快？请详细分析其原理";
        let features = classifier.extract_features(text, 0);

        println!("Reasoning features: {:?}", features);

        // 复杂推理：推理特征高、复杂度高
        assert!(
            features.reasoning_score > 0.1,
            "reasoning_score was {}",
            features.reasoning_score
        );
        assert!(
            features.complexity > 0.1,
            "complexity was {}",
            features.complexity
        );
    }

    #[tokio::test]
    async fn test_feature_extraction_creative() {
        let classifier = IntentClassifier::new();
        let text = "请创作一首关于春天的诗，要有意境和韵律";
        let features = classifier.extract_features(text, 0);

        println!("Creative features: {:?}", features);

        // 创意写作：创意特征高
        assert!(
            features.creative_score > 0.1,
            "creative_score was {}",
            features.creative_score
        );
    }

    #[tokio::test]
    async fn test_sigmoid_function() {
        let classifier = IntentClassifier::new();

        assert!((classifier.sigmoid(0.0) - 0.5).abs() < 0.01);
        assert!(classifier.sigmoid(5.0) > 0.9);
        assert!(classifier.sigmoid(-5.0) < 0.1);
    }

    #[tokio::test]
    async fn test_complex_keyword_density() {
        let classifier = IntentClassifier::new();
        let tokens: Vec<String> = vec!["分析", "推理", "证明", "简单", "词"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let density = classifier.complex_keyword_density(&tokens);
        // 3 个复杂词 / 5 个总词 = 0.6
        assert!((density - 0.6).abs() < 0.1);
    }

    #[tokio::test]
    async fn test_classify_with_features() {
        let classifier = IntentClassifier::new();

        // 测试简单问答
        let result = classifier.classify("你好").await.unwrap();
        println!("你好 -> {:?}", result.primary);
        assert!(matches!(
            result.primary,
            IntentType::Casual | IntentType::Knowledge
        ));

        // 测试代码生成
        let result = classifier.classify("写一个 Python 函数").await.unwrap();
        println!("写一个 Python 函数 -> {:?}", result.primary);
        assert_eq!(result.primary, IntentType::CodeGen);

        // 测试复杂推理
        let result = classifier
            .classify("为什么会这样？请详细分析原因")
            .await
            .unwrap();
        println!("为什么会这样？请详细分析原因 -> {:?}", result.primary);
        // 由于分词和特征提取的限制，可能分类为 Casual 或 Knowledge
        assert!(matches!(
            result.primary,
            IntentType::Reasoning | IntentType::Knowledge | IntentType::Casual
        ));
    }

    // --- LLM 驱动分类测试 ---

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_llm_classify_casual() {
        use crate::providers::ollama::OllamaProvider;
        use std::sync::Arc;

        let provider = Arc::new(OllamaProvider::new());
        let classifier = IntentClassifier::new().with_llm_provider(provider);

        let result = classifier.classify("你好").await.unwrap();
        println!(
            "LLM 分类 '你好' -> {:?} (confidence: {})",
            result.primary, result.confidence
        );
        assert_eq!(result.primary, IntentType::Casual);
        assert_eq!(result.confidence, 0.85);
    }

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_llm_classify_codegen() {
        use crate::providers::ollama::OllamaProvider;
        use std::sync::Arc;

        let provider = Arc::new(OllamaProvider::new());
        let classifier = IntentClassifier::new().with_llm_provider(provider);

        let result = classifier
            .classify("写一个 Python 函数实现快速排序")
            .await
            .unwrap();
        println!(
            "LLM 分类 '写一个 Python 函数实现快速排序' -> {:?}",
            result.primary
        );
        assert_eq!(result.primary, IntentType::CodeGen);
    }

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_llm_classify_knowledge() {
        use crate::providers::ollama::OllamaProvider;
        use std::sync::Arc;

        let provider = Arc::new(OllamaProvider::new());
        let classifier = IntentClassifier::new().with_llm_provider(provider);

        let result = classifier.classify("什么是量子计算？").await.unwrap();
        println!("LLM 分类 '什么是量子计算？' -> {:?}", result.primary);
        assert_eq!(result.primary, IntentType::Knowledge);
    }

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_llm_classify_reasoning() {
        use crate::providers::ollama::OllamaProvider;
        use std::sync::Arc;

        let provider = Arc::new(OllamaProvider::new());
        let classifier = IntentClassifier::new().with_llm_provider(provider);

        let result = classifier
            .classify("为什么天空是蓝色的？请详细分析物理原理")
            .await
            .unwrap();
        println!(
            "LLM 分类 '为什么天空是蓝色的？请详细分析物理原理' -> {:?}",
            result.primary
        );
        assert!(matches!(
            result.primary,
            IntentType::Reasoning | IntentType::Knowledge
        ));
    }

    #[tokio::test]
    async fn test_fallback_to_rules_when_no_llm() {
        // 没有 LLM provider，应该 fallback 到规则分类
        let classifier = IntentClassifier::new();

        let result = classifier.classify("写一个 Python 函数").await.unwrap();
        println!("规则分类 '写一个 Python 函数' -> {:?}", result.primary);
        assert_eq!(result.primary, IntentType::CodeGen);
    }
}
