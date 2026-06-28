//! 工作流触发器
//!
//! 定义工作流的触发条件（时间、事件、关键词等）。

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};

/// 触发器类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trigger {
    /// 定时触发（Cron 表达式）
    Schedule { cron: String },
    /// 手动触发
    Manual,
    /// Webhook 触发
    Webhook { path: String },
    /// 关键词触发
    Keyword { keywords: Vec<String> },
    /// 事件触发
    Event { event_type: String },
    /// 设备状态变化
    DeviceState { device_id: String, state: String },
    /// 时间点触发
    TimePoint { time: DateTime<Utc> },
    /// 地理围栏触发
    Geofence {
        device_id: String,
        zone_id: String,
        transition: String, // "enter", "exit", "dwell"
    },
}

impl Trigger {
    pub fn validate(&self) -> anyhow::Result<()> {
        match self {
            Trigger::Schedule { cron } => {
                if !is_valid_schedule_expression(cron) {
                    anyhow::bail!("invalid cron expression");
                }
            }
            Trigger::Manual => {}
            Trigger::Webhook { path } => require_non_empty("path", path)?,
            Trigger::Keyword { keywords } => {
                if keywords.iter().all(|keyword| keyword.trim().is_empty()) {
                    anyhow::bail!("keywords cannot be empty");
                }
            }
            Trigger::Event { event_type } => require_non_empty("event_type", event_type)?,
            Trigger::DeviceState { device_id, state } => {
                require_non_empty("device_id", device_id)?;
                require_non_empty("state", state)?;
            }
            Trigger::TimePoint { .. } => {}
            Trigger::Geofence {
                device_id,
                zone_id,
                transition,
            } => {
                require_non_empty("device_id", device_id)?;
                require_non_empty("zone_id", zone_id)?;
                require_non_empty("transition", transition)?;
            }
        }

        Ok(())
    }

    /// 检查触发器是否满足条件
    pub fn check(&self, context: &TriggerContext) -> bool {
        match self {
            Trigger::Schedule { cron } => schedule_matches(cron, context.current_time),
            Trigger::Manual => false,
            Trigger::Webhook { path } => context
                .webhook_path
                .as_ref()
                .is_some_and(|actual| normalize(actual) == normalize(path)),
            Trigger::Keyword { keywords } => context.user_input.as_ref().is_some_and(|input| {
                let normalized_input = normalize(input);
                keywords
                    .iter()
                    .map(|keyword| normalize(keyword))
                    .filter(|keyword| !keyword.is_empty())
                    .any(|keyword| normalized_input.contains(&keyword))
            }),
            Trigger::Event { event_type } => context
                .event_type
                .as_ref()
                .is_some_and(|actual| normalize(actual) == normalize(event_type)),
            Trigger::DeviceState { device_id, state } => {
                context.device_state.as_ref().is_some_and(|change| {
                    normalize(&change.device_id) == normalize(device_id)
                        && normalize(&change.state) == normalize(state)
                })
            }
            Trigger::TimePoint { time } => context.current_time >= *time,
            Trigger::Geofence {
                device_id,
                zone_id,
                transition,
            } => context.geofence_transition.as_ref().is_some_and(|gt| {
                normalize(&gt.device_id) == normalize(device_id)
                    && normalize(&gt.zone_id) == normalize(zone_id)
                    && normalize(&gt.transition) == normalize(transition)
            }),
        }
    }
}

/// 触发器上下文
#[derive(Debug, Clone)]
pub struct TriggerContext {
    /// 当前时间
    pub current_time: DateTime<Utc>,
    /// 用户输入
    pub user_input: Option<String>,
    /// 事件类型
    pub event_type: Option<String>,
    /// 事件数据
    pub event_data: Option<String>,
    /// webhook 路径
    pub webhook_path: Option<String>,
    /// 设备状态变更
    pub device_state: Option<DeviceStateChange>,
    /// 地理围栏转换
    pub geofence_transition: Option<GeofenceTransition>,
}

/// 设备状态变更上下文
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStateChange {
    pub device_id: String,
    pub state: String,
}

/// 地理围栏转换上下文
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeofenceTransition {
    pub device_id: String,
    pub zone_id: String,
    pub transition: String,
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn require_non_empty(field_name: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{field_name} cannot be empty");
    }
    Ok(())
}

pub(crate) fn is_valid_schedule_expression(cron: &str) -> bool {
    let fields = cron.split_whitespace().collect::<Vec<_>>();
    match fields.as_slice() {
        [minute, hour, day_of_month, month, day_of_week] => {
            field_is_valid(minute, 0, 59, false)
                && field_is_valid(hour, 0, 23, false)
                && field_is_valid(day_of_month, 1, 31, false)
                && field_is_valid(month, 1, 12, false)
                && field_is_valid(day_of_week, 0, 7, true)
        }
        [second, minute, hour, day_of_month, month, day_of_week] => {
            field_is_valid(second, 0, 59, false)
                && field_is_valid(minute, 0, 59, false)
                && field_is_valid(hour, 0, 23, false)
                && field_is_valid(day_of_month, 1, 31, false)
                && field_is_valid(month, 1, 12, false)
                && field_is_valid(day_of_week, 0, 7, true)
        }
        _ => false,
    }
}

fn schedule_matches(cron: &str, current_time: DateTime<Utc>) -> bool {
    if !is_valid_schedule_expression(cron) {
        return false;
    }

    let fields = cron.split_whitespace().collect::<Vec<_>>();
    match fields.as_slice() {
        [minute, hour, day_of_month, month, day_of_week] => {
            field_matches(minute, current_time.minute(), 0, 59, false)
                && field_matches(hour, current_time.hour(), 0, 23, false)
                && field_matches(day_of_month, current_time.day(), 1, 31, false)
                && field_matches(month, current_time.month(), 1, 12, false)
                && field_matches(
                    day_of_week,
                    current_time.weekday().num_days_from_sunday(),
                    0,
                    7,
                    true,
                )
        }
        [second, minute, hour, day_of_month, month, day_of_week] => {
            field_matches(second, current_time.second(), 0, 59, false)
                && field_matches(minute, current_time.minute(), 0, 59, false)
                && field_matches(hour, current_time.hour(), 0, 23, false)
                && field_matches(day_of_month, current_time.day(), 1, 31, false)
                && field_matches(month, current_time.month(), 1, 12, false)
                && field_matches(
                    day_of_week,
                    current_time.weekday().num_days_from_sunday(),
                    0,
                    7,
                    true,
                )
        }
        _ => false,
    }
}

fn field_is_valid(field: &str, min: u32, max: u32, is_weekday: bool) -> bool {
    let trimmed = field.trim();
    if trimmed.is_empty() {
        return false;
    }

    trimmed
        .split(',')
        .map(str::trim)
        .all(|item| cron_item_is_valid(item, min, max, is_weekday))
}

fn field_matches(field: &str, current: u32, min: u32, max: u32, is_weekday: bool) -> bool {
    let trimmed = field.trim();
    if trimmed.is_empty() {
        return false;
    }

    trimmed
        .split(',')
        .map(str::trim)
        .any(|item| cron_item_matches(item, current, min, max, is_weekday))
}

fn cron_item_is_valid(item: &str, min: u32, max: u32, is_weekday: bool) -> bool {
    if item.is_empty() {
        return false;
    }

    let (base, step) = match item.split_once('/') {
        Some((base, step)) => match step.parse::<u32>().ok().filter(|step| *step > 0) {
            Some(step) => (base, step),
            None => return false,
        },
        None => (item, 1),
    };

    if step == 0 {
        return false;
    }

    cron_item_start(base, min, max, is_weekday).is_some()
        && if let Some((start, end)) = base.split_once('-') {
            match (
                parse_cron_value(start, min, max, is_weekday),
                parse_cron_value(end, min, max, is_weekday),
            ) {
                (Some(start), Some(end)) => start <= end,
                _ => false,
            }
        } else {
            base == "*" || parse_cron_value(base, min, max, is_weekday).is_some()
        }
}

fn cron_item_matches(item: &str, current: u32, min: u32, max: u32, is_weekday: bool) -> bool {
    if item.is_empty() {
        return false;
    }

    let (base, step) = match item.split_once('/') {
        Some((base, step)) => {
            let step = step.parse::<u32>().ok().filter(|step| *step > 0);
            match step {
                Some(step) => (base, step),
                None => return false,
            }
        }
        None => (item, 1),
    };

    let start = match cron_item_start(base, min, max, is_weekday) {
        Some(start) => start,
        None => return false,
    };

    (min..=max).any(|candidate| {
        cron_base_contains(base, candidate, min, max, is_weekday)
            && (candidate.saturating_sub(start) % step == 0)
            && cron_value_matches(candidate, current, is_weekday)
    })
}

fn cron_item_start(base: &str, min: u32, max: u32, is_weekday: bool) -> Option<u32> {
    if base == "*" {
        return Some(min);
    }

    if let Some((start, _end)) = base.split_once('-') {
        return parse_cron_value(start, min, max, is_weekday);
    }

    parse_cron_value(base, min, max, is_weekday)
}

fn cron_base_contains(base: &str, candidate: u32, min: u32, max: u32, is_weekday: bool) -> bool {
    if base == "*" {
        return true;
    }

    if let Some((start, end)) = base.split_once('-') {
        let start = match parse_cron_value(start, min, max, is_weekday) {
            Some(start) => start,
            None => return false,
        };
        let end = match parse_cron_value(end, min, max, is_weekday) {
            Some(end) => end,
            None => return false,
        };
        return start <= end && candidate >= start && candidate <= end;
    }

    parse_cron_value(base, min, max, is_weekday).is_some_and(|expected| candidate == expected)
}

fn parse_cron_value(token: &str, min: u32, max: u32, is_weekday: bool) -> Option<u32> {
    let value = token.trim().parse::<u32>().ok()?;
    if is_weekday {
        return (value <= 7).then_some(value);
    }

    (value >= min && value <= max).then_some(value)
}

fn cron_value_matches(candidate: u32, current: u32, is_weekday: bool) -> bool {
    candidate == current || (is_weekday && current == 0 && candidate == 7)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn context_at(current_time: DateTime<Utc>) -> TriggerContext {
        TriggerContext {
            current_time,
            user_input: None,
            event_type: None,
            event_data: None,
            webhook_path: None,
            device_state: None,
            geofence_transition: None,
        }
    }

    #[test]
    fn schedule_matches_standard_five_field_cron() {
        let context = context_at(Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 45).unwrap());

        assert!(Trigger::Schedule {
            cron: "30 9 * * 1".to_string()
        }
        .check(&context));
        assert!(!Trigger::Schedule {
            cron: "31 9 * * 1".to_string()
        }
        .check(&context));
    }

    #[test]
    fn keyword_matches_any_keyword_case_insensitively() {
        let mut context = context_at(Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 0).unwrap());
        context.user_input = Some("Need an Invoice copy for March".to_string());

        assert!(Trigger::Keyword {
            keywords: vec!["refund".to_string(), "invoice".to_string()]
        }
        .check(&context));
        assert!(!Trigger::Keyword {
            keywords: vec!["shipment".to_string()]
        }
        .check(&context));
    }

    #[test]
    fn event_and_device_state_triggers_match_explicit_context_fields() {
        let mut context = context_at(Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 0).unwrap());
        context.event_type = Some("presence.detected".to_string());
        context.event_data = Some("{\"room\":\"office\"}".to_string());
        context.device_state = Some(DeviceStateChange {
            device_id: "lamp-1".to_string(),
            state: "on".to_string(),
        });

        assert!(Trigger::Event {
            event_type: "presence.detected".to_string()
        }
        .check(&context));
        assert!(Trigger::DeviceState {
            device_id: "lamp-1".to_string(),
            state: "on".to_string()
        }
        .check(&context));
        assert!(!Trigger::DeviceState {
            device_id: "lamp-1".to_string(),
            state: "off".to_string()
        }
        .check(&context));
    }

    #[test]
    fn webhook_trigger_matches_explicit_path() {
        let mut context = context_at(Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 0).unwrap());
        context.webhook_path = Some("/hooks/daily-summary".to_string());

        assert!(Trigger::Webhook {
            path: "/hooks/daily-summary".to_string()
        }
        .check(&context));
        assert!(!Trigger::Manual.check(&context));
    }

    #[test]
    fn time_point_matches_when_current_time_reaches_or_passes_target() {
        let time = Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 0).unwrap();

        assert!(Trigger::TimePoint { time }.check(&context_at(time)));
        assert!(Trigger::TimePoint { time }.check(&context_at(
            Utc.with_ymd_and_hms(2026, 3, 9, 9, 31, 0).unwrap(),
        )));
        assert!(!Trigger::TimePoint { time }.check(&context_at(
            Utc.with_ymd_and_hms(2026, 3, 9, 9, 29, 59).unwrap(),
        )));
    }
}
