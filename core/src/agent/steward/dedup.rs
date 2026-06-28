//! 任务去重 — 检测并合并语义重复的子任务
//!
//! 通过 Jaccard 相似度比较任务标题+描述，合并重复任务以减少 Agent 执行次数。

use std::collections::HashSet;

use super::planner::SubTask;

/// 任务去重器
pub struct TaskDeduplicator {
    similarity_threshold: f64,
}

impl TaskDeduplicator {
    pub fn new(similarity_threshold: f64) -> Self {
        Self {
            similarity_threshold,
        }
    }

    /// 查找重复任务对（返回索引对）
    pub fn find_duplicates(&self, tasks: &[SubTask]) -> Vec<(usize, usize)> {
        let mut duplicates = Vec::new();
        for i in 0..tasks.len() {
            for j in (i + 1)..tasks.len() {
                let text_a = format!("{} {}", tasks[i].title, tasks[i].description);
                let text_b = format!("{} {}", tasks[j].title, tasks[j].description);
                if jaccard_similarity(&text_a, &text_b) > self.similarity_threshold {
                    duplicates.push((i, j));
                }
            }
        }
        duplicates
    }

    /// 合并重复任务：保留第一个，将第二个的描述追加
    pub fn merge(&self, tasks: &mut Vec<SubTask>, duplicates: &[(usize, usize)]) {
        // 收集要移除的索引（取每对中的第二个）
        let mut to_remove: Vec<usize> = duplicates.iter().map(|(_, j)| *j).collect();
        to_remove.sort_unstable();
        to_remove.dedup();

        // 先合并描述
        for &(i, j) in duplicates {
            if i < tasks.len() && j < tasks.len() {
                let appended = format!("{}\n[合并] {}", tasks[i].description, tasks[j].description);
                tasks[i].description = appended;
            }
        }

        // 从后往前移除，避免索引偏移
        for idx in to_remove.into_iter().rev() {
            if idx < tasks.len() {
                tasks.remove(idx);
            }
        }
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

    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::steward::planner::{DeliverableSpec, SubTask, SubTaskStatus};
    use crate::agent::types::{Complexity, DeliverableType, Priority, Role};
    use chrono::Utc;

    fn make_task(title: &str, desc: &str) -> SubTask {
        SubTask {
            id: title.to_string(),
            title: title.to_string(),
            description: desc.to_string(),
            assigned_to: Role::Engineer,
            assigned_agent: None,
            deliverable_spec: DeliverableSpec {
                deliverable_type: DeliverableType::Code,
                description: format!("{} 交付物", title),
            },
            acceptance_criteria: vec![],
            priority: Priority::P1,
            estimated_complexity: Complexity::Medium,
            status: SubTaskStatus::Pending,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn similar_tasks_are_merged() {
        let dedup = TaskDeduplicator::new(0.7);
        let tasks = vec![
            make_task("implement user login feature", "implement user login feature with form validation and error handling"),
            make_task("implement user login feature module", "implement user login feature with form validation and error handling support"),
        ];
        let dups = dedup.find_duplicates(&tasks);
        assert!(!dups.is_empty());
    }

    #[test]
    fn dissimilar_tasks_preserved() {
        let dedup = TaskDeduplicator::new(0.7);
        let tasks = vec![
            make_task("design database schema", "design PostgreSQL database table structure for users"),
            make_task("write frontend components", "use React to build user interface components with styling"),
        ];
        let dups = dedup.find_duplicates(&tasks);
        assert!(dups.is_empty());
    }

    #[test]
    fn merge_preserves_dependencies() {
        let dedup = TaskDeduplicator::new(0.7);
        let mut tasks = vec![
            make_task("implement user authentication", "implement user authentication with JWT tokens and session management"),
            make_task("implement user authentication module", "implement user authentication with JWT tokens and session management support"),
            make_task("write unit tests", "write unit tests for the application"),
        ];
        let dups = dedup.find_duplicates(&tasks);
        dedup.merge(&mut tasks, &dups);

        // 重复的被合并，测试任务保留
        assert!(tasks.len() < 3);
        assert!(tasks.iter().any(|t| t.title == "write unit tests"));
    }

    #[test]
    fn empty_task_list_no_crash() {
        let dedup = TaskDeduplicator::new(0.7);
        let tasks: Vec<SubTask> = vec![];
        let dups = dedup.find_duplicates(&tasks);
        assert!(dups.is_empty());

        let mut empty_tasks: Vec<SubTask> = vec![];
        dedup.merge(&mut empty_tasks, &[]);
        assert!(empty_tasks.is_empty());
    }
}
