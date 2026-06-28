//! L0 用户画像
//!
//! 长期存储用户偏好、习惯、个人信息，构建持久化的用户模型。

use crate::storage::db::Database;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 用户画像
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// 用户 ID
    pub user_id: String,
    /// 显示名称
    pub display_name: String,
    /// 语言偏好
    pub language: String,
    /// 偏好设置（键值对）
    pub preferences: HashMap<String, String>,
    /// 兴趣标签
    pub interests: Vec<String>,
}

impl UserProfile {
    /// 创建默认用户画像
    pub fn new(user_id: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            display_name: String::new(),
            language: "zh-CN".to_string(),
            preferences: HashMap::new(),
            interests: Vec::new(),
        }
    }

    /// 从对话中自动提取并更新画像
    pub async fn update_from_conversation(&mut self, messages: &[String]) -> anyhow::Result<()> {
        for msg in messages {
            let lower = msg.to_lowercase();
            // 语言偏好检测
            if lower.contains("english") || lower.contains("英文") {
                self.preferences.insert("language_pref".into(), "en".into());
            }
            // 技术栈检测
            for tech in &["rust", "python", "javascript", "typescript", "go", "java"] {
                if lower.contains(tech) && !self.interests.contains(&tech.to_string()) {
                    self.interests.push(tech.to_string());
                }
            }
            // 话题兴趣检测
            for topic in &["ai", "机器学习", "web", "数据库", "云计算", "安全"] {
                if lower.contains(topic) && !self.interests.contains(&topic.to_string()) {
                    self.interests.push(topic.to_string());
                }
            }
        }
        Ok(())
    }

    /// 持久化到 SQLite
    pub async fn save(&self, db: &Database) -> anyhow::Result<()> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS user_profiles (
                user_id TEXT PRIMARY KEY,
                data_json TEXT NOT NULL
            )",
        )?;
        let json = serde_json::to_string(self)?;
        db.execute(
            "INSERT OR REPLACE INTO user_profiles (user_id, data_json) VALUES (?1, ?2)",
            &[&self.user_id, &json],
        )?;
        Ok(())
    }

    /// 从 SQLite 加载
    pub fn load(db: &Database, user_id: &str) -> anyhow::Result<Self> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS user_profiles (
                user_id TEXT PRIMARY KEY,
                data_json TEXT NOT NULL
            )",
        )?;
        let rows = db.query_map(
            "SELECT data_json FROM user_profiles WHERE user_id = ?1",
            &[&user_id],
            |row| row.get::<_, String>(0),
        )?;
        match rows.into_iter().next() {
            Some(json) => Ok(serde_json::from_str(&json)?),
            None => Ok(Self::new(user_id)),
        }
    }

    /// 合并两个画像
    pub fn merge(&self, other: &UserProfile) -> UserProfile {
        let mut merged = self.clone();
        for (k, v) in &other.preferences {
            merged
                .preferences
                .entry(k.clone())
                .or_insert_with(|| v.clone());
        }
        for interest in &other.interests {
            if !merged.interests.contains(interest) {
                merged.interests.push(interest.clone());
            }
        }
        if merged.display_name.is_empty() && !other.display_name.is_empty() {
            merged.display_name = other.display_name.clone();
        }
        merged
    }

    /// 画像相似度 (Jaccard on interests)
    pub fn similarity(&self, other: &UserProfile) -> f32 {
        if self.interests.is_empty() && other.interests.is_empty() {
            return 1.0;
        }
        let set_a: std::collections::HashSet<&str> =
            self.interests.iter().map(|s| s.as_str()).collect();
        let set_b: std::collections::HashSet<&str> =
            other.interests.iter().map(|s| s.as_str()).collect();
        let intersection = set_a.intersection(&set_b).count() as f32;
        let union = set_a.union(&set_b).count() as f32;
        if union == 0.0 {
            0.0
        } else {
            intersection / union
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db() -> Database {
        let path = PathBuf::from(format!(
            "/tmp/prometheus-profile-{}.db",
            uuid::Uuid::new_v4()
        ));
        Database::open(path).unwrap()
    }

    #[tokio::test]
    async fn save_load_roundtrip() {
        let db = temp_db();
        let mut profile = UserProfile::new("user-1");
        profile.display_name = "Alice".into();
        profile.interests.push("rust".into());
        profile.save(&db).await.unwrap();

        let loaded = UserProfile::load(&db, "user-1").unwrap();
        assert_eq!(loaded.display_name, "Alice");
        assert_eq!(loaded.interests, vec!["rust"]);
    }

    #[tokio::test]
    async fn update_from_conversation_extracts() {
        let mut profile = UserProfile::new("user-1");
        let messages = vec![
            "I love Rust programming".to_string(),
            "Also interested in AI and 机器学习".to_string(),
        ];
        profile.update_from_conversation(&messages).await.unwrap();
        assert!(profile.interests.contains(&"rust".to_string()));
        assert!(profile.interests.contains(&"ai".to_string()));
        assert!(profile.interests.contains(&"机器学习".to_string()));
    }

    #[tokio::test]
    async fn empty_messages_no_change() {
        let mut profile = UserProfile::new("user-1");
        profile.update_from_conversation(&[]).await.unwrap();
        assert!(profile.interests.is_empty());
    }

    #[test]
    fn merge_profiles() {
        let mut a = UserProfile::new("user-1");
        a.interests.push("rust".into());
        let mut b = UserProfile::new("user-1");
        b.interests.push("python".into());
        b.display_name = "Bob".into();
        let merged = a.merge(&b);
        assert!(merged.interests.contains(&"rust".to_string()));
        assert!(merged.interests.contains(&"python".to_string()));
        assert_eq!(merged.display_name, "Bob");
    }

    #[test]
    fn similarity_score() {
        let mut a = UserProfile::new("a");
        a.interests = vec!["rust".into(), "python".into(), "ai".into()];
        let mut b = UserProfile::new("b");
        b.interests = vec!["rust".into(), "go".into(), "ai".into()];
        let sim = a.similarity(&b);
        assert!(sim > 0.4 && sim < 0.6); // 2/4 = 0.5
    }

    #[test]
    fn default_values() {
        let p = UserProfile::new("test");
        assert_eq!(p.language, "zh-CN");
        assert!(p.display_name.is_empty());
        assert!(p.preferences.is_empty());
    }
}
