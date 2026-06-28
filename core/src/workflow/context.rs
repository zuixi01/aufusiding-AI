//! 工作流执行上下文。

use crate::workflow::triggers::TriggerContext;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default)]
pub struct WorkflowContext {
    data: Map<String, Value>,
    vault: Map<String, Value>,
}

impl WorkflowContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_trigger_context(trigger_context: &TriggerContext) -> Self {
        let mut context = Self::new();
        let event_data = trigger_context
            .event_data
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or_else(|| {
                trigger_context
                    .event_data
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null)
            });

        let mut trigger = Map::new();
        trigger.insert(
            "current_time".to_string(),
            Value::String(trigger_context.current_time.to_rfc3339()),
        );
        trigger.insert(
            "user_input".to_string(),
            trigger_context
                .user_input
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        trigger.insert(
            "event_type".to_string(),
            trigger_context
                .event_type
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        trigger.insert("event_data".to_string(), event_data);
        trigger.insert(
            "device_state".to_string(),
            trigger_context
                .device_state
                .as_ref()
                .map(|state| {
                    Value::Object(Map::from_iter([
                        (
                            "device_id".to_string(),
                            Value::String(state.device_id.clone()),
                        ),
                        ("state".to_string(), Value::String(state.state.clone())),
                    ]))
                })
                .unwrap_or(Value::Null),
        );

        context
            .data
            .insert("trigger".to_string(), Value::Object(trigger));
        context
            .data
            .insert("steps".to_string(), Value::Object(Map::new()));
        context
            .data
            .insert("vault".to_string(), Value::Object(Map::new()));
        context
    }

    pub fn set_path(&mut self, path: &str, value: Value) -> anyhow::Result<()> {
        let segments = parse_path(path)?;
        set_in_map(&mut self.data, &segments, value)
    }

    pub fn get(&self, path: &str) -> Option<&Value> {
        let segments = normalize_path(path);
        if segments.is_empty() {
            return None;
        }
        get_from_map(&self.data, &segments)
    }

    pub fn record_step_output(&mut self, step_key: &str, output: Value) -> anyhow::Result<()> {
        if step_key.trim().is_empty() {
            anyhow::bail!("step_key cannot be empty");
        }
        self.set_path(&format!("steps.{step_key}"), output)
    }

    pub fn insert_vault_secret(&mut self, key: &str, value: Value) -> anyhow::Result<()> {
        let segments = parse_path(key)?;
        set_in_map(&mut self.vault, &segments, value.clone())?;
        self.set_path(&format!("vault.{key}"), value)
    }

    pub fn vault_secret(&self, key: &str) -> Option<&Value> {
        let segments = normalize_path(key);
        if segments.is_empty() {
            return None;
        }
        get_from_map(&self.vault, &segments)
    }

    pub fn render_template(&self, template: &str) -> anyhow::Result<String> {
        let mut rendered = String::new();
        let mut remaining = template;

        while let Some(start) = remaining.find("{{") {
            let (prefix, tail) = remaining.split_at(start);
            rendered.push_str(prefix);
            let Some(end) = tail.find("}}") else {
                anyhow::bail!("unclosed template placeholder");
            };
            let token = tail[2..end].trim();
            let replacement = self.get(token).map(render_value).unwrap_or_default();
            rendered.push_str(&replacement);
            remaining = &tail[end + 2..];
        }

        rendered.push_str(remaining);
        Ok(rendered)
    }

    pub fn snapshot(&self) -> Value {
        Value::Object(self.data.clone())
    }
}

fn normalize_path(path: &str) -> Vec<&str> {
    path.trim()
        .trim_start_matches('.')
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn parse_path(path: &str) -> anyhow::Result<Vec<&str>> {
    let segments = normalize_path(path);
    if segments.is_empty() {
        anyhow::bail!("path cannot be empty");
    }
    Ok(segments)
}

fn set_in_map(
    target: &mut Map<String, Value>,
    segments: &[&str],
    value: Value,
) -> anyhow::Result<()> {
    if segments.is_empty() {
        anyhow::bail!("path cannot be empty");
    }

    let mut current = target;
    for segment in &segments[..segments.len() - 1] {
        let entry = current
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        current = entry
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("path segment is not an object: {segment}"))?;
    }

    current.insert(segments[segments.len() - 1].to_string(), value);
    Ok(())
}

fn get_from_map<'a>(source: &'a Map<String, Value>, segments: &[&str]) -> Option<&'a Value> {
    let mut current = source.get(segments.first().copied()?)?;
    for segment in &segments[1..] {
        current = current.as_object()?.get(*segment)?;
    }
    Some(current)
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(raw) => raw.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::triggers::TriggerContext;
    use chrono::TimeZone;

    #[test]
    fn builds_context_from_trigger_and_renders_nested_values() {
        let trigger = TriggerContext {
            current_time: chrono::Utc.with_ymd_and_hms(2026, 3, 10, 9, 0, 0).unwrap(),
            user_input: Some("daily summary".to_string()),
            event_type: Some("daily.summary".to_string()),
            event_data: Some("{\"summary\":\"Revenue up 12%\"}".to_string()),
            webhook_path: None,
            device_state: None,
            geofence_transition: None,
        };

        let mut context = WorkflowContext::from_trigger_context(&trigger);
        context
            .record_step_output("draft_summary", Value::String("Summary ready".to_string()))
            .unwrap();
        context
            .insert_vault_secret(
                "telegram.bot_token",
                Value::String("secret-123".to_string()),
            )
            .unwrap();

        let rendered = context
            .render_template(
                "Type={{trigger.event_type}}, Summary={{trigger.event_data.summary}}, Draft={{steps.draft_summary}}, Token={{vault.telegram.bot_token}}",
            )
            .unwrap();

        assert_eq!(
            rendered,
            "Type=daily.summary, Summary=Revenue up 12%, Draft=Summary ready, Token=secret-123"
        );
    }

    #[test]
    fn set_path_creates_nested_objects_and_missing_placeholders_render_empty_string() {
        let mut context = WorkflowContext::new();
        context
            .set_path(
                "workflow.metadata.name",
                Value::String("daily-digest".to_string()),
            )
            .unwrap();

        assert_eq!(
            context.get("workflow.metadata.name"),
            Some(&Value::String("daily-digest".to_string()))
        );
        assert_eq!(
            context
                .render_template("Workflow={{workflow.metadata.name}}, Missing={{steps.unknown}}")
                .unwrap(),
            "Workflow=daily-digest, Missing="
        );
    }
}
