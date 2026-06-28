#![allow(dead_code)]
//! 菱形浮窗逻辑

use serde::{Deserialize, Serialize};

/// 菱形动画状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiamondAnimation {
    None,
    Breathing,
    Static,
    Pulse,
}

/// 菱形浮窗状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiamondOverlayState {
    pub visible: bool,
    pub animation: DiamondAnimation,
    pub task_count: usize,
    pub has_urgent: bool,
}

impl DiamondOverlayState {
    pub fn hidden() -> Self {
        Self {
            visible: false,
            animation: DiamondAnimation::None,
            task_count: 0,
            has_urgent: false,
        }
    }

    pub fn update(&mut self, active_tasks: usize, has_urgent: bool) {
        self.task_count = active_tasks;
        self.has_urgent = has_urgent;

        if active_tasks == 0 && !has_urgent {
            self.visible = false;
            self.animation = DiamondAnimation::None;
        } else if has_urgent {
            self.visible = true;
            self.animation = DiamondAnimation::Pulse;
        } else if active_tasks > 0 {
            self.visible = true;
            self.animation = DiamondAnimation::Breathing;
        }
    }

    pub fn mark_viewed(&mut self) {
        if self.animation == DiamondAnimation::Static {
            self.animation = DiamondAnimation::None;
            self.visible = false;
        }
    }

    pub fn complete_tasks(&mut self) {
        if self.task_count > 0 && !self.has_urgent {
            self.animation = DiamondAnimation::Static;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_by_default() {
        let state = DiamondOverlayState::hidden();
        assert!(!state.visible);
        assert_eq!(state.animation, DiamondAnimation::None);
    }

    #[test]
    fn shows_breathing_with_tasks() {
        let mut state = DiamondOverlayState::hidden();
        state.update(2, false);
        assert!(state.visible);
        assert_eq!(state.animation, DiamondAnimation::Breathing);
    }

    #[test]
    fn shows_pulse_on_urgent() {
        let mut state = DiamondOverlayState::hidden();
        state.update(1, true);
        assert!(state.visible);
        assert_eq!(state.animation, DiamondAnimation::Pulse);
    }

    #[test]
    fn hides_when_no_tasks() {
        let mut state = DiamondOverlayState::hidden();
        state.update(2, false);
        assert!(state.visible);
        state.update(0, false);
        assert!(!state.visible);
    }

    #[test]
    fn complete_shows_static() {
        let mut state = DiamondOverlayState::hidden();
        state.update(1, false);
        state.complete_tasks();
        assert_eq!(state.animation, DiamondAnimation::Static);
    }

    #[test]
    fn mark_viewed_hides_static() {
        let mut state = DiamondOverlayState::hidden();
        state.update(1, false);
        state.complete_tasks();
        state.mark_viewed();
        assert!(!state.visible);
    }
}
