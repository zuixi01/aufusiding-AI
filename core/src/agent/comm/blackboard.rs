//! 共享黑板 — 子任务间的知识共享机制
//!
//! 子任务将发现和结论写入黑板，后续子任务可查询相关条目，
//! 避免重复推理已解决的问题。

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

/// 黑板条目
#[derive(Debug, Clone)]
pub struct BlackboardEntry {
    pub key: String,
    pub content: String,
    pub author_task: String,
    pub timestamp: DateTime<Utc>,
    pub relevance_tags: Vec<String>,
}

/// 共享黑板
#[derive(Clone)]
pub struct Blackboard {
    entries: Arc<RwLock<HashMap<String, BlackboardEntry>>>,
}

impl Blackboard {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 写入发现/结论到黑板
    pub async fn post(&self, key: &str, content: &str, author_task: &str, tags: Vec<String>) {
        let entry = BlackboardEntry {
            key: key.to_string(),
            content: content.to_string(),
            author_task: author_task.to_string(),
            timestamp: Utc::now(),
            relevance_tags: tags,
        };
        self.entries.write().await.insert(key.to_string(), entry);
    }

    /// 按标签检索相关条目
    pub async fn query(&self, tags: &[String]) -> Vec<BlackboardEntry> {
        let entries = self.entries.read().await;
        entries
            .values()
            .filter(|entry| tags.iter().any(|tag| entry.relevance_tags.contains(tag)))
            .cloned()
            .collect()
    }

    /// 生成黑板摘要（截断到 token 预算）
    pub async fn summary(&self, max_tokens: usize) -> String {
        let entries = self.entries.read().await;
        if entries.is_empty() {
            return String::new();
        }

        let mut sorted: Vec<&BlackboardEntry> = entries.values().collect();
        sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        let mut summary = String::from("[黑板摘要]\n");
        let mut token_count = 2; // 标题行

        for entry in sorted {
            let line = format!(
                "- [{}] {}: {}\n",
                entry.author_task, entry.key, entry.content
            );
            let line_tokens = rough_token_count(&line);
            if token_count + line_tokens > max_tokens {
                summary.push_str("...[截断]\n");
                break;
            }
            summary.push_str(&line);
            token_count += line_tokens;
        }

        summary
    }

    /// 条目数量
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// 是否为空
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }
}

/// 粗略 token 估算
fn rough_token_count(text: &str) -> usize {
    let count = text.split_whitespace().count();
    if count == 0 {
        1
    } else {
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn post_and_query_basic() {
        let bb = Blackboard::new();
        bb.post(
            "auth-design",
            "使用 JWT + OAuth2",
            "架构设计",
            vec!["auth".into(), "security".into()],
        )
        .await;
        bb.post(
            "db-schema",
            "用户表设计完成",
            "数据库设计",
            vec!["database".into()],
        )
        .await;

        let results = bb.query(&["auth".to_string()]).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "auth-design");

        let db_results = bb.query(&["database".to_string()]).await;
        assert_eq!(db_results.len(), 1);
    }

    #[tokio::test]
    async fn summary_token_truncation() {
        let bb = Blackboard::new();
        for i in 0..20 {
            bb.post(
                &format!("key-{}", i),
                &format!("这是一段很长的内容描述 number {}", i),
                &format!("task-{}", i),
                vec!["tag".into()],
            )
            .await;
        }

        let summary = bb.summary(10).await;
        assert!(summary.contains("[黑板摘要]"));
        assert!(summary.contains("...[截断]"));
    }

    #[tokio::test]
    async fn multi_task_shared_read_write() {
        let bb = Blackboard::new();
        let bb1 = bb.clone();
        let bb2 = bb.clone();

        let h1 = tokio::spawn(async move {
            bb1.post("finding-1", "发现A", "task-1", vec!["research".into()])
                .await;
        });
        let h2 = tokio::spawn(async move {
            bb2.post("finding-2", "发现B", "task-2", vec!["research".into()])
                .await;
        });

        h1.await.unwrap();
        h2.await.unwrap();

        assert_eq!(bb.len().await, 2);
        let results = bb.query(&["research".to_string()]).await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn empty_blackboard_returns_empty_summary() {
        let bb = Blackboard::new();
        let summary = bb.summary(100).await;
        assert!(summary.is_empty());
    }
}
