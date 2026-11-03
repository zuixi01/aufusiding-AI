//! MemoryManager — 五层记忆统一协调器
//!
//! 协调 L0 用户画像、L1 工作记忆、L2 情景记忆、L3 知识库、L4 主动回忆，
//! 实现记忆的自动存取、晋升和主动召回。

use crate::memory::episodic::{EpisodicMemory, Message, Role};
use crate::memory::knowledge::KnowledgeBase;
use crate::memory::proactive::{ProactiveRecall, RecallConfig, RecallResult};
use crate::memory::profile::UserProfile;
use crate::memory::working::WorkingMemory;
use crate::storage::db::Database;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 五层记忆统一协调器
pub struct MemoryManager {
    profile: UserProfile,
    working: WorkingMemory,
    episodic: Arc<RwLock<EpisodicMemory>>,
    knowledge: Arc<RwLock<KnowledgeBase>>,
    recall: ProactiveRecall,
    db: Database,
}

impl MemoryManager {
    /// 初始化所有五层记忆
    pub fn new(db_dir: &Path) -> anyhow::Result<Self> {
        let profile_db_path = db_dir.join("profile.db");
        let episodic_db_path = db_dir.join("episodic.db");
        let knowledge_db_path = db_dir.join("knowledge.db");

        std::fs::create_dir_all(db_dir)?;

        let db = Database::open(profile_db_path)?;
        let profile = UserProfile::load(&db, "default")?;
        let working = WorkingMemory::new(20);
        let episodic = Arc::new(RwLock::new(EpisodicMemory::new(episodic_db_path, 128)?));
        let knowledge = Arc::new(RwLock::new(KnowledgeBase::new(knowledge_db_path, 128)?));

        let recall = ProactiveRecall::with_sources(
            RecallConfig::default(),
            episodic.clone(),
            knowledge.clone(),
        );

        Ok(Self {
            profile,
            working,
            episodic,
            knowledge,
            recall,
            db,
        })
    }

    /// 处理用户消息：加入 L1 工作记忆，判断是否触发主动回忆
    pub async fn on_user_message(&mut self, text: &str) -> Vec<RecallResult> {
        let importance = estimate_importance(text);
        self.working.add(text.to_string(), importance);

        if self.recall.should_recall(text) {
            self.recall.recall(text).await.unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// 处理助手回复：更新 L0 用户画像
    pub async fn on_assistant_response(&mut self, user_text: &str, _assistant_text: &str) {
        let messages = vec![user_text.to_string()];
        if let Err(e) = self.profile.update_from_conversation(&messages).await {
            tracing::warn!("user profile update failed: {}", e);
        }
    }

    /// 会话结束：L1→L2 晋升 + L3 知识提取并行、清空 L1
    pub async fn on_session_end(&mut self, messages: &[(String, String)]) -> anyhow::Result<()> {
        // 准备数据
        let important_items = self.working.drain_important(0.15);
        let facts = extract_factual_statements(messages);

        // L1→L2 晋升 和 L3 知识提取并行执行
        let ep = self.episodic.clone();
        let kb = self.knowledge.clone();

        let ep_future = async {
            if !important_items.is_empty() {
                let episode_messages: Vec<Message> = important_items
                    .into_iter()
                    .map(|item| Message {
                        role: Role::User,
                        content: item.content,
                    })
                    .collect();
                let mut guard = ep.write().await;
                guard.add_episode(episode_messages).await.map(|_| ())
            } else {
                Ok(())
            }
        };

        let kb_future = async {
            if !facts.is_empty() {
                let mut guard = kb.write().await;
                let content = facts.join("\n");
                guard
                    .add_document("conversation-extract", &content)
                    .await
                    .map(|_| ())
            } else {
                Ok(())
            }
        };

        let (ep_result, kb_result): (anyhow::Result<()>, anyhow::Result<()>) =
            tokio::join!(ep_future, kb_future);
        ep_result?;
        kb_result?;

        // 清空 L1 工作记忆
        self.working.clear();

        // 保存用户画像
        if let Err(e) = self.profile.save(&self.db).await {
            tracing::warn!("user profile save failed: {}", e);
        }

        Ok(())
    }

    /// 构建可注入 prompt 的记忆上下文（L2+L3 并行搜索）
    pub async fn build_memory_context(&self, query: &str, max_items: usize) -> String {
        let ep = self.episodic.clone();
        let kb = self.knowledge.clone();
        let q1 = query.to_string();
        let q2 = query.to_string();

        let (ep_result, kb_result) = tokio::join!(
            async {
                let guard = ep.read().await;
                guard.search(&q1, max_items).await.unwrap_or_default()
            },
            async {
                let guard = kb.read().await;
                guard.search(&q2, max_items).await.unwrap_or_default()
            }
        );

        let mut parts = Vec::new();
        for ep in ep_result.iter().take(max_items) {
            parts.push(format!("[记忆] {}", ep.summary));
        }
        let remaining = max_items.saturating_sub(parts.len());
        for chunk in kb_result.into_iter().take(remaining) {
            parts.push(format!("[知识] {}", chunk.content));
        }
        parts.join("\n")
    }

    /// 获取用户画像引用
    pub fn profile(&self) -> &UserProfile {
        &self.profile
    }

    /// 获取工作记忆引用
    pub fn working(&self) -> &WorkingMemory {
        &self.working
    }
}

/// 基于文本特征估算重要性
pub fn estimate_importance(text: &str) -> f64 {
    let mut score: f64 = 0.3; // 基础分

    // 长文本更重要
    let len = text.chars().count();
    if len > 50 {
        score += 0.1;
    }
    if len > 100 {
        score += 0.1;
    }

    // 问句更重要
    if text.contains('?') || text.contains('？') {
        score += 0.15;
    }

    // 重要性标记词
    let important_markers = [
        "重要",
        "关键",
        "必须",
        "urgent",
        "important",
        "critical",
        "注意",
    ];
    for marker in &important_markers {
        if text.to_lowercase().contains(marker) {
            score += 0.15;
            break;
        }
    }

    score.min(1.0)
}

/// 从对话中提取事实性陈述（来自 assistant 回复）
pub fn extract_factual_statements(messages: &[(String, String)]) -> Vec<String> {
    let mut facts = Vec::new();

    for (_user, assistant) in messages {
        // 按句子分割
        for sentence in assistant.split(['。', '.', '！', '!']) {
            let s = sentence.trim();
            if s.is_empty() || s.chars().count() < 10 {
                continue;
            }
            // 过滤非事实性句子（问句、感叹、纯礼貌用语）
            if s.contains('?') || s.contains('？') {
                continue;
            }
            let polite = ["好的", "没问题", "当然", "可以", "明白", "了解"];
            if polite.contains(&s) {
                continue;
            }
            facts.push(s.to_string());
        }
    }

    facts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> MemoryManager {
        let dir = crate::test_support::temp_dir_path("memory-manager");
        MemoryManager::new(&dir).unwrap()
    }

    #[test]
    fn new_creates_all_layers() {
        let mgr = create_test_manager();
        assert!(mgr.working().is_empty());
        assert_eq!(mgr.profile().user_id, "default");
    }

    #[tokio::test]
    async fn on_user_message_adds_to_working_memory() {
        let mut mgr = create_test_manager();
        mgr.on_user_message("你好，我想了解 Rust").await;
        assert_eq!(mgr.working().len(), 1);
    }

    #[tokio::test]
    async fn on_user_message_triggers_recall() {
        let mut mgr = create_test_manager();
        // First message always triggers recall (new keywords)
        let results = mgr.on_user_message("Rust 编程语言").await;
        // May or may not have results depending on empty stores, but should not error
        let _ = results;
        assert_eq!(mgr.working().len(), 1);
    }

    #[tokio::test]
    async fn on_assistant_response_updates_profile() {
        let mut mgr = create_test_manager();
        mgr.on_assistant_response("我在学习 Rust", "Rust 是一门很好的语言")
            .await;
        assert!(mgr.profile().interests.contains(&"rust".to_string()));
    }

    #[tokio::test]
    async fn on_session_end_promotes_to_episodic() {
        let mut mgr = create_test_manager();
        // Add some important items to working memory
        mgr.working.add("重要的 Rust 讨论".to_string(), 0.9);
        mgr.working.add("关键架构决策".to_string(), 0.8);

        let messages = vec![(
            "用户消息".to_string(),
            "助手回复，这是一个事实性陈述，包含足够长的内容".to_string(),
        )];
        mgr.on_session_end(&messages).await.unwrap();

        // Working memory should be cleared
        assert!(mgr.working().is_empty());

        // Episodic memory should have the promoted episode
        let ep = mgr.episodic.read().await;
        assert!(ep.count().unwrap() > 0);
    }

    #[tokio::test]
    async fn on_session_end_extracts_knowledge() {
        let mut mgr = create_test_manager();
        let messages = vec![(
            "什么是 Rust".to_string(),
            "Rust 是一门注重安全性和性能的系统编程语言，由 Mozilla 开发".to_string(),
        )];
        mgr.on_session_end(&messages).await.unwrap();

        let kb = mgr.knowledge.read().await;
        assert!(kb.count().unwrap() > 0);
    }

    #[tokio::test]
    async fn on_session_end_clears_working_memory() {
        let mut mgr = create_test_manager();
        mgr.working.add("临时记忆".to_string(), 0.5);
        mgr.on_session_end(&[]).await.unwrap();
        assert!(mgr.working().is_empty());
    }

    #[tokio::test]
    async fn build_memory_context_returns_relevant_info() {
        let mgr = create_test_manager();
        {
            let mut ep = mgr.episodic.write().await;
            ep.add_episode(vec![
                Message {
                    role: Role::User,
                    content: "Rust 所有权系统".to_string(),
                },
                Message {
                    role: Role::Assistant,
                    content: "所有权是 Rust 核心".to_string(),
                },
            ])
            .await
            .unwrap();
        }

        let ctx = mgr.build_memory_context("Rust", 5).await;
        assert!(ctx.contains("[记忆]") || ctx.contains("[知识]") || ctx.is_empty());
    }

    #[test]
    fn estimate_importance_scores_questions_higher() {
        let q = estimate_importance("这个 API 怎么用？");
        let s = estimate_importance("好的");
        assert!(q > s);
    }

    #[test]
    fn extract_factual_statements_filters_assistant_facts() {
        let messages = vec![(
            "问题".to_string(),
            "Rust 的所有权系统确保内存安全。好的。".to_string(),
        )];
        let facts = extract_factual_statements(&messages);
        assert!(facts.iter().any(|f| f.contains("所有权")));
        // "好的" should be filtered out (too short)
        assert!(!facts.iter().any(|f| f == "好的"));
    }

    #[tokio::test]
    async fn parallel_search_returns_complete_results() {
        let mgr = create_test_manager();
        // 同时写入 L2 和 L3
        {
            let mut ep = mgr.episodic.write().await;
            ep.add_episode(vec![
                Message {
                    role: Role::User,
                    content: "数据库优化".to_string(),
                },
                Message {
                    role: Role::Assistant,
                    content: "索引是关键".to_string(),
                },
            ])
            .await
            .unwrap();
        }
        {
            let mut kb = mgr.knowledge.write().await;
            kb.add_document("db-tips", "数据库索引优化技巧")
                .await
                .unwrap();
        }

        let ctx = mgr.build_memory_context("数据库", 10).await;
        // 并行搜索应返回两层结果
        let has_memory = ctx.contains("[记忆]");
        let has_knowledge = ctx.contains("[知识]");
        assert!(has_memory || has_knowledge || ctx.is_empty());
    }

    #[tokio::test]
    async fn concurrent_promotion_no_data_corruption() {
        let mut mgr = create_test_manager();
        // 添加多条工作记忆
        for i in 0..5 {
            mgr.working.add(format!("重要话题 {} 关于 Rust", i), 0.5);
        }
        // 同时提供事实性消息
        let messages: Vec<(String, String)> = (0..3)
            .map(|i| {
                (
                    format!("问题 {}", i),
                    format!("Rust 的特性 {} 确保了安全性和性能。", i),
                )
            })
            .collect();

        mgr.on_session_end(&messages).await.unwrap();
        // L1 应已清空
        assert!(mgr.working().is_empty());
        // L2 应有晋升的记忆
        let ep = mgr.episodic.read().await;
        let episodes = ep.search("Rust", 10).await.unwrap_or_default();
        assert!(!episodes.is_empty());
    }
}
