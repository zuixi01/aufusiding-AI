//! L2 情景记忆
//!
//! 存储具体的对话片段和事件，支持按语义相似度检索，类似人类的情景记忆。
//! 使用向量存储实现语义检索，结合时间衰减计算相关性。

use crate::memory::embedding::EmbeddingService;
use crate::storage::vector::VectorStore;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
}

/// 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// 情景记忆条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    /// 条目 ID
    pub id: Uuid,
    /// 内容摘要
    pub summary: String,
    /// 原始消息
    pub messages: Vec<Message>,
    /// 提取的实体
    pub entities: Vec<String>,
    /// 嵌入向量（用于语义检索）
    pub embedding: Vec<f32>,
    /// 发生时间
    pub timestamp: DateTime<Utc>,
}

/// 情景记忆存储
pub struct EpisodicMemory {
    /// 向量存储
    vector_store: VectorStore,
    /// Embedding 服务
    embedding_service: Arc<dyn EmbeddingService>,
}

/// 提取高频关键词（过滤停用词）
pub fn extract_top_terms(text: &str, n: usize) -> Vec<String> {
    let stop_words: std::collections::HashSet<&str> = [
        // 英文停用词
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
        "do", "does", "did", "will", "would", "could", "should", "may", "might", "shall", "can",
        "need", "dare", "ought", "to", "of", "in", "for", "on", "with", "at", "by", "from", "as",
        "into", "through", "during", "before", "after", "above", "below", "and", "but", "or",
        "nor", "not", "so", "yet", "both", "either", "neither", "each", "every", "all", "any",
        "few", "more", "most", "other", "some", "such", "no", "only", "own", "same", "than", "too",
        "very", "just", "because", "if", "when", "where", "how", "what", "which", "who", "whom",
        "this", "that", "these", "those", "it", "its", "my", "your", "his", "her", "our", "their",
        "i", "me", "we", "us", "you", "he", "she", "they", "them", // 中文停用词
        "的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都", "一", "一个", "上", "也",
        "很", "到", "说", "要", "去", "你", "会", "着", "没有", "看", "好", "自己", "这", "他",
        "她", "吗", "吧", "啊", "呢", "哦", "嗯", "那", "还", "把", "被",
    ]
    .iter()
    .copied()
    .collect();

    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    // 分词：按空格和标点分割
    for word in text.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation()) {
        let w = word.trim().to_lowercase();
        if w.len() >= 2 && !stop_words.contains(w.as_str()) {
            *freq.entry(w).or_insert(0) += 1;
        }
    }

    let mut ranked: Vec<(String, usize)> = freq.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    ranked.into_iter().take(n).map(|(term, _)| term).collect()
}

/// 截断字符串到指定长度
fn truncate_str(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = chars[..max_chars].iter().collect();
        format!("{}...", truncated)
    }
}

/// 判断是否为 CJK 字符
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF |   // CJK Unified Ideographs
        0x3400..=0x4DBF |   // CJK Unified Ideographs Extension A
        0xF900..=0xFAFF     // CJK Compatibility Ideographs
    )
}

/// 简单的日期模式匹配（yyyy-mm-dd）
fn regex_lite_date_matches(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    // 匹配 YYYY-MM-DD 模式
    if len >= 10 {
        for i in 0..=len - 10 {
            if chars[i].is_ascii_digit()
                && chars[i + 1].is_ascii_digit()
                && chars[i + 2].is_ascii_digit()
                && chars[i + 3].is_ascii_digit()
                && chars[i + 4] == '-'
                && chars[i + 5].is_ascii_digit()
                && chars[i + 6].is_ascii_digit()
                && chars[i + 7] == '-'
                && chars[i + 8].is_ascii_digit()
                && chars[i + 9].is_ascii_digit()
            {
                let date: String = chars[i..i + 10].iter().collect();
                results.push(date);
            }
        }
    }
    results
}

impl EpisodicMemory {
    /// 创建情景记忆存储
    pub fn new(db_path: PathBuf, dimension: usize) -> anyhow::Result<Self> {
        let vector_store = VectorStore::new(db_path, dimension);
        vector_store.init()?;

        // 默认使用 MockEmbeddingService 保持向后兼容
        let embedding_service = Arc::new(crate::memory::embedding::MockEmbeddingService::new(
            dimension,
        ));

        Ok(Self {
            vector_store,
            embedding_service,
        })
    }

    /// 创建带自定义 EmbeddingService 的情景记忆
    pub fn with_embedding_service(
        db_path: PathBuf,
        embedding_service: Arc<dyn EmbeddingService>,
    ) -> anyhow::Result<Self> {
        let dimension = embedding_service.dimension();
        let vector_store = VectorStore::new(db_path, dimension);
        vector_store.init()?;

        Ok(Self {
            vector_store,
            embedding_service,
        })
    }

    /// 添加情景记忆
    pub async fn add_episode(&mut self, messages: Vec<Message>) -> anyhow::Result<Uuid> {
        // 生成摘要
        let summary = self.generate_summary(&messages).await?;

        // 提取实体
        let entities = self.extract_entities(&messages);

        // 生成向量（这里使用简单的模拟，实际应该调用嵌入模型）
        let embedding = self.generate_embedding(&summary, &entities).await?;

        let episode = Episode {
            id: Uuid::new_v4(),
            summary: summary.clone(),
            messages,
            entities,
            embedding: embedding.clone(),
            timestamp: Utc::now(),
        };

        // 存储到向量数据库
        let metadata = serde_json::to_string(&episode)?;
        self.vector_store
            .insert(&episode.id.to_string(), &embedding, &metadata)?;

        Ok(episode.id)
    }

    /// 搜索相关情景记忆：相似度 × 0.8 + 时间衰减 × 0.2
    pub async fn search(&self, query: &str, top_k: usize) -> anyhow::Result<Vec<Episode>> {
        // 生成查询向量
        let query_embedding = self.generate_embedding(query, &[]).await?;

        // 向量检索（获取更多候选，用于重排序）
        let candidates = self.vector_store.search(&query_embedding, top_k * 2)?;

        // 重排序：相似度 + 时间衰减
        let mut scored: Vec<(Episode, f64)> = Vec::new();

        for result in candidates {
            let episode: Episode = serde_json::from_str(&result.metadata)?;

            let days_ago = (Utc::now() - episode.timestamp).num_days() as f64;
            let time_decay = 1.0 / (1.0 + (1.0 + days_ago).ln());

            let score = 0.8 * result.similarity as f64 + 0.2 * time_decay;
            scored.push((episode, score));
        }

        // 按得分排序
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored.iter().take(top_k).map(|(e, _)| e.clone()).collect())
    }

    /// 生成摘要：提取主题关键词、决策句、行动项
    async fn generate_summary(&self, messages: &[Message]) -> anyhow::Result<String> {
        if messages.is_empty() {
            return Ok("空对话".to_string());
        }

        let all_text: String = messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        // 提取高频关键词
        let top_terms = extract_top_terms(&all_text, 5);

        // 识别决策句
        let decision_markers = [
            "决定", "选择", "采用", "确定", "will use", "agreed", "decided", "chose",
        ];
        let decisions: Vec<&str> = messages
            .iter()
            .filter(|m| {
                let lower = m.content.to_lowercase();
                decision_markers.iter().any(|marker| lower.contains(marker))
            })
            .map(|m| m.content.as_str())
            .collect();

        // 识别行动项
        let action_markers = [
            "需要",
            "计划",
            "TODO",
            "todo",
            "should",
            "will",
            "要做",
            "接下来",
        ];
        let actions: Vec<&str> = messages
            .iter()
            .filter(|m| {
                let lower = m.content.to_lowercase();
                action_markers.iter().any(|marker| lower.contains(marker))
            })
            .map(|m| m.content.as_str())
            .collect();

        let mut parts = Vec::new();
        if !top_terms.is_empty() {
            parts.push(format!("Topics: {}", top_terms.join(", ")));
        }
        if !decisions.is_empty() {
            let d: Vec<String> = decisions
                .iter()
                .take(2)
                .map(|s| truncate_str(s, 60))
                .collect();
            parts.push(format!("Decisions: {}", d.join("; ")));
        }
        if !actions.is_empty() {
            let a: Vec<String> = actions
                .iter()
                .take(2)
                .map(|s| truncate_str(s, 60))
                .collect();
            parts.push(format!("Actions: {}", a.join("; ")));
        }

        if parts.is_empty() {
            // 回退：简单拼接
            let content: Vec<String> = messages
                .iter()
                .take(3)
                .map(|m| truncate_str(&m.content, 50))
                .collect();
            Ok(content.join(" | "))
        } else {
            Ok(parts.join(" | "))
        }
    }

    /// 提取实体：中文专有名词、英文大写词、日期、技术术语
    fn extract_entities(&self, messages: &[Message]) -> Vec<String> {
        let mut entities = Vec::new();

        let tech_terms: std::collections::HashSet<&str> = [
            "Rust",
            "Python",
            "JavaScript",
            "TypeScript",
            "Go",
            "Java",
            "C++",
            "Docker",
            "Kubernetes",
            "Redis",
            "PostgreSQL",
            "MySQL",
            "MongoDB",
            "React",
            "Vue",
            "Angular",
            "Node",
            "API",
            "REST",
            "GraphQL",
            "Linux",
            "Git",
            "AWS",
            "Azure",
            "GCP",
            "Terraform",
            "Nginx",
            "Kafka",
            "RabbitMQ",
            "Elasticsearch",
            "SQLite",
            "gRPC",
            "WebSocket",
        ]
        .iter()
        .copied()
        .collect();

        let cn_suffixes = [
            "系统", "框架", "模型", "引擎", "平台", "服务", "模块", "组件", "协议", "算法",
        ];

        for message in messages {
            let content = &message.content;

            // 英文大写开头词 + 技术术语
            for word in content.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
                let trimmed = word.trim();
                if trimmed.len() >= 2 {
                    // 技术术语（大小写不敏感匹配）
                    for &tech in &tech_terms {
                        if trimmed.eq_ignore_ascii_case(tech) {
                            entities.push(tech.to_string());
                        }
                    }
                    // 英文大写开头词（非技术术语）
                    if trimmed.len() > 2
                        && trimmed.chars().next().is_some_and(|c| c.is_uppercase())
                        && trimmed.chars().any(|c| c.is_lowercase())
                        && !tech_terms.contains(trimmed)
                    {
                        entities.push(trimmed.to_string());
                    }
                }
            }

            // 中文专有名词模式（2-4 汉字 + 常见后缀）
            for suffix in &cn_suffixes {
                let chars: Vec<char> = content.chars().collect();
                for i in 0..chars.len() {
                    for name_len in 2..=4 {
                        if i + name_len + suffix.chars().count() <= chars.len() {
                            let candidate: String = chars[i..i + name_len].iter().collect();
                            let after: String = chars
                                [i + name_len..i + name_len + suffix.chars().count()]
                                .iter()
                                .collect();
                            if after == *suffix && candidate.chars().all(is_cjk) {
                                let full = format!("{}{}", candidate, suffix);
                                entities.push(full);
                            }
                        }
                    }
                }
            }

            // 日期模式
            // ISO 格式: 2024-01-01
            for cap in regex_lite_date_matches(content) {
                entities.push(cap);
            }

            // 中文日期词
            for keyword in &[
                "明天",
                "后天",
                "昨天",
                "今天",
                "下周",
                "上周",
                "下个月",
                "本周",
            ] {
                if content.contains(keyword) {
                    entities.push(keyword.to_string());
                }
            }
        }

        // 去重
        entities.sort();
        entities.dedup();
        entities
    }

    /// 生成嵌入向量（委托给 EmbeddingService）
    async fn generate_embedding(
        &self,
        text: &str,
        _entities: &[String],
    ) -> anyhow::Result<Vec<f32>> {
        self.embedding_service.embed(text).await
    }

    /// 获取记忆数量
    pub fn count(&self) -> anyhow::Result<usize> {
        self.vector_store.count()
    }

    /// 删除情景记忆
    pub fn delete(&self, id: &Uuid) -> anyhow::Result<()> {
        self.vector_store.delete(&id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_memory() -> EpisodicMemory {
        let db = crate::test_support::temp_db_path("episodic-test");
        EpisodicMemory::new(db, 128).unwrap()
    }

    fn create_test_messages(content: &str) -> Vec<Message> {
        vec![
            Message {
                role: Role::User,
                content: content.to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "好的，我明白了".to_string(),
            },
        ]
    }

    #[tokio::test]
    async fn test_add_episode() {
        let mut memory = create_test_memory();
        let messages = create_test_messages("今天天气很好");

        let id = memory.add_episode(messages).await.unwrap();
        assert_ne!(id, Uuid::nil());

        assert_eq!(memory.count().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_search_episodes() {
        let mut memory = create_test_memory();

        // 添加多个情景
        memory
            .add_episode(create_test_messages("今天天气很好"))
            .await
            .unwrap();
        memory
            .add_episode(create_test_messages("明天要下雨"))
            .await
            .unwrap();
        memory
            .add_episode(create_test_messages("我喜欢编程"))
            .await
            .unwrap();

        // 搜索相关情景
        let results = memory.search("天气", 2).await.unwrap();

        assert!(results.len() <= 2);
        assert!(results.len() > 0);
    }

    #[tokio::test]
    async fn test_extract_entities() {
        let memory = create_test_memory();
        let messages = vec![Message {
            role: Role::User,
            content: "我在 Beijing 工作，喜欢 Rust 编程".to_string(),
        }];

        let entities = memory.extract_entities(&messages);

        assert!(entities.contains(&"Beijing".to_string()));
        assert!(entities.contains(&"Rust".to_string()));
    }

    #[tokio::test]
    async fn test_generate_summary() {
        let memory = create_test_memory();
        let messages = create_test_messages("测试消息");

        let summary = memory.generate_summary(&messages).await.unwrap();

        assert!(!summary.is_empty());
    }

    #[tokio::test]
    async fn test_delete_episode() {
        let mut memory = create_test_memory();
        let messages = create_test_messages("要删除的记忆");

        let id = memory.add_episode(messages).await.unwrap();
        assert_eq!(memory.count().unwrap(), 1);

        memory.delete(&id).unwrap();
        assert_eq!(memory.count().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_time_decay_in_search() {
        let mut memory = create_test_memory();

        // 添加情景
        let id1 = memory
            .add_episode(create_test_messages("第一条记忆"))
            .await
            .unwrap();

        // 搜索应该能找到
        let results = memory.search("记忆", 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id1);
    }

    #[tokio::test]
    async fn test_embedding_generation() {
        let memory = create_test_memory();

        let embedding1 = memory.generate_embedding("测试文本", &[]).await.unwrap();
        let embedding2 = memory.generate_embedding("测试文本", &[]).await.unwrap();

        // 相同文本应该生成相同向量
        assert_eq!(embedding1.len(), 128);
        assert_eq!(embedding1, embedding2);
    }

    #[tokio::test]
    async fn test_multiple_episodes() {
        let mut memory = create_test_memory();

        // 添加多个情景
        for i in 0..5 {
            let messages = create_test_messages(&format!("情景 {}", i));
            memory.add_episode(messages).await.unwrap();
        }

        assert_eq!(memory.count().unwrap(), 5);

        // 搜索应该返回最相关的
        let results = memory.search("情景", 3).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn generate_summary_extracts_topics() {
        let memory = create_test_memory();
        let messages = vec![
            Message {
                role: Role::User,
                content: "Rust 编程 Rust 性能优化 Rust 并发".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "Rust 的并发模型非常强大".to_string(),
            },
        ];
        let summary = memory.generate_summary(&messages).await.unwrap();
        assert!(summary.contains("Topics:"));
        assert!(summary.to_lowercase().contains("rust"));
    }

    #[tokio::test]
    async fn generate_summary_identifies_decisions() {
        let memory = create_test_memory();
        let messages = vec![
            Message {
                role: Role::User,
                content: "我们决定使用 PostgreSQL 作为数据库".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "好的选择".to_string(),
            },
        ];
        let summary = memory.generate_summary(&messages).await.unwrap();
        assert!(summary.contains("Decisions:"));
    }

    #[tokio::test]
    async fn generate_summary_identifies_actions() {
        let memory = create_test_memory();
        let messages = vec![
            Message {
                role: Role::User,
                content: "我们需要重构认证模块".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "计划下周开始".to_string(),
            },
        ];
        let summary = memory.generate_summary(&messages).await.unwrap();
        assert!(summary.contains("Actions:"));
    }

    #[tokio::test]
    async fn generate_summary_empty_messages() {
        let memory = create_test_memory();
        let summary = memory.generate_summary(&[]).await.unwrap();
        assert_eq!(summary, "空对话");
    }

    #[tokio::test]
    async fn extract_entities_finds_tech_terms() {
        let memory = create_test_memory();
        let messages = vec![Message {
            role: Role::User,
            content: "我在用 Rust 和 Docker 部署 API 服务".to_string(),
        }];
        let entities = memory.extract_entities(&messages);
        assert!(entities.contains(&"Rust".to_string()));
        assert!(entities.contains(&"Docker".to_string()));
        assert!(entities.contains(&"API".to_string()));
    }

    #[tokio::test]
    async fn extract_entities_finds_dates() {
        let memory = create_test_memory();
        let messages = vec![Message {
            role: Role::User,
            content: "截止日期是 2024-03-15，明天开始".to_string(),
        }];
        let entities = memory.extract_entities(&messages);
        assert!(entities.contains(&"2024-03-15".to_string()));
        assert!(entities.contains(&"明天".to_string()));
    }

    #[test]
    fn extract_top_terms_basic() {
        let terms = extract_top_terms("rust rust rust python python java", 2);
        assert_eq!(terms.len(), 2);
        assert_eq!(terms[0], "rust");
        assert_eq!(terms[1], "python");
    }

    #[test]
    fn extract_top_terms_filters_stopwords() {
        let terms = extract_top_terms("the the the is is a a rust", 3);
        assert!(terms.contains(&"rust".to_string()));
        assert!(!terms.contains(&"the".to_string()));
    }
}
