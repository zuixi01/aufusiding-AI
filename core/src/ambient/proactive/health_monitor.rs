//! 健康监控
//!
//! 可穿戴设备数据聚合（心率/步数/睡眠/提醒）。

use serde::{Deserialize, Serialize};

/// 睡眠摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepSummary {
    pub total_hours: f32,
    pub deep_sleep_hours: f32,
    pub light_sleep_hours: f32,
    pub rem_hours: f32,
    pub awake_minutes: u32,
}

/// 健康警报
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthAlert {
    pub alert_type: HealthAlertType,
    pub message: String,
    pub value: f64,
    pub threshold: f64,
}

/// 健康警报类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthAlertType {
    HeartRateHigh,
    HeartRateLow,
    InactivityWarning,
    SleepDeficit,
}

/// 健康监控器
pub struct HealthMonitor {
    heart_rate: Option<u32>,
    steps_today: u32,
    sleep: Option<SleepSummary>,
    /// 心率上限
    hr_high_threshold: u32,
    /// 心率下限
    hr_low_threshold: u32,
    /// 每日步数目标
    step_goal: u32,
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self {
            heart_rate: None,
            steps_today: 0,
            sleep: None,
            hr_high_threshold: 120,
            hr_low_threshold: 50,
            step_goal: 8000,
        }
    }

    /// 更新心率
    pub fn update_heart_rate(&mut self, bpm: u32) {
        self.heart_rate = Some(bpm);
    }

    /// 更新步数
    pub fn update_steps(&mut self, steps: u32) {
        self.steps_today = steps;
    }

    /// 更新睡眠数据
    pub fn update_sleep(&mut self, summary: SleepSummary) {
        self.sleep = Some(summary);
    }

    /// 获取当前心率
    pub fn heart_rate(&self) -> Option<u32> {
        self.heart_rate
    }

    /// 获取今日步数
    pub fn steps_today(&self) -> u32 {
        self.steps_today
    }

    /// 获取睡眠摘要
    pub fn sleep_summary(&self) -> Option<&SleepSummary> {
        self.sleep.as_ref()
    }

    /// 检测异常
    pub fn check_anomalies(&self) -> Vec<HealthAlert> {
        let mut alerts = Vec::new();

        if let Some(hr) = self.heart_rate {
            if hr > self.hr_high_threshold {
                alerts.push(HealthAlert {
                    alert_type: HealthAlertType::HeartRateHigh,
                    message: format!("心率偏高: {} bpm", hr),
                    value: hr as f64,
                    threshold: self.hr_high_threshold as f64,
                });
            }
            if hr < self.hr_low_threshold {
                alerts.push(HealthAlert {
                    alert_type: HealthAlertType::HeartRateLow,
                    message: format!("心率偏低: {} bpm", hr),
                    value: hr as f64,
                    threshold: self.hr_low_threshold as f64,
                });
            }
        }

        if let Some(sleep) = &self.sleep {
            if sleep.total_hours < 6.0 {
                alerts.push(HealthAlert {
                    alert_type: HealthAlertType::SleepDeficit,
                    message: format!("睡眠不足: {:.1} 小时", sleep.total_hours),
                    value: sleep.total_hours as f64,
                    threshold: 6.0,
                });
            }
        }

        alerts
    }

    /// 生成每日健康报告
    pub fn daily_report(&self) -> String {
        let mut parts = Vec::new();

        if let Some(sleep) = &self.sleep {
            parts.push(format!(
                "昨晚睡眠 {:.1} 小时，深睡 {:.1} 小时",
                sleep.total_hours, sleep.deep_sleep_hours
            ));
        }

        parts.push(format!(
            "今日步数 {}，目标 {}",
            self.steps_today, self.step_goal
        ));

        if let Some(hr) = self.heart_rate {
            parts.push(format!("当前心率 {} bpm", hr));
        }

        let anomalies = self.check_anomalies();
        if !anomalies.is_empty() {
            parts.push(format!("注意：{}", anomalies[0].message));
        }

        parts.join("。") + "。"
    }
}
