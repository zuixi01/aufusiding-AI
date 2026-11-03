//! 工作流步骤定义与执行。

use crate::workflow::context::WorkflowContext;
use chrono::{DateTime, Utc};
use petgraph::algo::toposort;
use petgraph::graphmap::DiGraphMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepAttemptRecord {
    pub attempt: u32,
    pub status: StepStatus,
    pub output: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionRecord {
    pub step_id: Uuid,
    pub step_name: String,
    pub step_key: String,
    pub status: StepStatus,
    pub attempts: Vec<StepAttemptRecord>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            backoff_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: Uuid,
    pub name: String,
    pub key: String,
    pub step: Step,
    #[serde(default)]
    pub depends_on: Vec<Uuid>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub retry_policy: RetryPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Step {
    LlmCall {
        prompt_template: String,
        model_preference: Option<String>,
    },
    HttpRequest {
        url: String,
        method: String,
        headers: BTreeMap<String, String>,
        body_template: Option<String>,
    },
    Transform {
        jq_expression: String,
    },
    Condition {
        expression: String,
        if_true: Value,
        if_false: Value,
    },
    SendMessage {
        channel: String,
        template: String,
    },
    VaultRead {
        key: String,
    },
}

fn default_timeout_secs() -> u64 {
    30
}

impl WorkflowStep {
    pub fn validate(&self) -> anyhow::Result<()> {
        require_non_empty("step name", &self.name)?;
        require_non_empty("step key", &self.key)?;
        if !self
            .key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            anyhow::bail!("step key must contain only ascii letters, numbers or underscores");
        }
        if self.timeout_secs == 0 {
            anyhow::bail!("timeout_secs must be greater than zero");
        }
        if self.retry_policy.max_attempts == 0 {
            anyhow::bail!("retry max_attempts must be greater than zero");
        }

        let mut seen = HashSet::new();
        for dependency in &self.depends_on {
            if *dependency == self.id {
                anyhow::bail!("step cannot depend on itself");
            }
            if !seen.insert(*dependency) {
                anyhow::bail!("duplicate dependency: {dependency}");
            }
        }

        match &self.step {
            Step::LlmCall {
                prompt_template, ..
            } => require_non_empty("prompt_template", prompt_template)?,
            Step::HttpRequest { url, method, .. } => {
                require_non_empty("url", url)?;
                require_non_empty("method", method)?;
            }
            Step::Transform { jq_expression } => require_non_empty("jq_expression", jq_expression)?,
            Step::Condition { expression, .. } => require_non_empty("expression", expression)?,
            Step::SendMessage { channel, template } => {
                require_non_empty("channel", channel)?;
                require_non_empty("template", template)?;
            }
            Step::VaultRead { key } => require_non_empty("key", key)?,
        }

        Ok(())
    }

    pub async fn execute(&self, context: &WorkflowContext) -> anyhow::Result<Value> {
        self.validate()?;

        match &self.step {
            Step::LlmCall {
                prompt_template,
                model_preference,
            } => {
                let prompt = context.render_template(prompt_template)?;
                Ok(json!({
                    "type": "llm_call",
                    "model_preference": model_preference,
                    "prompt": prompt,
                    "text": format!("Mock LLM response: {}", context.render_template(prompt_template)?),
                }))
            }
            Step::HttpRequest {
                url,
                method,
                headers,
                body_template,
            } => {
                let client = reqwest::Client::new();
                let method = reqwest::Method::from_bytes(method.as_bytes())?;
                let rendered_url = context.render_template(url)?;
                let mut request = client.request(method, rendered_url);

                for (name, value) in headers {
                    request = request.header(name, context.render_template(value)?);
                }
                if let Some(body_template) = body_template {
                    request = request.body(context.render_template(body_template)?);
                }

                let response = request.send().await?;
                let status_code = response.status().as_u16();
                let mut response_headers = serde_json::Map::new();
                for (name, value) in response.headers() {
                    response_headers.insert(
                        name.to_string(),
                        Value::String(value.to_str().unwrap_or_default().to_string()),
                    );
                }
                let body_text = response.text().await?;
                let body =
                    serde_json::from_str::<Value>(&body_text).unwrap_or(Value::String(body_text));

                Ok(json!({
                    "type": "http_request",
                    "status_code": status_code,
                    "headers": response_headers,
                    "body": body,
                }))
            }
            Step::Transform { jq_expression } => evaluate_transform(jq_expression, context),
            Step::Condition {
                expression,
                if_true,
                if_false,
            } => {
                if evaluate_condition(expression, context)? {
                    render_value_templates(if_true, context)
                } else {
                    render_value_templates(if_false, context)
                }
            }
            Step::SendMessage { channel, template } => Ok(json!({
                "type": "send_message",
                "channel": channel,
                "message": context.render_template(template)?,
            })),
            Step::VaultRead { key } => context
                .vault_secret(key)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("vault key not found: {key}")),
        }
    }
}

pub fn topological_order(steps: &[WorkflowStep]) -> anyhow::Result<Vec<WorkflowStep>> {
    let mut graph = DiGraphMap::<Uuid, ()>::new();
    let mut by_id = HashMap::new();

    for step in steps {
        step.validate()?;
        graph.add_node(step.id);
        by_id.insert(step.id, step.clone());
    }

    for step in steps {
        for dependency in &step.depends_on {
            if !by_id.contains_key(dependency) {
                anyhow::bail!("missing dependency: {dependency}");
            }
            graph.add_edge(*dependency, step.id, ());
        }
    }

    let ordered = toposort(&graph, None).map_err(|cycle| {
        anyhow::anyhow!(
            "workflow step graph contains a cycle at {}",
            cycle.node_id()
        )
    })?;

    ordered
        .into_iter()
        .map(|id| {
            by_id
                .get(&id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("step disappeared from graph: {id}"))
        })
        .collect()
}

pub async fn execute_step_with_retry(
    step: &WorkflowStep,
    context: &WorkflowContext,
) -> StepExecutionRecord {
    let started_at = Utc::now();
    let mut attempts = Vec::new();
    let max_attempts = step.retry_policy.max_attempts.max(1);
    let mut final_status = StepStatus::Failed;

    for attempt in 1..=max_attempts {
        let result = timeout(
            Duration::from_secs(step.timeout_secs),
            step.execute(context),
        )
        .await;
        match result {
            Ok(Ok(output)) => {
                attempts.push(StepAttemptRecord {
                    attempt,
                    status: StepStatus::Succeeded,
                    output: Some(output),
                    error: None,
                });
                final_status = StepStatus::Succeeded;
                break;
            }
            Ok(Err(error)) => {
                attempts.push(StepAttemptRecord {
                    attempt,
                    status: StepStatus::Failed,
                    output: None,
                    error: Some(error.to_string()),
                });
            }
            Err(_elapsed) => {
                attempts.push(StepAttemptRecord {
                    attempt,
                    status: StepStatus::Failed,
                    output: None,
                    error: Some(format!("step timed out after {}s", step.timeout_secs)),
                });
            }
        }

        if attempt < max_attempts && step.retry_policy.backoff_ms > 0 {
            sleep(Duration::from_millis(step.retry_policy.backoff_ms)).await;
        }
    }

    let finished_at = Utc::now();
    StepExecutionRecord {
        step_id: step.id,
        step_name: step.name.clone(),
        step_key: step.key.clone(),
        status: final_status,
        attempts,
        started_at,
        finished_at,
    }
}

fn value_as_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn render_value_templates(value: &Value, context: &WorkflowContext) -> anyhow::Result<Value> {
    match value {
        Value::String(text) => Ok(Value::String(context.render_template(text)?)),
        Value::Array(items) => {
            let mut rendered = Vec::with_capacity(items.len());
            for item in items {
                rendered.push(render_value_templates(item, context)?);
            }
            Ok(Value::Array(rendered))
        }
        Value::Object(map) => {
            let mut rendered = serde_json::Map::new();
            for (key, value) in map {
                rendered.insert(key.clone(), render_value_templates(value, context)?);
            }
            Ok(Value::Object(rendered))
        }
        other => Ok(other.clone()),
    }
}

fn require_non_empty(field_name: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{field_name} cannot be empty");
    }
    Ok(())
}

fn evaluate_transform(expression: &str, context: &WorkflowContext) -> anyhow::Result<Value> {
    let trimmed = expression.trim();
    if trimmed.starts_with('.') {
        return context
            .get(trimmed)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("transform path not found: {trimmed}"));
    }

    if trimmed.contains("{{") {
        return Ok(Value::String(context.render_template(trimmed)?));
    }

    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        return Ok(parsed);
    }

    context
        .get(trimmed)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("unsupported transform expression: {trimmed}"))
}

fn evaluate_condition(expression: &str, context: &WorkflowContext) -> anyhow::Result<bool> {
    let trimmed = expression.trim();
    if let Some(args) = function_args(trimmed, "eq")? {
        let left = resolve_operand(&args[0], context)?;
        let right = resolve_operand(&args[1], context)?;
        return Ok(left == right);
    }
    if let Some(args) = function_args(trimmed, "contains")? {
        let left = resolve_operand(&args[0], context)?;
        let needle = resolve_operand(&args[1], context)?;
        return Ok(value_as_string(&left).contains(&value_as_string(&needle)));
    }
    if let Some(args) = function_args(trimmed, "exists")? {
        return Ok(context.get(&args[0]).is_some());
    }
    if let Some(inner) = trimmed
        .strip_prefix("not(")
        .and_then(|raw| raw.strip_suffix(')'))
    {
        return Ok(!evaluate_condition(inner, context)?);
    }

    Ok(context.get(trimmed).is_some_and(truthy))
}

fn function_args(expression: &str, name: &str) -> anyhow::Result<Option<Vec<String>>> {
    let prefix = format!("{name}(");
    let Some(after_prefix) = expression.strip_prefix(&prefix) else {
        return Ok(None);
    };

    // The expression starts with "name(" — it's intended to be a function call.
    // Now validate that it's well-formed.
    let Some(raw_args) = after_prefix.strip_suffix(')') else {
        anyhow::bail!(
            "malformed {name}() expression: missing closing parenthesis in '{expression}'"
        );
    };

    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '\0';
    let mut depth = 0;

    for ch in raw_args.chars() {
        match ch {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = ch;
                current.push(ch);
            }
            '"' | '\'' if in_quotes && ch == quote_char => {
                in_quotes = false;
                current.push(ch);
            }
            '(' if !in_quotes => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_quotes && depth > 0 => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_quotes && depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if in_quotes {
        anyhow::bail!("malformed {name}() expression: unclosed quote in '{expression}'");
    }

    if depth != 0 {
        anyhow::bail!("malformed {name}() expression: unbalanced parentheses in '{expression}'");
    }

    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }

    Ok(Some(args))
}

fn resolve_operand(operand: &str, context: &WorkflowContext) -> anyhow::Result<Value> {
    let trimmed = operand.trim();
    if let Some(inner) = trimmed
        .strip_prefix('"')
        .and_then(|raw| raw.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|raw| raw.strip_suffix('\''))
        })
    {
        return Ok(Value::String(inner.to_string()));
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        return Ok(parsed);
    }
    context
        .get(trimmed)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("unknown operand: {trimmed}"))
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(boolean) => *boolean,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use serde_json::json;

    fn step(
        name: &str,
        key: &str,
        step: Step,
        depends_on: Vec<Uuid>,
        retry_policy: RetryPolicy,
    ) -> WorkflowStep {
        WorkflowStep {
            id: Uuid::new_v4(),
            name: name.to_string(),
            key: key.to_string(),
            step,
            depends_on,
            timeout_secs: 30,
            retry_policy,
        }
    }

    #[test]
    fn topological_order_respects_dependencies_and_rejects_cycles() {
        let fetch = step(
            "Fetch",
            "fetch",
            Step::Transform {
                jq_expression: ".trigger.event_data.summary".to_string(),
            },
            vec![],
            RetryPolicy::default(),
        );
        let summarize = step(
            "Summarize",
            "summarize",
            Step::LlmCall {
                prompt_template: "{{steps.fetch}}".to_string(),
                model_preference: Some("local".to_string()),
            },
            vec![fetch.id],
            RetryPolicy::default(),
        );
        let send = step(
            "Send",
            "send",
            Step::SendMessage {
                channel: "telegram".to_string(),
                template: "{{steps.summarize.text}}".to_string(),
            },
            vec![summarize.id],
            RetryPolicy::default(),
        );

        let ordered = topological_order(&[send.clone(), summarize.clone(), fetch.clone()]).unwrap();
        assert_eq!(
            ordered
                .iter()
                .map(|step| step.key.as_str())
                .collect::<Vec<_>>(),
            vec!["fetch", "summarize", "send"]
        );

        let fetch_with_cycle = WorkflowStep {
            depends_on: vec![send.id],
            ..fetch.clone()
        };
        let error = topological_order(&[send, summarize, fetch_with_cycle]).unwrap_err();
        assert!(error.to_string().contains("cycle"));
    }

    #[tokio::test]
    async fn executes_http_request_step_with_rendered_templates() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/summary")
                .header("authorization", "Bearer secret-123")
                .body("{\"prompt\":\"Summary ready\"}");
            then.status(200)
                .header("content-type", "application/json")
                .body("{\"message\":\"queued\"}");
        });

        let mut context = WorkflowContext::new();
        context
            .record_step_output("draft_summary", Value::String("Summary ready".to_string()))
            .unwrap();
        context
            .insert_vault_secret(
                "telegram.bot_token",
                Value::String("secret-123".to_string()),
            )
            .unwrap();

        let step = step(
            "Notify",
            "notify",
            Step::HttpRequest {
                url: server.url("/summary"),
                method: "POST".to_string(),
                headers: BTreeMap::from([(
                    "authorization".to_string(),
                    "Bearer {{vault.telegram.bot_token}}".to_string(),
                )]),
                body_template: Some("{\"prompt\":\"{{steps.draft_summary}}\"}".to_string()),
            },
            vec![],
            RetryPolicy::default(),
        );

        let output = step.execute(&context).await.unwrap();
        mock.assert();
        assert_eq!(output["status_code"], 200);
        assert_eq!(output["body"]["message"], "queued");
    }

    #[tokio::test]
    async fn condition_step_branches_using_context_expression() {
        let mut context = WorkflowContext::new();
        context
            .set_path(
                "trigger.event_type",
                Value::String("daily.summary".to_string()),
            )
            .unwrap();

        let step = step(
            "Branch",
            "branch",
            Step::Transform {
                jq_expression: ".steps.send".to_string(),
            },
            vec![],
            RetryPolicy::default(),
        );

        let step = WorkflowStep {
            step: Step::Condition {
                expression: "eq(trigger.event_type, \"daily.summary\")".to_string(),
                if_true: json!("daily"),
                if_false: json!("other"),
            },
            ..step
        };

        assert_eq!(step.execute(&context).await.unwrap(), json!("daily"));
    }
}
