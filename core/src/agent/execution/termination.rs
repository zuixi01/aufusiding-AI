//! 自适应提前终止 — 检测 ReAct 循环中的无效重复和收敛失败
//!
//! 通过 Jaccard 相似度检测重复 thought，以及关键词检测放弃信号，
//! 避免在无效步骤上浪费 token。

use std::collections::{HashSet, VecDeque};

/// 终止检测器
pub struct TerminationDetector {
    recent_thoughts: VecDeque<String>,
    window_size: usize,
    similarity_threshold: f64,
}

impl TerminationDetector {
    pub fn new() -> Self {
        Self {
            recent_thoughts: VecDeque::new(),
            window_size: 3,
            similarity_threshold: 0.8,
        }
    }

    /// 检测是否应提前终止
    pub fn should_terminate(&mut self, current_thought: &str) -> bool {
        let dominated_by_loop = self.detect_loop(current_thought);
        let giving_up = Self::detect_giving_up(current_thought);

        self.recent_thoughts.push_back(current_thought.to_string());
        if self.recent_thoughts.len() > self.window_size {
            self.recent_thoughts.pop_front();
        }

        dominated_by_loop || giving_up
    }

    /// 生成强制总结 prompt
    pub fn force_answer_prompt() -> String {
        "基于以上所有信息，请直接给出最终答案。不要再使用工具，不要再思考。\
         请用 ANSWER: 开头回复你的最终结论。"
            .to_string()
    }

    /// 检测连续重复 thought（Jaccard 相似度）
    fn detect_loop(&self, current: &str) -> bool {
        if self.recent_thoughts.len() < 2 {
            return false;
        }
        // 检查最近 2 条是否与当前相似
        let recent: Vec<&String> = self.recent_thoughts.iter().rev().take(2).collect();
        recent
            .iter()
            .all(|prev| jaccard_similarity(prev, current) > self.similarity_threshold)
    }

    /// 检测放弃信号关键词
    fn detect_giving_up(thought: &str) -> bool {
        const GIVE_UP_SIGNALS: &[&str] = &[
            "无法",
            "不确定",
            "重试",
            "失败",
            "cannot",
            "unable",
            "give up",
            "stuck",
        ];
        GIVE_UP_SIGNALS
            .iter()
            .any(|signal| thought.contains(signal))
    }
}

impl Default for TerminationDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Jaccard 相似度（基于字符 bigram）
fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let bigrams_a: HashSet<(char, char)> = a.chars().zip(a.chars().skip(1)).collect();
    let bigrams_b: HashSet<(char, char)> = b.chars().zip(b.chars().skip(1)).collect();

    if bigrams_a.is_empty() && bigrams_b.is_empty() {
        return 1.0;
    }

    let intersection = bigrams_a.intersection(&bigrams_b).count();
    let union = bigrams_a.union(&bigrams_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_thoughts_trigger_termination() {
        let mut detector = TerminationDetector::new();
        // 需要先积累 2 条历史
        assert!(!detector.should_terminate("我需要搜索数据库中的用户信息"));
        assert!(!detector.should_terminate("我需要搜索数据库中的用户信息"));
        // 第 3 次相同 thought → 检测到循环
        assert!(detector.should_terminate("我需要搜索数据库中的用户信息"));
    }

    #[test]
    fn giving_up_signal_triggers_termination() {
        let mut detector = TerminationDetector::new();
        assert!(detector.should_terminate("无法找到相关信息，我不确定如何继续"));
    }

    #[test]
    fn normal_reasoning_does_not_trigger() {
        let mut detector = TerminationDetector::new();
        assert!(!detector.should_terminate("首先分析需求"));
        assert!(!detector.should_terminate("然后设计架构"));
        assert!(!detector.should_terminate("最后编写代码"));
    }

    #[test]
    fn force_answer_prompt_format() {
        let prompt = TerminationDetector::force_answer_prompt();
        assert!(prompt.contains("ANSWER:"));
        assert!(prompt.contains("最终答案"));
    }
}
