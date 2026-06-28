//! SQLite 数据库管理
//!
//! 封装 rusqlite，提供统一的数据库访问接口。

use anyhow::Context;
use rusqlite::{Connection, ToSql};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// 数据库管理器
pub struct Database {
    /// 数据库文件路径
    path: PathBuf,
    /// 共享连接
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// 创建或打开数据库
    pub fn open(path: PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create db parent: {}", parent.display()))?;
        }

        let conn = Connection::open(&path)
            .with_context(|| format!("failed to open sqlite db: {}", path.display()))?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            ",
        )?;

        Ok(Self {
            path,
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 数据库路径
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// 初始化数据库表结构
    pub fn init_schema(&self) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("sqlite connection lock poisoned"))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                state TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                last_active_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session_ts
              ON messages(session_id, timestamp_ms);
            CREATE INDEX IF NOT EXISTS idx_messages_content
              ON messages(content);
            ",
        )?;

        Ok(())
    }

    /// 执行更新
    pub fn execute(&self, sql: &str, params: &[&dyn ToSql]) -> anyhow::Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("sqlite connection lock poisoned"))?;
        Ok(conn.execute(sql, params)?)
    }

    /// 执行批量 SQL（DDL/多语句）
    pub fn execute_batch(&self, sql: &str) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("sqlite connection lock poisoned"))?;
        conn.execute_batch(sql)?;
        Ok(())
    }

    /// 查询并映射结果
    pub fn query_map<T, F>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        map: F,
    ) -> anyhow::Result<Vec<T>>
    where
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("sqlite connection lock poisoned"))?;
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, map)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// 开启事务
    pub fn transaction<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&Connection) -> anyhow::Result<R>,
    {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("sqlite connection lock poisoned"))?;
        let tx = conn.transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_init_schema() {
        let path = crate::test_support::temp_db_path("prometheus-db");
        let db = Database::open(path).unwrap();
        db.init_schema().unwrap();

        let tables = db
            .query_map(
                "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('sessions','messages','memories')",
                &[],
                |row| row.get::<_, String>(0),
            )
            .unwrap();

        assert_eq!(tables.len(), 3);
    }

    #[test]
    fn execute_and_query_roundtrip() {
        let path = crate::test_support::temp_db_path("prometheus-db");
        let db = Database::open(path).unwrap();
        db.init_schema().unwrap();

        let session_id = uuid::Uuid::new_v4().to_string();
        db.execute(
            "INSERT INTO sessions (id, title, state, created_at_ms, last_active_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            &[&session_id, &"demo", &"active", &1_i64, &1_i64],
        )
        .unwrap();

        let rows = db
            .query_map(
                "SELECT title FROM sessions WHERE id = ?1",
                &[&session_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap();

        assert_eq!(rows, vec!["demo".to_string()]);
    }

    #[test]
    fn transaction_rolls_back_on_error() {
        let path = crate::test_support::temp_db_path("prometheus-db");
        let db = Database::open(path).unwrap();
        db.init_schema().unwrap();

        let res: anyhow::Result<()> = db.transaction(|conn| {
            conn.execute(
                "INSERT INTO sessions (id, title, state, created_at_ms, last_active_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["id-1", "title", "active", 1_i64, 1_i64],
            )?;
            anyhow::bail!("force rollback")
        });
        assert!(res.is_err());

        let count = db
            .query_map("SELECT COUNT(*) FROM sessions", &[], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(count[0], 0);
    }
}
