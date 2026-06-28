#![allow(dead_code)]
//! 多任务卡片视图数据源

use serde::{Deserialize, Serialize};

/// 任务卡片状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCardStatus {
    Running,
    Completed,
    Failed,
}

/// 任务卡片
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCard {
    pub id: String,
    pub title: String,
    pub category: String,
    pub icon: String,
    pub status: TaskCardStatus,
    pub progress: Option<f32>,
    pub summary: String,
}

/// 任务卡片管理器
pub struct TaskCardManager {
    cards: Vec<TaskCard>,
    max_cards: usize,
}

impl TaskCardManager {
    pub fn new(max_cards: usize) -> Self {
        Self {
            cards: Vec::new(),
            max_cards,
        }
    }

    pub fn add(&mut self, card: TaskCard) {
        if self.cards.len() >= self.max_cards {
            // 移除最早完成的卡片
            if let Some(pos) = self
                .cards
                .iter()
                .position(|c| c.status == TaskCardStatus::Completed)
            {
                self.cards.remove(pos);
            } else if let Some(pos) = self
                .cards
                .iter()
                .position(|c| c.status == TaskCardStatus::Failed)
            {
                self.cards.remove(pos);
            }
        }
        if self.cards.len() < self.max_cards {
            self.cards.push(card);
        }
    }

    pub fn update_status(&mut self, id: &str, status: TaskCardStatus) {
        if let Some(card) = self.cards.iter_mut().find(|c| c.id == id) {
            card.status = status;
        }
    }

    pub fn update_progress(&mut self, id: &str, progress: f32) {
        if let Some(card) = self.cards.iter_mut().find(|c| c.id == id) {
            card.progress = Some(progress.clamp(0.0, 1.0));
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.cards.retain(|c| c.id != id);
    }

    pub fn active_cards(&self) -> Vec<&TaskCard> {
        self.cards
            .iter()
            .filter(|c| c.status == TaskCardStatus::Running)
            .collect()
    }

    pub fn all_cards(&self) -> &[TaskCard] {
        &self.cards
    }

    pub fn card_count(&self) -> usize {
        self.cards.len()
    }

    pub fn clear_completed(&mut self) {
        self.cards.retain(|c| c.status == TaskCardStatus::Running);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_card(id: &str, status: TaskCardStatus) -> TaskCard {
        TaskCard {
            id: id.into(),
            title: format!("Task {id}"),
            category: "test".into(),
            icon: "icon".into(),
            status,
            progress: None,
            summary: String::new(),
        }
    }

    #[test]
    fn add_and_count() {
        let mut mgr = TaskCardManager::new(10);
        mgr.add(make_card("1", TaskCardStatus::Running));
        mgr.add(make_card("2", TaskCardStatus::Running));
        assert_eq!(mgr.card_count(), 2);
    }

    #[test]
    fn max_cards_evicts_completed() {
        let mut mgr = TaskCardManager::new(2);
        mgr.add(make_card("1", TaskCardStatus::Completed));
        mgr.add(make_card("2", TaskCardStatus::Running));
        mgr.add(make_card("3", TaskCardStatus::Running));
        assert_eq!(mgr.card_count(), 2);
        assert!(mgr.all_cards().iter().all(|c| c.id != "1"));
    }

    #[test]
    fn update_status() {
        let mut mgr = TaskCardManager::new(10);
        mgr.add(make_card("1", TaskCardStatus::Running));
        mgr.update_status("1", TaskCardStatus::Completed);
        assert_eq!(mgr.all_cards()[0].status, TaskCardStatus::Completed);
    }

    #[test]
    fn update_progress_clamped() {
        let mut mgr = TaskCardManager::new(10);
        mgr.add(make_card("1", TaskCardStatus::Running));
        mgr.update_progress("1", 1.5);
        assert_eq!(mgr.all_cards()[0].progress, Some(1.0));
    }

    #[test]
    fn active_cards_filter() {
        let mut mgr = TaskCardManager::new(10);
        mgr.add(make_card("1", TaskCardStatus::Running));
        mgr.add(make_card("2", TaskCardStatus::Completed));
        mgr.add(make_card("3", TaskCardStatus::Running));
        assert_eq!(mgr.active_cards().len(), 2);
    }

    #[test]
    fn remove_card() {
        let mut mgr = TaskCardManager::new(10);
        mgr.add(make_card("1", TaskCardStatus::Running));
        mgr.remove("1");
        assert_eq!(mgr.card_count(), 0);
    }

    #[test]
    fn clear_completed() {
        let mut mgr = TaskCardManager::new(10);
        mgr.add(make_card("1", TaskCardStatus::Running));
        mgr.add(make_card("2", TaskCardStatus::Completed));
        mgr.add(make_card("3", TaskCardStatus::Failed));
        mgr.clear_completed();
        assert_eq!(mgr.card_count(), 1);
        assert_eq!(mgr.all_cards()[0].id, "1");
    }
}
