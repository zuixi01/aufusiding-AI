//! 健康数据聚合
//!
//! 从可穿戴设备收集健康数据，提供每日摘要和趋势分析。

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// 健康数据快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub timestamp: DateTime<Utc>,
    pub heart_rate: Option<u16>,
    pub steps: Option<u32>,
    pub blood_oxygen: Option<u8>,
    pub sleep_hours: Option<f64>,
    pub calories_burned: Option<u32>,
    pub distance_meters: Option<u32>,
}

/// 每日健康摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySummary {
    pub date: NaiveDate,
    pub avg_heart_rate: Option<f64>,
    pub total_steps: u32,
    pub total_calories: u32,
    pub total_distance_meters: u32,
    pub total_sleep_hours: f64,
    pub min_blood_oxygen: Option<u8>,
    pub snapshot_count: usize,
}

/// 健康数据聚合器
pub struct HealthAggregator {
    snapshots: Vec<HealthSnapshot>,
}

impl HealthAggregator {
    pub fn new() -> Self {
        Self {
            snapshots: Vec::new(),
        }
    }

    /// 记录一条健康快照
    pub fn record(&mut self, snapshot: HealthSnapshot) {
        self.snapshots.push(snapshot);
    }

    /// 获取所有快照
    pub fn snapshots(&self) -> &[HealthSnapshot] {
        &self.snapshots
    }

    /// 获取指定日期的每日摘要
    pub fn daily_summary(&self, date: NaiveDate) -> DailySummary {
        let day_snapshots: Vec<&HealthSnapshot> = self
            .snapshots
            .iter()
            .filter(|s| s.timestamp.date_naive() == date)
            .collect();

        let snapshot_count = day_snapshots.len();

        // 平均心率
        let heart_rates: Vec<f64> = day_snapshots
            .iter()
            .filter_map(|s| s.heart_rate.map(|hr| hr as f64))
            .collect();
        let avg_heart_rate = if heart_rates.is_empty() {
            None
        } else {
            let sum: f64 = heart_rates.iter().sum();
            Some(sum / heart_rates.len() as f64)
        };

        // 总步数（取最大值，因为步数是累计的）
        let total_steps = day_snapshots
            .iter()
            .filter_map(|s| s.steps)
            .max()
            .unwrap_or(0);

        // 总卡路里（取最大值）
        let total_calories = day_snapshots
            .iter()
            .filter_map(|s| s.calories_burned)
            .max()
            .unwrap_or(0);

        // 总距离（取最大值）
        let total_distance_meters = day_snapshots
            .iter()
            .filter_map(|s| s.distance_meters)
            .max()
            .unwrap_or(0);

        // 总睡眠时间（求和）
        let total_sleep_hours: f64 = day_snapshots.iter().filter_map(|s| s.sleep_hours).sum();

        // 最低血氧
        let min_blood_oxygen = day_snapshots.iter().filter_map(|s| s.blood_oxygen).min();

        DailySummary {
            date,
            avg_heart_rate,
            total_steps,
            total_calories,
            total_distance_meters,
            total_sleep_hours,
            min_blood_oxygen,
            snapshot_count,
        }
    }

    /// 获取最新的快照
    pub fn latest(&self) -> Option<&HealthSnapshot> {
        self.snapshots.iter().max_by_key(|s| s.timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    fn make_snapshot(
        hour: u32,
        heart_rate: Option<u16>,
        steps: Option<u32>,
        blood_oxygen: Option<u8>,
        sleep_hours: Option<f64>,
        calories: Option<u32>,
        distance: Option<u32>,
    ) -> HealthSnapshot {
        HealthSnapshot {
            timestamp: Utc.with_ymd_and_hms(2026, 3, 12, hour, 0, 0).unwrap(),
            heart_rate,
            steps,
            blood_oxygen,
            sleep_hours,
            calories_burned: calories,
            distance_meters: distance,
        }
    }

    fn setup() -> HealthAggregator {
        let mut agg = HealthAggregator::new();
        agg.record(make_snapshot(
            8,
            Some(65),
            Some(1000),
            Some(98),
            Some(7.5),
            Some(200),
            Some(800),
        ));
        agg.record(make_snapshot(
            12,
            Some(80),
            Some(5000),
            Some(97),
            None,
            Some(800),
            Some(4000),
        ));
        agg.record(make_snapshot(
            18,
            Some(72),
            Some(8500),
            Some(99),
            None,
            Some(1500),
            Some(6800),
        ));
        agg
    }

    #[test]
    fn record_and_snapshots() {
        let agg = setup();
        assert_eq!(agg.snapshots().len(), 3);
    }

    #[test]
    fn daily_summary_avg_heart_rate() {
        let agg = setup();
        let date = NaiveDate::from_ymd_opt(2026, 3, 12).unwrap();
        let summary = agg.daily_summary(date);
        let avg = summary.avg_heart_rate.unwrap();
        // (65 + 80 + 72) / 3 = 72.333...
        assert!((avg - 72.333).abs() < 0.1);
    }

    #[test]
    fn daily_summary_total_steps() {
        let agg = setup();
        let date = NaiveDate::from_ymd_opt(2026, 3, 12).unwrap();
        let summary = agg.daily_summary(date);
        assert_eq!(summary.total_steps, 8500);
    }

    #[test]
    fn daily_summary_total_calories() {
        let agg = setup();
        let date = NaiveDate::from_ymd_opt(2026, 3, 12).unwrap();
        let summary = agg.daily_summary(date);
        assert_eq!(summary.total_calories, 1500);
    }

    #[test]
    fn daily_summary_total_distance() {
        let agg = setup();
        let date = NaiveDate::from_ymd_opt(2026, 3, 12).unwrap();
        let summary = agg.daily_summary(date);
        assert_eq!(summary.total_distance_meters, 6800);
    }

    #[test]
    fn daily_summary_sleep_hours() {
        let agg = setup();
        let date = NaiveDate::from_ymd_opt(2026, 3, 12).unwrap();
        let summary = agg.daily_summary(date);
        assert!((summary.total_sleep_hours - 7.5).abs() < 0.01);
    }

    #[test]
    fn daily_summary_min_blood_oxygen() {
        let agg = setup();
        let date = NaiveDate::from_ymd_opt(2026, 3, 12).unwrap();
        let summary = agg.daily_summary(date);
        assert_eq!(summary.min_blood_oxygen, Some(97));
    }

    #[test]
    fn daily_summary_snapshot_count() {
        let agg = setup();
        let date = NaiveDate::from_ymd_opt(2026, 3, 12).unwrap();
        let summary = agg.daily_summary(date);
        assert_eq!(summary.snapshot_count, 3);
    }

    #[test]
    fn daily_summary_empty_day() {
        let agg = setup();
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let summary = agg.daily_summary(date);
        assert_eq!(summary.snapshot_count, 0);
        assert!(summary.avg_heart_rate.is_none());
        assert_eq!(summary.total_steps, 0);
        assert_eq!(summary.total_calories, 0);
        assert_eq!(summary.total_distance_meters, 0);
        assert!((summary.total_sleep_hours - 0.0).abs() < 0.01);
        assert!(summary.min_blood_oxygen.is_none());
    }

    #[test]
    fn latest_returns_most_recent() {
        let agg = setup();
        let latest = agg.latest().unwrap();
        assert_eq!(latest.timestamp.hour(), 18);
        assert_eq!(latest.heart_rate, Some(72));
    }

    #[test]
    fn latest_empty_returns_none() {
        let agg = HealthAggregator::new();
        assert!(agg.latest().is_none());
    }

    #[test]
    fn new_aggregator_is_empty() {
        let agg = HealthAggregator::new();
        assert!(agg.snapshots().is_empty());
    }
}
