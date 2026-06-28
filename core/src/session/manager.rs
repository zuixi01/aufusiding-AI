//! 会话管理器
//!
//! 负责创建、恢复、切换和销毁对话会话，维护活跃会话池。

use crate::storage::Database;
use anyhow::Context;
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 会话状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionState {
    Active,
    Paused,
    Archived,
}

/// 对话会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// 会话唯一 ID
    pub id: Uuid,
    /// 会话标题（自动生成或用户自定义）
    pub title: String,
    /// 会话状态
    pub state: SessionState,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后活跃时间
    pub last_active: DateTime<Utc>,
}

/// 会话管理器
pub struct SessionManager {
    /// 活跃会话列表
    active_sessions: Vec<Session>,
    /// 可选 SQLite 持久化实例
    db: Option<Database>,
}

impl SessionManager {
    /// 创建会话管理器（仅内存）
    pub fn new() -> Self {
        Self {
            active_sessions: Vec::new(),
            db: None,
        }
    }

    /// 创建会话管理器（SQLite 持久化）
    pub fn with_db_path(db_path: std::path::PathBuf) -> anyhow::Result<Self> {
        let db = Database::open(db_path)?;
        db.init_schema()?;

        let mut manager = Self {
            active_sessions: Vec::new(),
            db: Some(db),
        };
        manager.load_from_db()?;
        Ok(manager)
    }

    fn load_from_db(&mut self) -> anyhow::Result<()> {
        let Some(db) = &self.db else {
            return Ok(());
        };

        let rows = db.query_map(
            "SELECT id, title, state, created_at_ms, last_active_ms
             FROM sessions
             ORDER BY last_active_ms DESC",
            &[],
            |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let state: String = row.get(2)?;
                let created_at_ms: i64 = row.get(3)?;
                let last_active_ms: i64 = row.get(4)?;

                Ok(Session {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()),
                    title,
                    state: str_to_state(&state),
                    created_at: Utc
                        .timestamp_millis_opt(created_at_ms)
                        .single()
                        .unwrap_or_else(Utc::now),
                    last_active: Utc
                        .timestamp_millis_opt(last_active_ms)
                        .single()
                        .unwrap_or_else(Utc::now),
                })
            },
        )?;

        self.active_sessions.clear();
        for row in rows {
            self.active_sessions.push(row);
        }
        Ok(())
    }

    fn persist_session(&self, session: &Session) -> anyhow::Result<()> {
        let Some(db) = &self.db else {
            return Ok(());
        };

        let id = session.id.to_string();
        let state = state_to_str(&session.state);
        let created_at_ms = session.created_at.timestamp_millis();
        let last_active_ms = session.last_active.timestamp_millis();

        db.execute(
            "INSERT OR REPLACE INTO sessions (id, title, state, created_at_ms, last_active_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            &[&id, &session.title, &state, &created_at_ms, &last_active_ms],
        )?;
        Ok(())
    }

    /// 创建新会话
    pub async fn create_session(&mut self, title: Option<&str>) -> anyhow::Result<Session> {
        let now = Utc::now();
        let session = Session {
            id: Uuid::new_v4(),
            title: title.unwrap_or("新对话").to_string(),
            state: SessionState::Active,
            created_at: now,
            last_active: now,
        };

        self.active_sessions.push(session.clone());
        self.persist_session(&session)?;
        Ok(session)
    }

    /// 恢复已有会话
    pub async fn resume_session(&mut self, id: Uuid) -> anyhow::Result<&Session> {
        let idx = self
            .active_sessions
            .iter()
            .position(|s| s.id == id)
            .with_context(|| format!("session not found: {id}"))?;

        self.active_sessions[idx].state = SessionState::Active;
        self.active_sessions[idx].last_active = Utc::now();
        let clone = self.active_sessions[idx].clone();
        self.persist_session(&clone)?;
        Ok(&self.active_sessions[idx])
    }

    /// 归档会话
    pub async fn archive_session(&mut self, id: Uuid) -> anyhow::Result<()> {
        let idx = self
            .active_sessions
            .iter()
            .position(|s| s.id == id)
            .with_context(|| format!("session not found: {id}"))?;

        self.active_sessions[idx].state = SessionState::Archived;
        self.active_sessions[idx].last_active = Utc::now();
        let clone = self.active_sessions[idx].clone();
        self.persist_session(&clone)?;
        Ok(())
    }

    /// 返回当前活跃会话
    pub fn sessions(&self) -> &[Session] {
        &self.active_sessions
    }
}

fn state_to_str(state: &SessionState) -> &'static str {
    match state {
        SessionState::Active => "active",
        SessionState::Paused => "paused",
        SessionState::Archived => "archived",
    }
}

fn str_to_state(state: &str) -> SessionState {
    match state {
        "paused" => SessionState::Paused,
        "archived" => SessionState::Archived,
        _ => SessionState::Active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_archive_session() {
        let mut manager = SessionManager::new();
        let created = manager.create_session(Some("测试会话")).await.unwrap();
        assert_eq!(manager.sessions().len(), 1);

        manager.archive_session(created.id).await.unwrap();
        assert_eq!(manager.sessions()[0].state, SessionState::Archived);
    }

    #[tokio::test]
    async fn persists_sessions_across_instances() {
        let db = crate::test_support::temp_db_path("session");

        let mut a = SessionManager::with_db_path(db.clone()).unwrap();
        let created = a.create_session(Some("持久会话")).await.unwrap();
        drop(a);

        let b = SessionManager::with_db_path(db).unwrap();
        assert_eq!(b.sessions().len(), 1);
        assert_eq!(b.sessions()[0].id, created.id);
        assert_eq!(b.sessions()[0].title, "持久会话");
    }

    #[tokio::test]
    async fn updates_state_in_database() {
        let db = crate::test_support::temp_db_path("session");

        let mut manager = SessionManager::with_db_path(db.clone()).unwrap();
        let created = manager.create_session(Some("状态测试")).await.unwrap();
        manager.archive_session(created.id).await.unwrap();
        drop(manager);

        let reloaded = SessionManager::with_db_path(db).unwrap();
        assert_eq!(reloaded.sessions()[0].state, SessionState::Archived);
    }

    #[tokio::test]
    async fn resume_nonexistent_session_errors() {
        let mut manager = SessionManager::new();
        let err = manager.resume_session(Uuid::new_v4()).await.unwrap_err();
        assert!(err.to_string().contains("session not found"));
    }

    #[tokio::test]
    async fn archive_already_archived_session() {
        let mut manager = SessionManager::new();
        let session = manager.create_session(Some("test")).await.unwrap();
        manager.archive_session(session.id).await.unwrap();
        // archiving again should still succeed (idempotent state set)
        manager.archive_session(session.id).await.unwrap();
        assert_eq!(manager.sessions()[0].state, SessionState::Archived);
    }

    #[tokio::test]
    async fn create_multiple_sessions() {
        let mut manager = SessionManager::new();
        for i in 0..5 {
            manager
                .create_session(Some(&format!("session-{i}")))
                .await
                .unwrap();
        }
        assert_eq!(manager.sessions().len(), 5);
    }

    #[tokio::test]
    async fn persistence_roundtrip() {
        let db = crate::test_support::temp_db_path("session-roundtrip");

        let mut manager = SessionManager::with_db_path(db.clone()).unwrap();
        let s1 = manager.create_session(Some("first")).await.unwrap();
        let s2 = manager.create_session(Some("second")).await.unwrap();
        manager.archive_session(s1.id).await.unwrap();
        drop(manager);

        let reloaded = SessionManager::with_db_path(db).unwrap();
        assert_eq!(reloaded.sessions().len(), 2);
        let archived = reloaded.sessions().iter().find(|s| s.id == s1.id).unwrap();
        assert_eq!(archived.state, SessionState::Archived);
        let active = reloaded.sessions().iter().find(|s| s.id == s2.id).unwrap();
        assert_eq!(active.state, SessionState::Active);
    }
}
