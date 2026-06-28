//! Token usage 持久化存储

use crate::providers::traits::TokenUsage;
use crate::storage::Database;
use chrono::Utc;
use uuid::Uuid;

pub struct UsageStore {
    db: Database,
}

impl UsageStore {
    pub fn new(db_path: std::path::PathBuf) -> anyhow::Result<Self> {
        let db = Database::open(db_path)?;
        let store = Self { db };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS token_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt_tokens INTEGER NOT NULL,
                completion_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_token_usage_session
              ON token_usage(session_id, created_at_ms DESC);
            ",
        )
    }

    pub fn record(
        &self,
        session_id: Uuid,
        provider: &str,
        model: &str,
        usage: &TokenUsage,
    ) -> anyhow::Result<()> {
        let sid = session_id.to_string();
        let prompt = usage.prompt_tokens as i64;
        let completion = usage.completion_tokens as i64;
        let total = usage.total_tokens as i64;
        let now = Utc::now().timestamp_millis();

        self.db.execute(
            "INSERT INTO token_usage (
                session_id, provider, model,
                prompt_tokens, completion_tokens, total_tokens, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[&sid, &provider, &model, &prompt, &completion, &total, &now],
        )?;
        Ok(())
    }

    pub fn aggregate_for_session(&self, session_id: Uuid) -> anyhow::Result<Option<TokenUsage>> {
        let sid = session_id.to_string();
        let rows = self.db.query_map(
            "SELECT
                COALESCE(SUM(prompt_tokens), 0),
                COALESCE(SUM(completion_tokens), 0),
                COALESCE(SUM(total_tokens), 0)
             FROM token_usage
             WHERE session_id = ?1",
            &[&sid],
            |row| {
                Ok(TokenUsage {
                    prompt_tokens: row.get::<_, i64>(0)? as usize,
                    completion_tokens: row.get::<_, i64>(1)? as usize,
                    total_tokens: row.get::<_, i64>(2)? as usize,
                })
            },
        )?;

        let Some(usage) = rows.first() else {
            return Ok(None);
        };
        if usage.total_tokens == 0 {
            return Ok(None);
        }
        Ok(Some(usage.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_aggregate_usage() {
        let db = crate::test_support::temp_db_path("usage");
        let store = UsageStore::new(db).unwrap();
        let sid = Uuid::new_v4();

        store
            .record(
                sid,
                "openai",
                "gpt-4o-mini",
                &TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                },
            )
            .unwrap();

        store
            .record(
                sid,
                "openai",
                "gpt-4o-mini",
                &TokenUsage {
                    prompt_tokens: 3,
                    completion_tokens: 2,
                    total_tokens: 5,
                },
            )
            .unwrap();

        let usage = store.aggregate_for_session(sid).unwrap().unwrap();
        assert_eq!(usage.prompt_tokens, 13);
        assert_eq!(usage.completion_tokens, 7);
        assert_eq!(usage.total_tokens, 20);
    }
}
