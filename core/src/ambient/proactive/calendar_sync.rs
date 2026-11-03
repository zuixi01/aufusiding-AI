//! 日历同步
//!
//! CalDAV/Google Calendar/Apple Calendar 同步接口。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 提醒偏移量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderOffset {
    pub minutes_before: u32,
}

/// 日历事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: Uuid,
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub is_all_day: bool,
    pub attendees: Vec<String>,
    pub reminders: Vec<ReminderOffset>,
}

/// 日历提供者 trait
#[async_trait]
pub trait CalendarProvider: Send + Sync {
    async fn list_events(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<CalendarEvent>>;
    async fn create_event(&self, event: CalendarEvent) -> anyhow::Result<Uuid>;
    async fn update_event(&self, event: CalendarEvent) -> anyhow::Result<()>;
    async fn delete_event(&self, id: &Uuid) -> anyhow::Result<()>;
}

/// Mock 日历提供者
pub struct MockCalendarProvider {
    events: std::sync::Mutex<Vec<CalendarEvent>>,
}

impl MockCalendarProvider {
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl CalendarProvider for MockCalendarProvider {
    async fn list_events(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<CalendarEvent>> {
        let events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        Ok(events
            .iter()
            .filter(|e| e.start >= from && e.start <= to)
            .cloned()
            .collect())
    }

    async fn create_event(&self, event: CalendarEvent) -> anyhow::Result<Uuid> {
        let id = event.id;
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(event);
        Ok(id)
    }

    async fn update_event(&self, event: CalendarEvent) -> anyhow::Result<()> {
        let mut events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pos) = events.iter().position(|e| e.id == event.id) {
            events[pos] = event;
            Ok(())
        } else {
            anyhow::bail!("event not found: {}", event.id)
        }
    }

    async fn delete_event(&self, id: &Uuid) -> anyhow::Result<()> {
        let mut events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pos) = events.iter().position(|e| e.id == *id) {
            events.remove(pos);
            Ok(())
        } else {
            anyhow::bail!("event not found: {}", id)
        }
    }
}

/// 日历同步服务
pub struct CalendarSync {
    events: Vec<CalendarEvent>,
    provider: Option<Box<dyn CalendarProvider>>,
}

impl CalendarSync {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            provider: None,
        }
    }

    pub fn with_provider(mut self, provider: Box<dyn CalendarProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    /// 添加事件（模拟同步）
    pub fn add_event(&mut self, event: CalendarEvent) {
        self.events.push(event);
    }

    /// 获取今日事件
    pub fn today_events(&self, now: DateTime<Utc>) -> Vec<&CalendarEvent> {
        let today = now.date_naive();
        self.events
            .iter()
            .filter(|e| e.start.date_naive() == today)
            .collect()
    }

    /// 获取未来 N 天事件
    pub fn upcoming_events(&self, now: DateTime<Utc>, days: i64) -> Vec<&CalendarEvent> {
        let end = now + chrono::Duration::days(days);
        self.events
            .iter()
            .filter(|e| e.start >= now && e.start <= end)
            .collect()
    }

    /// 下一个事件
    pub fn next_event(&self, now: DateTime<Utc>) -> Option<&CalendarEvent> {
        self.events
            .iter()
            .filter(|e| e.start > now)
            .min_by_key(|e| e.start)
    }

    /// 生成日程语音摘要
    pub fn voice_summary(&self, now: DateTime<Utc>) -> String {
        let today = self.today_events(now);
        if today.is_empty() {
            return "今天没有日程安排。".into();
        }

        let count = today.len();
        let first = &today[0];
        let time = first.start.format("%H:%M");

        if count == 1 {
            format!("今天有 1 个日程：{}点{}。", time, first.title)
        } else {
            format!(
                "今天有 {} 个日程，第一个是{}点的{}。",
                count, time, first.title
            )
        }
    }

    /// 事件总数
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// 通过 provider 创建事件
    pub async fn create_event(&mut self, event: CalendarEvent) -> anyhow::Result<Uuid> {
        let id = event.id;
        if let Some(provider) = &self.provider {
            provider.create_event(event.clone()).await?;
        }
        self.events.push(event);
        Ok(id)
    }

    /// 通过 provider 更新事件
    pub async fn update_event(&mut self, event: CalendarEvent) -> anyhow::Result<()> {
        if let Some(provider) = &self.provider {
            provider.update_event(event.clone()).await?;
        }
        if let Some(pos) = self.events.iter().position(|e| e.id == event.id) {
            self.events[pos] = event;
            Ok(())
        } else {
            anyhow::bail!("event not found: {}", event.id)
        }
    }

    /// 通过 provider 删除事件
    pub async fn delete_event(&mut self, id: &Uuid) -> anyhow::Result<()> {
        if let Some(provider) = &self.provider {
            provider.delete_event(id).await?;
        }
        if let Some(pos) = self.events.iter().position(|e| e.id == *id) {
            self.events.remove(pos);
            Ok(())
        } else {
            anyhow::bail!("event not found: {}", id)
        }
    }

    /// 获取需要提醒的事件
    pub fn due_reminders(&self, now: DateTime<Utc>) -> Vec<(&CalendarEvent, &ReminderOffset)> {
        self.events
            .iter()
            .flat_map(|e| {
                e.reminders.iter().filter_map(move |r| {
                    let remind_at = e.start - chrono::Duration::minutes(r.minutes_before as i64);
                    if remind_at <= now && now < e.start {
                        Some((e, r))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }
}

/// ICS (iCalendar) 文件解析器，为 CalDAV 提供基础支持
pub struct IcsCalendarSync;

impl IcsCalendarSync {
    /// 从 .ics 文件路径加载事件
    pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<Vec<CalendarEvent>> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("无法读取 ICS 文件 '{}': {}", path.display(), e))?;
        Self::parse_ics(&content)
    }

    /// 解析 ICS 格式字符串为日历事件列表
    pub fn parse_ics(content: &str) -> anyhow::Result<Vec<CalendarEvent>> {
        let mut events = Vec::new();
        let mut in_vevent = false;
        let mut summary = None::<String>;
        let mut dtstart = None::<String>;
        let mut dtend = None::<String>;
        let mut description = None::<String>;
        let mut location = None::<String>;

        for line in content.lines() {
            let line = line.trim_end_matches('\r');

            if line == "BEGIN:VEVENT" {
                in_vevent = true;
                summary = None;
                dtstart = None;
                dtend = None;
                description = None;
                location = None;
                continue;
            }

            if line == "END:VEVENT" {
                if in_vevent {
                    let start = dtstart
                        .as_deref()
                        .and_then(Self::parse_ics_datetime)
                        .unwrap_or_else(Utc::now);
                    let end = dtend
                        .as_deref()
                        .and_then(Self::parse_ics_datetime)
                        .unwrap_or(start + chrono::Duration::hours(1));
                    let is_all_day = dtstart.as_deref().map(|s| s.len() == 8).unwrap_or(false);

                    events.push(CalendarEvent {
                        id: Uuid::new_v4(),
                        title: summary.clone().unwrap_or_else(|| "(无标题)".into()),
                        start,
                        end,
                        location: location.clone(),
                        description: description.clone(),
                        is_all_day,
                        attendees: Vec::new(),
                        reminders: Vec::new(),
                    });
                }
                in_vevent = false;
                continue;
            }

            if !in_vevent {
                continue;
            }

            // 提取属性值（忽略参数部分，如 DTSTART;TZID=xxx:value）
            if let Some(val) = Self::extract_property(line, "SUMMARY") {
                summary = Some(val);
            } else if let Some(val) = Self::extract_property(line, "DTSTART") {
                dtstart = Some(val);
            } else if let Some(val) = Self::extract_property(line, "DTEND") {
                dtend = Some(val);
            } else if let Some(val) = Self::extract_property(line, "DESCRIPTION") {
                description = Some(val);
            } else if let Some(val) = Self::extract_property(line, "LOCATION") {
                location = Some(val);
            }
        }

        Ok(events)
    }

    /// 从 ICS 行中提取属性值
    /// 支持 "PROP:value" 和 "PROP;PARAM=x:value" 两种格式
    fn extract_property(line: &str, property: &str) -> Option<String> {
        if let Some(rest) = line.strip_prefix(property) {
            if let Some(stripped) = rest.strip_prefix(':') {
                return Some(stripped.to_string());
            }
            if rest.starts_with(';') {
                if let Some(colon_pos) = rest.find(':') {
                    return Some(rest[colon_pos + 1..].to_string());
                }
            }
        }
        None
    }

    /// 解析 ICS 日期时间格式
    /// 支持: 20240315T100000Z, 20240315T100000, 20240315
    fn parse_ics_datetime(s: &str) -> Option<DateTime<Utc>> {
        use chrono::NaiveDate;

        // 全天事件: 20240315
        if s.len() == 8 {
            let date = NaiveDate::parse_from_str(s, "%Y%m%d").ok()?;
            return Some(date.and_hms_opt(0, 0, 0)?.and_utc());
        }

        // UTC 格式: 20240315T100000Z
        if let Some(trimmed) = s.strip_suffix('Z') {
            let naive = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y%m%dT%H%M%S").ok()?;
            return Some(naive.and_utc());
        }

        // 本地时间（当作 UTC 处理）: 20240315T100000
        let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S").ok()?;
        Some(naive.and_utc())
    }
}
