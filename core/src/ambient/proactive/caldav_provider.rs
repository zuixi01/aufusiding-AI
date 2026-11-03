//! CalDAV 日历提供者
//!
//! 通过 CalDAV HTTP API 实现 CalendarProvider trait。
//! 使用 reqwest::Client + 可配置 base_url，便于 httpmock 测试。
//! 注意：reqwest 不支持 PROPFIND，因此使用标准 HTTP 方法。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::calendar_sync::{CalendarEvent, CalendarProvider, ReminderOffset};

// ─── API 类型 ────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct ApiEvent {
    id: String,
    title: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    location: Option<String>,
    description: Option<String>,
    is_all_day: bool,
    #[serde(default)]
    attendees: Vec<String>,
    #[serde(default)]
    reminders: Vec<ApiReminder>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiReminder {
    minutes_before: u32,
}

#[derive(Debug, Deserialize)]
struct ApiEventList {
    events: Vec<ApiEvent>,
}

#[derive(Debug, Deserialize)]
struct ApiCreateResponse {
    id: String,
}

// ─── CalDavProvider ─────────────────────────────────────

/// CalDAV 日历提供者
pub struct CalDavProvider {
    username: String,
    password: String,
    client: Client,
    base_url: String,
}

impl CalDavProvider {
    pub fn new(base_url: &str, username: &str, password: &str) -> Self {
        Self {
            username: username.to_string(),
            password: password.to_string(),
            client: Client::new(),
            base_url: base_url.to_string(),
        }
    }

    /// 创建带自定义 base URL 的实例（用于测试）
    pub fn with_base_url(base_url: &str, username: &str, password: &str) -> Self {
        Self::new(base_url, username, password)
    }

    fn convert_event(api: ApiEvent) -> CalendarEvent {
        let id = Uuid::parse_str(&api.id).unwrap_or_else(|_| Uuid::new_v4());
        CalendarEvent {
            id,
            title: api.title,
            start: api.start,
            end: api.end,
            location: api.location,
            description: api.description,
            is_all_day: api.is_all_day,
            attendees: api.attendees,
            reminders: api
                .reminders
                .into_iter()
                .map(|r| ReminderOffset {
                    minutes_before: r.minutes_before,
                })
                .collect(),
        }
    }

    fn to_api_event(event: &CalendarEvent) -> ApiEvent {
        ApiEvent {
            id: event.id.to_string(),
            title: event.title.clone(),
            start: event.start,
            end: event.end,
            location: event.location.clone(),
            description: event.description.clone(),
            is_all_day: event.is_all_day,
            attendees: event.attendees.clone(),
            reminders: event
                .reminders
                .iter()
                .map(|r| ApiReminder {
                    minutes_before: r.minutes_before,
                })
                .collect(),
        }
    }
}

#[async_trait]
impl CalendarProvider for CalDavProvider {
    async fn list_events(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<CalendarEvent>> {
        let resp = self
            .client
            .get(format!(
                "{}/events?from={}&to={}",
                self.base_url,
                from.to_rfc3339(),
                to.to_rfc3339()
            ))
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("list_events failed: {}", resp.status());
        }
        let list: ApiEventList = resp.json().await?;
        Ok(list.events.into_iter().map(Self::convert_event).collect())
    }

    async fn create_event(&self, event: CalendarEvent) -> anyhow::Result<Uuid> {
        let api_event = Self::to_api_event(&event);
        let resp = self
            .client
            .post(format!("{}/events", self.base_url))
            .basic_auth(&self.username, Some(&self.password))
            .json(&api_event)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("create_event failed: {}", resp.status());
        }
        let created: ApiCreateResponse = resp.json().await?;
        let id = Uuid::parse_str(&created.id).unwrap_or(event.id);
        Ok(id)
    }

    async fn update_event(&self, event: CalendarEvent) -> anyhow::Result<()> {
        let api_event = Self::to_api_event(&event);
        let resp = self
            .client
            .put(format!("{}/events/{}", self.base_url, event.id))
            .basic_auth(&self.username, Some(&self.password))
            .json(&api_event)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("update_event failed: {}", resp.status());
        }
        Ok(())
    }

    async fn delete_event(&self, id: &Uuid) -> anyhow::Result<()> {
        let resp = self
            .client
            .delete(format!("{}/events/{}", self.base_url, id))
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("delete_event failed: {}", resp.status());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    const TEST_USER: &str = "cal_user";
    const TEST_PASS: &str = "cal_pass";
    const TEST_EVENT_ID: &str = "550e8400-e29b-41d4-a716-446655440000";

    fn mock_api_event(id: &str, title: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "title": title,
            "start": "2026-03-17T09:00:00Z",
            "end": "2026-03-17T10:00:00Z",
            "location": "Room A",
            "description": "Team standup",
            "is_all_day": false,
            "attendees": ["alice@example.com"],
            "reminders": [{"minutes_before": 15}]
        })
    }

    #[tokio::test]
    async fn list_events_returns_events() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/events");
            then.status(200).json_body(serde_json::json!({
                "events": [mock_api_event(TEST_EVENT_ID, "Standup")]
            }));
        });
        let provider = CalDavProvider::with_base_url(&server.base_url(), TEST_USER, TEST_PASS);
        let from = "2026-03-17T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let to = "2026-03-18T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let events = provider.list_events(from, to).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Standup");
        assert_eq!(events[0].reminders.len(), 1);
    }

    #[tokio::test]
    async fn list_events_uses_basic_auth() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/events")
                .header("Authorization", "Basic Y2FsX3VzZXI6Y2FsX3Bhc3M=");
            then.status(200)
                .json_body(serde_json::json!({"events": []}));
        });
        let provider = CalDavProvider::with_base_url(&server.base_url(), TEST_USER, TEST_PASS);
        let from = Utc::now();
        let to = from + chrono::Duration::days(1);
        provider.list_events(from, to).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn create_event_posts_to_events() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/events");
            then.status(201)
                .json_body(serde_json::json!({"id": TEST_EVENT_ID}));
        });
        let provider = CalDavProvider::with_base_url(&server.base_url(), TEST_USER, TEST_PASS);
        let event = CalendarEvent {
            id: Uuid::parse_str(TEST_EVENT_ID).unwrap(),
            title: "New Meeting".into(),
            start: Utc::now(),
            end: Utc::now() + chrono::Duration::hours(1),
            location: Some("Room B".into()),
            description: None,
            is_all_day: false,
            attendees: vec![],
            reminders: vec![],
        };
        let id = provider.create_event(event).await.unwrap();
        mock.assert();
        assert_eq!(id, Uuid::parse_str(TEST_EVENT_ID).unwrap());
    }

    #[tokio::test]
    async fn update_event_puts_to_event_id() {
        let event_id = Uuid::parse_str(TEST_EVENT_ID).unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(PUT).path(format!("/events/{}", TEST_EVENT_ID));
            then.status(200);
        });
        let provider = CalDavProvider::with_base_url(&server.base_url(), TEST_USER, TEST_PASS);
        let event = CalendarEvent {
            id: event_id,
            title: "Updated Meeting".into(),
            start: Utc::now(),
            end: Utc::now() + chrono::Duration::hours(1),
            location: None,
            description: None,
            is_all_day: false,
            attendees: vec![],
            reminders: vec![],
        };
        provider.update_event(event).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn delete_event_sends_delete() {
        let event_id = Uuid::parse_str(TEST_EVENT_ID).unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(DELETE)
                .path(format!("/events/{}", TEST_EVENT_ID));
            then.status(204);
        });
        let provider = CalDavProvider::with_base_url(&server.base_url(), TEST_USER, TEST_PASS);
        provider.delete_event(&event_id).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn list_events_server_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/events");
            then.status(500);
        });
        let provider = CalDavProvider::with_base_url(&server.base_url(), TEST_USER, TEST_PASS);
        let from = Utc::now();
        let to = from + chrono::Duration::days(1);
        assert!(provider.list_events(from, to).await.is_err());
    }

    #[tokio::test]
    async fn create_event_server_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/events");
            then.status(403);
        });
        let provider = CalDavProvider::with_base_url(&server.base_url(), TEST_USER, TEST_PASS);
        let event = CalendarEvent {
            id: Uuid::new_v4(),
            title: "Fail".into(),
            start: Utc::now(),
            end: Utc::now() + chrono::Duration::hours(1),
            location: None,
            description: None,
            is_all_day: false,
            attendees: vec![],
            reminders: vec![],
        };
        assert!(provider.create_event(event).await.is_err());
    }

    #[tokio::test]
    async fn delete_event_not_found() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(DELETE).path_includes("/events/");
            then.status(404);
        });
        let provider = CalDavProvider::with_base_url(&server.base_url(), TEST_USER, TEST_PASS);
        assert!(provider.delete_event(&Uuid::new_v4()).await.is_err());
    }
}
