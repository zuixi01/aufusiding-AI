//! 规则持久化存储
//!
//! SQLite 持久化自动化规则，支持跨重启恢复。

use crate::storage::db::Database;
use std::path::Path;

use super::automation::{AutomationEngine, AutomationRule};

/// 规则持久化存储（SQLite）
pub struct RuleStore {
    db: Database,
}

impl RuleStore {
    pub fn new(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let db = Database::open(db_path.as_ref().to_path_buf())?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS automation_rules (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                data TEXT NOT NULL
            )",
        )?;
        Ok(Self { db })
    }

    /// 保存规则（INSERT OR REPLACE）
    pub fn save(&self, rule: &AutomationRule) -> anyhow::Result<()> {
        let data = serde_json::to_string(rule)?;
        self.db.execute(
            "INSERT OR REPLACE INTO automation_rules (id, name, enabled, data) VALUES (?1, ?2, ?3, ?4)",
            &[&rule.id, &rule.name, &rule.enabled, &data],
        )?;
        Ok(())
    }

    /// 删除规则
    pub fn delete(&self, rule_id: &str) -> anyhow::Result<bool> {
        let affected = self
            .db
            .execute("DELETE FROM automation_rules WHERE id = ?1", &[&rule_id])?;
        Ok(affected > 0)
    }

    /// 加载所有规则
    pub fn load_all(&self) -> anyhow::Result<Vec<AutomationRule>> {
        let data_strings: Vec<String> = self.db.query_map(
            "SELECT data FROM automation_rules ORDER BY id",
            &[],
            |row| row.get(0),
        )?;
        let mut rules = Vec::new();
        for data in data_strings {
            let rule: AutomationRule = serde_json::from_str(&data)?;
            rules.push(rule);
        }
        Ok(rules)
    }

    /// 从引擎导出并保存所有规则
    pub async fn sync_from_engine(&self, engine: &AutomationEngine) -> anyhow::Result<()> {
        let rules = engine.list_rules().await;
        for rule in &rules {
            self.save(rule)?;
        }
        Ok(())
    }

    /// 加载规则并导入引擎
    pub async fn sync_to_engine(&self, engine: &AutomationEngine) -> anyhow::Result<()> {
        let rules = self.load_all()?;
        for rule in rules {
            // Skip duplicates silently
            let _ = engine.add_rule(rule).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::automation::*;
    use crate::iot::event_bus::{EventBus, EventKind};
    use std::sync::Arc;

    fn temp_store() -> RuleStore {
        let path = format!("/tmp/prometheus-rules-{}.db", uuid::Uuid::new_v4());
        RuleStore::new(&path).unwrap()
    }

    fn sample_rule(id: &str) -> AutomationRule {
        AutomationRule {
            id: id.to_string(),
            name: format!("Rule {}", id),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "sensor-001".into(),
                event_kind: EventKind::StateChanged,
            }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "test".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: Some(60),
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let store = temp_store();
        let rule = sample_rule("r1");
        store.save(&rule).unwrap();
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "r1");
        assert_eq!(loaded[0].name, "Rule r1");
    }

    #[test]
    fn delete_rule() {
        let store = temp_store();
        store.save(&sample_rule("r1")).unwrap();
        assert!(store.delete("r1").unwrap());
        assert!(store.load_all().unwrap().is_empty());
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let store = temp_store();
        assert!(!store.delete("no-such").unwrap());
    }

    #[test]
    fn load_all_multiple() {
        let store = temp_store();
        store.save(&sample_rule("a")).unwrap();
        store.save(&sample_rule("b")).unwrap();
        store.save(&sample_rule("c")).unwrap();
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 3);
    }

    #[test]
    fn persistence_across_instances() {
        let path = format!("/tmp/prometheus-rules-{}.db", uuid::Uuid::new_v4());
        {
            let store = RuleStore::new(&path).unwrap();
            store.save(&sample_rule("r1")).unwrap();
        }
        let store2 = RuleStore::new(&path).unwrap();
        let loaded = store2.load_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "r1");
    }

    #[test]
    fn complex_rule_serialization() {
        let store = temp_store();
        let rule = AutomationRule {
            id: "complex".into(),
            name: "Complex rule".into(),
            enabled: true,
            triggers: vec![Trigger::Threshold {
                device_id: "s1".into(),
                property: "temp".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            }],
            conditions: vec![Condition::And(vec![
                Condition::DeviceState {
                    device_id: "s1".into(),
                    property: "temp".into(),
                    op: CompareOp::Gt,
                    value: serde_json::json!(25),
                },
                Condition::Not(Box::new(Condition::DaysOfWeek { days: vec![0, 6] })),
            ])],
            actions: vec![Action::Sequence(vec![
                Action::DeviceCommand {
                    device_id: "ac-001".into(),
                    command: "turn_on".into(),
                    params: serde_json::json!({}),
                },
                Action::Delay {
                    duration_ms: 1000,
                    then: Box::new(Action::Notify {
                        message: "done".into(),
                        level: NotifyLevel::Warning,
                    }),
                },
            ])],
            last_triggered: None,
            cooldown_secs: Some(300),
        };
        store.save(&rule).unwrap();
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded[0].id, "complex");
        assert_eq!(loaded[0].conditions.len(), 1);
    }

    #[tokio::test]
    async fn sync_from_engine() {
        let store = temp_store();
        let bus = Arc::new(EventBus::new(16));
        let engine = AutomationEngine::new(bus);
        engine.add_rule(sample_rule("r1")).await.unwrap();
        engine.add_rule(sample_rule("r2")).await.unwrap();

        store.sync_from_engine(&engine).await.unwrap();
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[tokio::test]
    async fn sync_to_engine() {
        let store = temp_store();
        store.save(&sample_rule("r1")).unwrap();
        store.save(&sample_rule("r2")).unwrap();

        let bus = Arc::new(EventBus::new(16));
        let engine = AutomationEngine::new(bus);
        store.sync_to_engine(&engine).await.unwrap();

        let rules = engine.list_rules().await;
        assert_eq!(rules.len(), 2);
    }
}
