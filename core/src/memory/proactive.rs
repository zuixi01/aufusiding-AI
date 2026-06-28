//! L4 主动回忆
//!
//! 基于当前上下文主动检索相关记忆，无需用户显式提问。
//! 实现"你之前提到过..."的智能联想能力。

use crate::memory::episodic::EpisodicMemory;
use crate::memory::knowledge::KnowledgeBase;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 回忆配置
#[derive(Debug, Clone)]
pub struct RecallConfig {
    pub threshold: f32,
    pub max_results: usize,
    pub min_relevance: f32,
}

impl Default for RecallConfig {
    fn default() -> Self {
        Self {
            threshold: 0.3,
            max_results: 5,
            min_relevance: 0.1,
        }
    }
}

/// 回忆触发条件
#[derive(Debug, Clone)]
pub struct RecallTrigger {
    pub keywords: Vec<String>,
    pub similarity_threshold: f32,
}

/// 主动回忆结果
#[derive(Debug, Clone)]
pub struct RecallResult {
    pub summary: String,
    pub relevance: f32,
    pub source_layer: String,
}

/// 主动回忆引擎
pub struct ProactiveRecall {
    config: RecallConfig,
    episodic: Option<Arc<RwLock<EpisodicMemory>>>,
    knowledge: Option<Arc<RwLock<KnowledgeBase>>>,
    last_keywords: Vec<String>,
}

impl ProactiveRecall {
    /// 创建简单实例（无真实数据源，向后兼容）
    pub fn new(threshold: f32) -> Self {
        Self {
            config: RecallConfig {
                threshold,
                ..Default::default()
            },
            episodic: None,
            knowledge: None,
            last_keywords: Vec::new(),
        }
    }

    pub fn with_config(config: RecallConfig) -> Self {
        Self {
            config,
            episodic: None,
            knowledge: None,
            last_keywords: Vec::new(),
        }
    }

    /// 创建带真实 L2/L3 数据源的实例
    pub fn with_sources(
        config: RecallConfig,
        episodic: Arc<RwLock<EpisodicMemory>>,
        knowledge: Arc<RwLock<KnowledgeBase>>,
    ) -> Self {
        Self {
            config,
            episodic: Some(episodic),
            knowledge: Some(knowledge),
            last_keywords: Vec::new(),
        }
    }

    /// 从上下文提取关键词
    fn extract_keywords(context: &str) -> Vec<String> {
        context
            .split_whitespace()
            .filter(|w| w.len() >= 2)
            .map(|w| w.to_lowercase())
            .collect()
    }

    /// 根据当前对话上下文，主动检索相关记忆（L2+L3 并行）
    pub async fn recall(&self, context: &str) -> anyhow::Result<Vec<RecallResult>> {
        let keywords = Self::extract_keywords(context);
        if keywords.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();

        if self.episodic.is_some() || self.knowledge.is_some() {
            // L2 + L3 并行查询
            let ep = self.episodic.clone();
            let kb = self.knowledge.clone();
            let ctx1 = context.to_string();
            let ctx2 = context.to_string();

            let (ep_results, kb_results) = tokio::join!(
                async {
                    if let Some(ep) = ep {
                        let guard = ep.read().await;
                        guard.search(&ctx1, 3).await.unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                },
                async {
                    if let Some(kb) = kb {
                        let guard = kb.read().await;
                        guard.search(&ctx2, 3).await.unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                }
            );

            for episode in ep_results {
                results.push(RecallResult {
                    summary: episode.summary.clone(),
                    relevance: 0.7,
                    source_layer: "L2-episodic".into(),
                });
            }
            for chunk in kb_results {
                results.push(RecallResult {
                    summary: chunk.content.clone(),
                    relevance: 0.6,
                    source_layer: "L3-knowledge".into(),
                });
            }
        } else {
            // 无真实数据源，回退到模拟
            let relevance = (keywords.len() as f32 * 0.15).min(0.9);
            if relevance >= self.config.min_relevance {
                results.push(RecallResult {
                    summary: format!("相关对话记忆: {}", keywords.join(", ")),
                    relevance,
                    source_layer: "L2-episodic".into(),
                });
            }
            if keywords.len() >= 2 {
                let rel = (keywords.len() as f32 * 0.12).min(0.85);
                if rel >= self.config.min_relevance {
                    results.push(RecallResult {
                        summary: format!("相关知识: {}", keywords[..2].join(", ")),
                        relevance: rel,
                        source_layer: "L3-knowledge".into(),
                    });
                }
            }
        }

        // 按相关性排序，截断到 max_results
        results.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(self.config.max_results);
        Ok(results)
    }

    /// 判断是否应该主动插入回忆（关键词变化率 > 阈值）
    pub fn should_recall(&mut self, context: &str) -> bool {
        let new_keywords = Self::extract_keywords(context);
        if new_keywords.is_empty() {
            return false;
        }
        if self.last_keywords.is_empty() {
            self.last_keywords = new_keywords;
            return true;
        }
        let new_set: std::collections::HashSet<&str> =
            new_keywords.iter().map(|s| s.as_str()).collect();
        let old_set: std::collections::HashSet<&str> =
            self.last_keywords.iter().map(|s| s.as_str()).collect();
        let intersection = new_set.intersection(&old_set).count();
        let union = new_set.union(&old_set).count();
        let change_rate = 1.0 - (intersection as f32 / union as f32);

        self.last_keywords = new_keywords;
        change_rate > self.config.threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::episodic::{EpisodicMemory, Message, Role};
    use crate::memory::knowledge::KnowledgeBase;

    fn create_test_episodic() -> EpisodicMemory {
        let db = crate::test_support::temp_db_path("proactive-ep");
        EpisodicMemory::new(db, 128).unwrap()
    }

    fn create_test_kb() -> KnowledgeBase {
        let db = crate::test_support::temp_db_path("proactive-kb");
        KnowledgeBase::new(db, 128).unwrap()
    }

    #[tokio::test]
    async fn basic_recall_backward_compat() {
        let recall = ProactiveRecall::new(0.3);
        let results = recall.recall("rust programming language").await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].relevance > 0.0);
    }

    #[tokio::test]
    async fn empty_context_returns_empty() {
        let recall = ProactiveRecall::new(0.3);
        let results = recall.recall("").await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn threshold_filtering() {
        let config = RecallConfig {
            threshold: 0.3,
            max_results: 5,
            min_relevance: 0.99,
        };
        let recall = ProactiveRecall::with_config(config);
        let results = recall.recall("test query").await.unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn should_recall_detects_change() {
        let mut recall = ProactiveRecall::new(0.3);
        assert!(recall.should_recall("rust programming"));
        assert!(!recall.should_recall("rust programming"));
        assert!(recall.should_recall("python machine learning deep neural"));
    }

    #[tokio::test]
    async fn multi_layer_results() {
        let recall = ProactiveRecall::new(0.3);
        let results = recall
            .recall("rust programming language design patterns")
            .await
            .unwrap();
        let layers: Vec<&str> = results.iter().map(|r| r.source_layer.as_str()).collect();
        assert!(layers.contains(&"L2-episodic"));
        assert!(layers.contains(&"L3-knowledge"));
    }

    #[tokio::test]
    async fn recall_queries_episodic_memory() {
        let mut ep = create_test_episodic();
        ep.add_episode(vec![
            Message {
                role: Role::User,
                content: "Rust 的所有权系统很强大".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "是的，这是 Rust 的核心特性".to_string(),
            },
        ])
        .await
        .unwrap();

        let ep = Arc::new(RwLock::new(ep));
        let kb = Arc::new(RwLock::new(create_test_kb()));
        let recall = ProactiveRecall::with_sources(RecallConfig::default(), ep, kb);

        let results = recall.recall("Rust 所有权").await.unwrap();
        assert!(results.iter().any(|r| r.source_layer == "L2-episodic"));
    }

    #[tokio::test]
    async fn recall_queries_knowledge_base() {
        let ep = create_test_episodic();
        let mut kb = create_test_kb();
        kb.add_document("rust.md", "Rust 是一门系统编程语言，注重安全性和性能")
            .await
            .unwrap();

        let ep = Arc::new(RwLock::new(ep));
        let kb = Arc::new(RwLock::new(kb));
        let recall = ProactiveRecall::with_sources(RecallConfig::default(), ep, kb);

        let results = recall.recall("Rust 编程").await.unwrap();
        assert!(results.iter().any(|r| r.source_layer == "L3-knowledge"));
    }

    #[tokio::test]
    async fn recall_merges_and_sorts_results() {
        let mut ep = create_test_episodic();
        ep.add_episode(vec![
            Message {
                role: Role::User,
                content: "讨论数据库设计".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "好的".to_string(),
            },
        ])
        .await
        .unwrap();

        let mut kb = create_test_kb();
        kb.add_document("db.md", "数据库设计最佳实践")
            .await
            .unwrap();

        let ep = Arc::new(RwLock::new(ep));
        let kb = Arc::new(RwLock::new(kb));
        let recall = ProactiveRecall::with_sources(RecallConfig::default(), ep, kb);

        let results = recall.recall("数据库设计").await.unwrap();
        // Should have results from both layers, sorted by relevance
        assert!(results.len() >= 2);
        for i in 1..results.len() {
            assert!(results[i - 1].relevance >= results[i].relevance);
        }
    }

    #[tokio::test]
    async fn recall_respects_max_results() {
        let mut ep = create_test_episodic();
        for i in 0..5 {
            ep.add_episode(vec![
                Message {
                    role: Role::User,
                    content: format!("话题 {}", i),
                },
                Message {
                    role: Role::Assistant,
                    content: "好的".to_string(),
                },
            ])
            .await
            .unwrap();
        }

        let ep = Arc::new(RwLock::new(ep));
        let kb = Arc::new(RwLock::new(create_test_kb()));
        let config = RecallConfig {
            max_results: 2,
            ..Default::default()
        };
        let recall = ProactiveRecall::with_sources(config, ep, kb);

        let results = recall.recall("话题").await.unwrap();
        assert!(results.len() <= 2);
    }
}
