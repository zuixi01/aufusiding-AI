//! 数据导出
//!
//! GDPR 数据可携带权：导出用户的所有个人数据为 JSON 格式。

use crate::memory::episodic::EpisodicMemory;
use crate::memory::knowledge::KnowledgeBase;
use crate::memory::working::WorkingMemory;
use crate::session::history::HistoryStore;
use serde_json::json;
use uuid::Uuid;

/// 数据导出器
pub struct DataExporter<'a> {
    history: &'a HistoryStore,
    episodic: &'a EpisodicMemory,
    knowledge: &'a KnowledgeBase,
    working: &'a WorkingMemory,
}

impl<'a> DataExporter<'a> {
    pub fn new(
        history: &'a HistoryStore,
        episodic: &'a EpisodicMemory,
        knowledge: &'a KnowledgeBase,
        working: &'a WorkingMemory,
    ) -> Self {
        Self {
            history,
            episodic,
            knowledge,
            working,
        }
    }

    /// 导出用户所有数据
    pub async fn export_all(&self, session_id: Uuid) -> anyhow::Result<serde_json::Value> {
        // 导出历史消息
        let messages = self.history.get_session_messages(session_id).await?;
        let messages_json: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                json!({
                    "id": m.id.to_string(),
                    "role": format!("{:?}", m.role),
                    "content": m.content,
                    "timestamp": m.timestamp.to_rfc3339(),
                })
            })
            .collect();

        // 导出工作记忆
        let working_items = self.working.get_active_context(usize::MAX);

        // 导出情景记忆计数
        let episodic_count = self.episodic.count().unwrap_or(0);

        // 导出知识库计数
        let knowledge_count = self.knowledge.count().unwrap_or(0);

        Ok(json!({
            "export_version": "1.0",
            "session_id": session_id.to_string(),
            "history": {
                "messages": messages_json,
                "count": messages.len(),
            },
            "working_memory": {
                "items": working_items,
                "count": working_items.len(),
            },
            "episodic_memory": {
                "count": episodic_count,
            },
            "knowledge_base": {
                "count": knowledge_count,
            },
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::episodic::EpisodicMemory;
    use crate::memory::knowledge::KnowledgeBase;
    use crate::memory::working::WorkingMemory;
    use crate::session::history::{HistoryStore, Message, Role};
    use chrono::Utc;
    use std::path::PathBuf;

    fn temp_path(prefix: &str) -> PathBuf {
        crate::test_support::temp_db_path(prefix)
    }

    #[tokio::test]
    async fn export_empty_data() {
        let history = HistoryStore::new(temp_path("hist"));
        let episodic = EpisodicMemory::new(temp_path("ep"), 128).unwrap();
        let knowledge = KnowledgeBase::new(temp_path("kb"), 128).unwrap();
        let working = WorkingMemory::new(64);

        let exporter = DataExporter::new(&history, &episodic, &knowledge, &working);
        let result = exporter.export_all(Uuid::new_v4()).await.unwrap();

        assert_eq!(result["history"]["count"], 0);
        assert_eq!(result["working_memory"]["count"], 0);
        assert_eq!(result["export_version"], "1.0");
    }

    #[tokio::test]
    async fn export_with_history() {
        let history = HistoryStore::new(temp_path("hist"));
        let session_id = Uuid::new_v4();

        history
            .append(&Message {
                id: Uuid::new_v4(),
                session_id,
                role: Role::User,
                content: "hello".into(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        let episodic = EpisodicMemory::new(temp_path("ep"), 128).unwrap();
        let knowledge = KnowledgeBase::new(temp_path("kb"), 128).unwrap();
        let working = WorkingMemory::new(64);

        let exporter = DataExporter::new(&history, &episodic, &knowledge, &working);
        let result = exporter.export_all(session_id).await.unwrap();

        assert_eq!(result["history"]["count"], 1);
        assert_eq!(result["history"]["messages"][0]["content"], "hello");
    }

    #[tokio::test]
    async fn export_with_working_memory() {
        let history = HistoryStore::new(temp_path("hist"));
        let episodic = EpisodicMemory::new(temp_path("ep"), 128).unwrap();
        let knowledge = KnowledgeBase::new(temp_path("kb"), 128).unwrap();
        let mut working = WorkingMemory::new(64);
        working.add("记忆条目".into(), 0.8);

        let exporter = DataExporter::new(&history, &episodic, &knowledge, &working);
        let result = exporter.export_all(Uuid::new_v4()).await.unwrap();

        assert_eq!(result["working_memory"]["count"], 1);
    }
}
