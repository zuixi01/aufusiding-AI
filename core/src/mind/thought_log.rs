//! 思维日志系统
//!
//! 记录 PBRIC 每阶段的内心活动和决策过程，支持用户查询和回放。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 思维阶段
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ThinkingPhase {
    Perceive,
    Believe,
    Reason,
    Intend,
    Commit,
    Reflect,
    Curiosity,
}

/// 单条思维记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtEntry {
    pub id: u64,
    pub session_id: String,
    pub phase: ThinkingPhase,
    pub summary: String,
    pub reasoning: String,
    pub input_context: String,
    pub output_decision: Option<String>,
    pub confidence: f64,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

/// 思维链：一次完整 PBRIC 循环的所有思维记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtChain {
    pub chain_id: String,
    pub session_id: String,
    pub user_input: String,
    pub final_output: Option<String>,
    pub entries: Vec<ThoughtEntry>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

/// 查询过滤器
#[derive(Debug, Clone, Default)]
pub struct ThoughtQuery {
    pub session_id: Option<String>,
    pub phase: Option<ThinkingPhase>,
    pub min_confidence: Option<f64>,
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

/// 思维记录输入参数
pub struct ThoughtInput<'a> {
    pub session_id: &'a str,
    pub phase: ThinkingPhase,
    pub summary: &'a str,
    pub reasoning: &'a str,
    pub input_context: &'a str,
    pub output_decision: Option<&'a str>,
    pub confidence: f64,
    pub duration_ms: u64,
}

/// 思维日志管理器
pub struct ThoughtLogger {
    entries: Vec<ThoughtEntry>,
    chains: Vec<ThoughtChain>,
    active_chain: Option<ThoughtChain>,
    next_id: u64,
    next_chain_id: u64,
    max_entries: usize,
}

impl ThoughtLogger {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            chains: Vec::new(),
            active_chain: None,
            next_id: 1,
            next_chain_id: 1,
            max_entries,
        }
    }

    /// 开始新的思维链
    pub fn begin_chain(&mut self, session_id: &str, user_input: &str) -> String {
        if self.active_chain.is_some() {
            self.end_chain(None);
        }
        let chain_id = format!("chain_{}", self.next_chain_id);
        self.next_chain_id += 1;
        self.active_chain = Some(ThoughtChain {
            chain_id: chain_id.clone(),
            session_id: session_id.to_string(),
            user_input: user_input.to_string(),
            final_output: None,
            entries: Vec::new(),
            started_at: Utc::now(),
            ended_at: None,
        });
        chain_id
    }

    /// 记录一条思维
    #[allow(clippy::too_many_arguments)]
    pub fn log_thought(
        &mut self,
        session_id: &str,
        phase: ThinkingPhase,
        summary: &str,
        reasoning: &str,
        input_context: &str,
        output_decision: Option<&str>,
        confidence: f64,
        duration_ms: u64,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let entry = ThoughtEntry {
            id,
            session_id: session_id.to_string(),
            phase,
            summary: summary.to_string(),
            reasoning: reasoning.to_string(),
            input_context: input_context.to_string(),
            output_decision: output_decision.map(|s| s.to_string()),
            confidence: confidence.clamp(0.0, 1.0),
            duration_ms,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        };

        if let Some(chain) = &mut self.active_chain {
            chain.entries.push(entry.clone());
        }
        self.entries.push(entry);

        // 超过上限时淘汰最旧记录
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        id
    }

    /// 结束当前思维链
    pub fn end_chain(&mut self, final_output: Option<&str>) {
        if let Some(mut chain) = self.active_chain.take() {
            chain.final_output = final_output.map(|s| s.to_string());
            chain.ended_at = Some(Utc::now());
            self.chains.push(chain);
        }
    }

    /// 为最近的思维记录添加元数据
    pub fn add_metadata(&mut self, entry_id: u64, key: &str, value: &str) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == entry_id) {
            entry.metadata.insert(key.to_string(), value.to_string());
            // 同步到活跃链
            if let Some(chain) = &mut self.active_chain {
                if let Some(ce) = chain.entries.iter_mut().find(|e| e.id == entry_id) {
                    ce.metadata.insert(key.to_string(), value.to_string());
                }
            }
            return true;
        }
        false
    }

    /// 查询思维记录
    pub fn query(&self, filter: &ThoughtQuery) -> Vec<&ThoughtEntry> {
        let mut results: Vec<&ThoughtEntry> = self
            .entries
            .iter()
            .filter(|e| {
                if let Some(sid) = &filter.session_id {
                    if &e.session_id != sid {
                        return false;
                    }
                }
                if let Some(phase) = &filter.phase {
                    if &e.phase != phase {
                        return false;
                    }
                }
                if let Some(min_conf) = filter.min_confidence {
                    if e.confidence < min_conf {
                        return false;
                    }
                }
                if let Some(after) = filter.after {
                    if e.timestamp < after {
                        return false;
                    }
                }
                if let Some(before) = filter.before {
                    if e.timestamp > before {
                        return false;
                    }
                }
                true
            })
            .collect();

        // 按时间倒序
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }
        results
    }

    /// 获取思维链
    pub fn get_chain(&self, chain_id: &str) -> Option<&ThoughtChain> {
        self.chains.iter().find(|c| c.chain_id == chain_id)
    }

    /// 获取会话的所有思维链
    pub fn get_session_chains(&self, session_id: &str) -> Vec<&ThoughtChain> {
        self.chains
            .iter()
            .filter(|c| c.session_id == session_id)
            .collect()
    }

    /// 生成思维摘要（最近 N 条记录的概览）
    pub fn summarize_recent(&self, count: usize) -> ThoughtSummary {
        let recent: Vec<&ThoughtEntry> = self.entries.iter().rev().take(count).collect();
        let total = recent.len();
        if total == 0 {
            return ThoughtSummary::default();
        }

        let avg_confidence =
            recent.iter().map(|e| e.confidence).sum::<f64>() / total as f64;
        let avg_duration =
            recent.iter().map(|e| e.duration_ms).sum::<u64>() / total as u64;

        let mut phase_counts: HashMap<String, usize> = HashMap::new();
        for e in &recent {
            *phase_counts
                .entry(format!("{:?}", e.phase))
                .or_insert(0) += 1;
        }

        ThoughtSummary {
            entry_count: total,
            avg_confidence,
            avg_duration_ms: avg_duration,
            phase_distribution: phase_counts,
        }
    }

    /// 获取所有记录数
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 获取完成的思维链数
    pub fn chain_count(&self) -> usize {
        self.chains.len()
    }

    /// 是否有活跃链
    pub fn has_active_chain(&self) -> bool {
        self.active_chain.is_some()
    }
}

/// 思维摘要
#[derive(Debug, Clone, Default)]
pub struct ThoughtSummary {
    pub entry_count: usize,
    pub avg_confidence: f64,
    pub avg_duration_ms: u64,
    pub phase_distribution: HashMap<String, usize>,
}

impl Default for ThoughtLogger {
    fn default() -> Self {
        Self::new(10000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_sample(logger: &mut ThoughtLogger, session: &str, phase: ThinkingPhase) -> u64 {
        logger.log_thought(
            session,
            phase.clone(),
            &format!("{:?} summary", phase),
            "reasoning details",
            "input ctx",
            Some("decision"),
            0.8,
            50,
        )
    }

    #[test]
    fn test_create_logger() {
        let logger = ThoughtLogger::new(100);
        assert!(logger.is_empty());
        assert_eq!(logger.chain_count(), 0);
        assert!(!logger.has_active_chain());
    }

    #[test]
    fn test_log_thought() {
        let mut logger = ThoughtLogger::new(100);
        let id = log_sample(&mut logger, "s1", ThinkingPhase::Perceive);

        assert_eq!(id, 1);
        assert_eq!(logger.len(), 1);
    }

    #[test]
    fn test_begin_and_end_chain() {
        let mut logger = ThoughtLogger::new(100);
        let chain_id = logger.begin_chain("s1", "你好");
        assert!(logger.has_active_chain());

        log_sample(&mut logger, "s1", ThinkingPhase::Perceive);
        log_sample(&mut logger, "s1", ThinkingPhase::Believe);

        logger.end_chain(Some("你好！有什么可以帮你的？"));
        assert!(!logger.has_active_chain());
        assert_eq!(logger.chain_count(), 1);

        let chain = logger.get_chain(&chain_id).unwrap();
        assert_eq!(chain.entries.len(), 2);
        assert_eq!(chain.final_output.as_deref(), Some("你好！有什么可以帮你的？"));
        assert!(chain.ended_at.is_some());
    }

    #[test]
    fn test_auto_close_previous_chain() {
        let mut logger = ThoughtLogger::new(100);
        logger.begin_chain("s1", "input1");
        log_sample(&mut logger, "s1", ThinkingPhase::Perceive);

        // 开始新链会自动关闭旧链
        logger.begin_chain("s1", "input2");
        assert_eq!(logger.chain_count(), 1);
        assert!(logger.has_active_chain());
    }

    #[test]
    fn test_query_by_session() {
        let mut logger = ThoughtLogger::new(100);
        log_sample(&mut logger, "s1", ThinkingPhase::Perceive);
        log_sample(&mut logger, "s2", ThinkingPhase::Believe);
        log_sample(&mut logger, "s1", ThinkingPhase::Reason);

        let query = ThoughtQuery {
            session_id: Some("s1".to_string()),
            ..Default::default()
        };
        let results = logger.query(&query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_by_phase() {
        let mut logger = ThoughtLogger::new(100);
        log_sample(&mut logger, "s1", ThinkingPhase::Perceive);
        log_sample(&mut logger, "s1", ThinkingPhase::Believe);
        log_sample(&mut logger, "s1", ThinkingPhase::Perceive);

        let query = ThoughtQuery {
            phase: Some(ThinkingPhase::Perceive),
            ..Default::default()
        };
        let results = logger.query(&query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_by_min_confidence() {
        let mut logger = ThoughtLogger::new(100);
        logger.log_thought("s1", ThinkingPhase::Perceive, "low", "", "", None, 0.3, 10);
        logger.log_thought("s1", ThinkingPhase::Believe, "high", "", "", None, 0.9, 10);

        let query = ThoughtQuery {
            min_confidence: Some(0.5),
            ..Default::default()
        };
        let results = logger.query(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].summary, "high");
    }

    #[test]
    fn test_query_with_limit() {
        let mut logger = ThoughtLogger::new(100);
        for _ in 0..10 {
            log_sample(&mut logger, "s1", ThinkingPhase::Reason);
        }

        let query = ThoughtQuery {
            limit: Some(3),
            ..Default::default()
        };
        let results = logger.query(&query);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_add_metadata() {
        let mut logger = ThoughtLogger::new(100);
        let id = log_sample(&mut logger, "s1", ThinkingPhase::Perceive);

        assert!(logger.add_metadata(id, "model", "qwen-7b"));
        assert!(!logger.add_metadata(999, "key", "val"));

        let query = ThoughtQuery::default();
        let results = logger.query(&query);
        assert_eq!(results[0].metadata.get("model").unwrap(), "qwen-7b");
    }

    #[test]
    fn test_max_entries_eviction() {
        let mut logger = ThoughtLogger::new(5);
        for i in 0..8 {
            logger.log_thought(
                "s1",
                ThinkingPhase::Perceive,
                &format!("entry_{}", i),
                "", "", None, 0.5, 10,
            );
        }
        assert_eq!(logger.len(), 5);
        // 最旧的被淘汰，最新的保留
        let query = ThoughtQuery::default();
        let results = logger.query(&query);
        assert_eq!(results[0].summary, "entry_7");
    }

    #[test]
    fn test_get_session_chains() {
        let mut logger = ThoughtLogger::new(100);

        logger.begin_chain("s1", "q1");
        logger.end_chain(Some("a1"));

        logger.begin_chain("s2", "q2");
        logger.end_chain(Some("a2"));

        logger.begin_chain("s1", "q3");
        logger.end_chain(Some("a3"));

        let s1_chains = logger.get_session_chains("s1");
        assert_eq!(s1_chains.len(), 2);

        let s2_chains = logger.get_session_chains("s2");
        assert_eq!(s2_chains.len(), 1);
    }

    #[test]
    fn test_summarize_recent() {
        let mut logger = ThoughtLogger::new(100);
        logger.log_thought("s1", ThinkingPhase::Perceive, "a", "", "", None, 0.6, 40);
        logger.log_thought("s1", ThinkingPhase::Believe, "b", "", "", None, 0.8, 60);
        logger.log_thought("s1", ThinkingPhase::Perceive, "c", "", "", None, 1.0, 80);

        let summary = logger.summarize_recent(10);
        assert_eq!(summary.entry_count, 3);
        assert!((summary.avg_confidence - 0.8).abs() < 0.01);
        assert_eq!(summary.avg_duration_ms, 60);
        assert_eq!(*summary.phase_distribution.get("Perceive").unwrap(), 2);
        assert_eq!(*summary.phase_distribution.get("Believe").unwrap(), 1);
    }

    #[test]
    fn test_summarize_empty() {
        let logger = ThoughtLogger::new(100);
        let summary = logger.summarize_recent(10);
        assert_eq!(summary.entry_count, 0);
        assert_eq!(summary.avg_confidence, 0.0);
    }
}
