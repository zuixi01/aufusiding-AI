//! 历史管理
//!
//! 对话消息的持久化存储与检索，支持按会话、时间范围、关键词查询。

use crate::storage::Database;
use chrono::TimeZone;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// 对话消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 消息 ID
    pub id: Uuid,
    /// 所属会话 ID
    pub session_id: Uuid,
    /// 角色
    pub role: Role,
    /// 消息内容
    pub content: String,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

/// 历史存储
pub struct HistoryStore {
    db: Database,
}

impl HistoryStore {
    /// 创建历史存储
    pub fn new(db_path: std::path::PathBuf) -> Self {
        let db = Database::open(db_path).expect("failed to open history db");
        let store = Self { db };
        if let Err(e) = store.init_schema() {
            tracing::error!("failed to init history schema: {}", e);
        }
        store
    }

    fn init_schema(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session_ts
              ON messages(session_id, timestamp_ms);
            CREATE INDEX IF NOT EXISTS idx_messages_content
              ON messages(content);
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts
              USING fts5(id UNINDEXED, content, tokenize='unicode61');
            ",
        )?;
        // 首次启动时自动回填 FTS 索引（仅填充尚未索引的行）
        self.backfill_fts()?;
        Ok(())
    }

    /// 回填 FTS 索引：将 messages 表中尚未索引的行写入 messages_fts
    fn backfill_fts(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "INSERT OR IGNORE INTO messages_fts(id, content)
             SELECT id, content FROM messages
             WHERE id NOT IN (SELECT id FROM messages_fts);",
        )?;
        Ok(())
    }

    /// 追加消息（同步写入 FTS 索引）
    pub async fn append(&self, message: &Message) -> anyhow::Result<()> {
        let id = message.id.to_string();
        let session_id = message.session_id.to_string();
        let role = role_to_str(&message.role);
        let content = message.content.clone();
        let ts = message.timestamp.timestamp_millis();

        self.db.execute(
            "INSERT OR REPLACE INTO messages (id, session_id, role, content, timestamp_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            &[&id, &session_id, &role, &content, &ts],
        )?;

        // 同步写入 FTS 索引
        if let Err(e) = self.db.execute(
            "INSERT OR REPLACE INTO messages_fts(id, content) VALUES (?1, ?2)",
            &[&id, &content],
        ) {
            tracing::warn!("FTS index write failed: {}", e);
        }

        Ok(())
    }

    /// 获取会话的所有消息
    pub async fn get_session_messages(&self, session_id: Uuid) -> anyhow::Result<Vec<Message>> {
        let session_id_str = session_id.to_string();
        self.db.query_map(
            "SELECT id, session_id, role, content, timestamp_ms
             FROM messages
             WHERE session_id = ?1
             ORDER BY timestamp_ms ASC, rowid ASC",
            &[&session_id_str],
            |row| {
                let id: String = row.get(0)?;
                let sid: String = row.get(1)?;
                let role: String = row.get(2)?;
                let content: String = row.get(3)?;
                let timestamp_ms: i64 = row.get(4)?;

                Ok(Message {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()),
                    session_id: Uuid::parse_str(&sid).unwrap_or_else(|_| Uuid::nil()),
                    role: str_to_role(&role),
                    content,
                    timestamp: Utc
                        .timestamp_millis_opt(timestamp_ms)
                        .single()
                        .unwrap_or_else(Utc::now),
                })
            },
        )
    }

    /// 按关键词搜索历史消息（FTS5 全文搜索，回退 LIKE）
    pub async fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<Message>> {
        let limit_i64 = limit as i64;
        // 尝试 FTS5 MATCH 搜索
        let fts_result = self.db.query_map(
            "SELECT m.id, m.session_id, m.role, m.content, m.timestamp_ms
             FROM messages_fts f
             JOIN messages m ON m.id = f.id
             WHERE messages_fts MATCH ?1
             ORDER BY m.timestamp_ms DESC
             LIMIT ?2",
            &[&query as &dyn rusqlite::types::ToSql, &limit_i64],
            |row| {
                let id: String = row.get(0)?;
                let sid: String = row.get(1)?;
                let role: String = row.get(2)?;
                let content: String = row.get(3)?;
                let timestamp_ms: i64 = row.get(4)?;
                Ok(Message {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()),
                    session_id: Uuid::parse_str(&sid).unwrap_or_else(|_| Uuid::nil()),
                    role: str_to_role(&role),
                    content,
                    timestamp: Utc
                        .timestamp_millis_opt(timestamp_ms)
                        .single()
                        .unwrap_or_else(Utc::now),
                })
            },
        );

        // FTS 成功且有结果则返回，否则回退 LIKE（兼容 CJK 子串搜索）
        if let Ok(ref results) = fts_result {
            if !results.is_empty() {
                return fts_result;
            }
        }

        // LIKE 回退
        let pattern = format!("%{}%", query);
        self.db.query_map(
            "SELECT id, session_id, role, content, timestamp_ms
             FROM messages
             WHERE content LIKE ?1
             ORDER BY timestamp_ms DESC, rowid DESC
             LIMIT ?2",
            &[&pattern as &dyn rusqlite::types::ToSql, &limit_i64],
            |row| {
                let id: String = row.get(0)?;
                let sid: String = row.get(1)?;
                let role: String = row.get(2)?;
                let content: String = row.get(3)?;
                let timestamp_ms: i64 = row.get(4)?;
                Ok(Message {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()),
                    session_id: Uuid::parse_str(&sid).unwrap_or_else(|_| Uuid::nil()),
                    role: str_to_role(&role),
                    content,
                    timestamp: Utc
                        .timestamp_millis_opt(timestamp_ms)
                        .single()
                        .unwrap_or_else(Utc::now),
                })
            },
        )
    }

    /// 分页获取会话消息
    pub async fn get_session_messages_paginated(
        &self,
        session_id: Uuid,
        offset: usize,
        limit: usize,
    ) -> anyhow::Result<Vec<Message>> {
        let session_id_str = session_id.to_string();
        let offset_i64 = offset as i64;
        let limit_i64 = limit as i64;
        self.db.query_map(
            "SELECT id, session_id, role, content, timestamp_ms
             FROM messages
             WHERE session_id = ?1
             ORDER BY timestamp_ms ASC, rowid ASC
             LIMIT ?2 OFFSET ?3",
            &[
                &session_id_str as &dyn rusqlite::types::ToSql,
                &limit_i64,
                &offset_i64,
            ],
            |row| {
                let id: String = row.get(0)?;
                let sid: String = row.get(1)?;
                let role: String = row.get(2)?;
                let content: String = row.get(3)?;
                let timestamp_ms: i64 = row.get(4)?;
                Ok(Message {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()),
                    session_id: Uuid::parse_str(&sid).unwrap_or_else(|_| Uuid::nil()),
                    role: str_to_role(&role),
                    content,
                    timestamp: Utc
                        .timestamp_millis_opt(timestamp_ms)
                        .single()
                        .unwrap_or_else(Utc::now),
                })
            },
        )
    }

    /// 删除指定会话的所有消息
    pub fn delete_by_session(&self, session_id: Uuid) -> anyhow::Result<usize> {
        let sid = session_id.to_string();
        self.db
            .execute("DELETE FROM messages WHERE session_id = ?1", &[&sid])
    }
}

fn role_to_str(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

fn str_to_role(role: &str) -> Role {
    match role {
        "assistant" => Role::Assistant,
        "system" => Role::System,
        "tool" => Role::Tool,
        _ => Role::User,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn append_and_search_messages() {
        let db = crate::test_support::temp_db_path("history-test");
        let store = HistoryStore::new(db);
        let session_id = Uuid::new_v4();

        let message = Message {
            id: Uuid::new_v4(),
            session_id,
            role: Role::User,
            content: "请提醒我明天开会".to_string(),
            timestamp: Utc::now(),
        };

        store.append(&message).await.unwrap();

        let found = store.search("提醒", 10).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].session_id, session_id);
    }

    #[tokio::test]
    async fn persists_messages_across_store_instances() {
        let db = crate::test_support::temp_db_path("history-persist");
        let session_id = Uuid::new_v4();

        let store_a = HistoryStore::new(db.clone());
        store_a
            .append(&Message {
                id: Uuid::new_v4(),
                session_id,
                role: Role::User,
                content: "first message".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        let store_b = HistoryStore::new(db);
        let messages = store_b.get_session_messages(session_id).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "first message");
    }

    #[tokio::test]
    async fn search_respects_limit() {
        let db = crate::test_support::temp_db_path("history-limit");
        let store = HistoryStore::new(db);
        let session_id = Uuid::new_v4();

        for idx in 0..5 {
            store
                .append(&Message {
                    id: Uuid::new_v4(),
                    session_id,
                    role: Role::User,
                    content: format!("limit keyword {}", idx),
                    timestamp: Utc::now(),
                })
                .await
                .unwrap();
        }

        let found = store.search("keyword", 3).await.unwrap();
        assert_eq!(found.len(), 3);
    }

    #[tokio::test]
    async fn fts_keyword_search() {
        let db = crate::test_support::temp_db_path("history-fts");
        let store = HistoryStore::new(db);
        let session_id = Uuid::new_v4();

        store
            .append(&Message {
                id: Uuid::new_v4(),
                session_id,
                role: Role::User,
                content: "Rust programming language".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        store
            .append(&Message {
                id: Uuid::new_v4(),
                session_id,
                role: Role::User,
                content: "Python scripting".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        let found = store.search("Rust", 10).await.unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].content.contains("Rust"));
    }

    #[tokio::test]
    async fn fts_cjk_text_search() {
        let db = crate::test_support::temp_db_path("history-fts-cjk");
        let store = HistoryStore::new(db);
        let session_id = Uuid::new_v4();

        store
            .append(&Message {
                id: Uuid::new_v4(),
                session_id,
                role: Role::User,
                content: "数据库优化技巧".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        store
            .append(&Message {
                id: Uuid::new_v4(),
                session_id,
                role: Role::User,
                content: "网络编程基础".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        // FTS5 unicode61 tokenizer handles CJK
        let found = store.search("数据库优化技巧", 10).await.unwrap();
        assert!(!found.is_empty());
    }

    #[tokio::test]
    async fn paginated_returns_subset() {
        let db = crate::test_support::temp_db_path("history-paginated");
        let store = HistoryStore::new(db);
        let session_id = Uuid::new_v4();

        for idx in 0..10 {
            store
                .append(&Message {
                    id: Uuid::new_v4(),
                    session_id,
                    role: Role::User,
                    content: format!("message {}", idx),
                    timestamp: Utc::now(),
                })
                .await
                .unwrap();
        }

        let page1 = store
            .get_session_messages_paginated(session_id, 0, 3)
            .await
            .unwrap();
        assert_eq!(page1.len(), 3);

        let page2 = store
            .get_session_messages_paginated(session_id, 3, 3)
            .await
            .unwrap();
        assert_eq!(page2.len(), 3);

        // 不同页不重叠
        assert_ne!(page1[0].content, page2[0].content);

        // 超出范围返回空
        let page_end = store
            .get_session_messages_paginated(session_id, 100, 3)
            .await
            .unwrap();
        assert!(page_end.is_empty());
    }

    #[tokio::test]
    async fn backfill_migration_indexes_existing_data() {
        let db_path = crate::test_support::temp_db_path("history-backfill");
        let session_id = Uuid::new_v4();

        // 模拟旧数据：直接写入 messages 表（不经过 append，所以 FTS 未同步）
        {
            let db = crate::storage::Database::open(db_path.clone()).unwrap();
            db.execute_batch(
                "CREATE TABLE IF NOT EXISTS messages (
                    id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    timestamp_ms INTEGER NOT NULL
                );",
            )
            .unwrap();
            let id = Uuid::new_v4().to_string();
            let sid = session_id.to_string();
            db.execute(
                "INSERT INTO messages (id, session_id, role, content, timestamp_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                &[&id, &sid, &"user", &"legacy data about Rust ownership", &1_i64],
            ).unwrap();
        }

        // 创建 HistoryStore 触发 init_schema + backfill
        let store = HistoryStore::new(db_path);
        let found = store.search("Rust", 10).await.unwrap();
        // 回填后 FTS 应能搜到旧数据
        assert!(!found.is_empty());
        assert!(found[0].content.contains("Rust"));
    }
}
