//! 数据删除
//!
//! GDPR 被遗忘权：级联删除用户数据，生成删除凭证。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::memory::episodic::EpisodicMemory;
use crate::memory::working::WorkingMemory;
use crate::session::history::HistoryStore;

/// 删除凭证
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionReceipt {
    pub session_id: Uuid,
    pub messages_deleted: usize,
    pub episodic_deleted: usize,
    pub working_cleared: bool,
    pub timestamp: DateTime<Utc>,
}

/// 数据删除器
pub struct DataEraser<'a> {
    history: &'a HistoryStore,
    episodic: &'a mut EpisodicMemory,
    working: &'a mut WorkingMemory,
}

impl<'a> DataEraser<'a> {
    pub fn new(
        history: &'a HistoryStore,
        episodic: &'a mut EpisodicMemory,
        working: &'a mut WorkingMemory,
    ) -> Self {
        Self {
            history,
            episodic,
            working,
        }
    }

    /// 删除用户所有数据
    pub async fn delete_all(&mut self, session_id: Uuid) -> anyhow::Result<DeletionReceipt> {
        // 删除历史消息
        let messages_deleted = self.history.delete_by_session(session_id)?;

        // 删除情景记忆（搜索并逐条删除）
        let episodes = self.episodic.search("", 1000).await.unwrap_or_default();
        let mut episodic_deleted = 0;
        for ep in &episodes {
            if self.episodic.delete(&ep.id).is_ok() {
                episodic_deleted += 1;
            }
        }

        // 清空工作记忆（通过创建新实例替换）
        let capacity = self.working.len().max(1);
        *self.working = WorkingMemory::new(capacity);
        let working_cleared = self.working.is_empty();

        Ok(DeletionReceipt {
            session_id,
            messages_deleted,
            episodic_deleted,
            working_cleared,
            timestamp: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::episodic::EpisodicMemory;
    use crate::memory::working::WorkingMemory;
    use crate::session::history::{HistoryStore, Message, Role};
    use chrono::Utc;
    use std::path::PathBuf;

    fn temp_path(prefix: &str) -> PathBuf {
        crate::test_support::temp_db_path(prefix)
    }

    #[tokio::test]
    async fn delete_empty_data() {
        let history = HistoryStore::new(temp_path("hist"));
        let mut episodic = EpisodicMemory::new(temp_path("ep"), 128).unwrap();
        let mut working = WorkingMemory::new(64);

        let mut eraser = DataEraser::new(&history, &mut episodic, &mut working);
        let receipt = eraser.delete_all(Uuid::new_v4()).await.unwrap();

        assert_eq!(receipt.messages_deleted, 0);
        assert_eq!(receipt.episodic_deleted, 0);
        assert!(receipt.working_cleared);
    }

    #[tokio::test]
    async fn delete_history_messages() {
        let history = HistoryStore::new(temp_path("hist"));
        let session_id = Uuid::new_v4();

        for i in 0..3 {
            history
                .append(&Message {
                    id: Uuid::new_v4(),
                    session_id,
                    role: Role::User,
                    content: format!("msg {i}"),
                    timestamp: Utc::now(),
                })
                .await
                .unwrap();
        }

        let mut episodic = EpisodicMemory::new(temp_path("ep"), 128).unwrap();
        let mut working = WorkingMemory::new(64);

        let mut eraser = DataEraser::new(&history, &mut episodic, &mut working);
        let receipt = eraser.delete_all(session_id).await.unwrap();

        assert_eq!(receipt.messages_deleted, 3);

        // 验证数据已删除
        let remaining = history.get_session_messages(session_id).await.unwrap();
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn delete_clears_working_memory() {
        let history = HistoryStore::new(temp_path("hist"));
        let mut episodic = EpisodicMemory::new(temp_path("ep"), 128).unwrap();
        let mut working = WorkingMemory::new(64);
        working.add("item1".into(), 0.5);
        working.add("item2".into(), 0.8);
        assert_eq!(working.len(), 2);

        let mut eraser = DataEraser::new(&history, &mut episodic, &mut working);
        let receipt = eraser.delete_all(Uuid::new_v4()).await.unwrap();

        assert!(receipt.working_cleared);
        assert!(working.is_empty());
    }

    #[test]
    fn deletion_receipt_serializable() {
        let receipt = DeletionReceipt {
            session_id: Uuid::new_v4(),
            messages_deleted: 5,
            episodic_deleted: 2,
            working_cleared: true,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&receipt).unwrap();
        let parsed: DeletionReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.messages_deleted, 5);
    }
}
