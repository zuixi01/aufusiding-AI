//! 电子表格工具
//!
//! 读写 CSV、公式计算（SUM/AVG/COUNT/MIN/MAX）、数据透视。
//! 实现 AgentTool trait。

use async_trait::async_trait;
use serde_json::Value;

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};

/// 电子表格工具
pub struct SpreadsheetTool;

impl SpreadsheetTool {
    pub fn new() -> Self {
        Self
    }

    /// 读取 CSV 为结构化数据
    pub fn read_csv(data: &str) -> Value {
        let mut rows: Vec<Vec<String>> = Vec::new();
        for line in data.lines() {
            if line.trim().is_empty() {
                continue;
            }
            rows.push(line.split(',').map(|s| s.trim().to_string()).collect());
        }
        if rows.is_empty() {
            return serde_json::json!({"headers": [], "rows": []});
        }
        let headers = rows[0].clone();
        let data_rows: Vec<Vec<String>> = rows.into_iter().skip(1).collect();
        serde_json::json!({"headers": headers, "rows": data_rows})
    }

    /// 将结构化数据写为 CSV 字符串
    pub fn write_csv(headers: &[String], rows: &[Vec<String>]) -> String {
        let mut output = headers.join(",");
        for row in rows {
            output.push('\n');
            output.push_str(&row.join(","));
        }
        output
    }

    /// 公式计算
    pub fn formula(values: &[f64], op: &str) -> Option<f64> {
        if values.is_empty() {
            return None;
        }
        match op.to_uppercase().as_str() {
            "SUM" => Some(values.iter().sum()),
            "AVG" => Some(values.iter().sum::<f64>() / values.len() as f64),
            "COUNT" => Some(values.len() as f64),
            "MIN" => values.iter().cloned().reduce(f64::min),
            "MAX" => values.iter().cloned().reduce(f64::max),
            _ => None,
        }
    }

    /// 数据透视：按 group_by 列分组，对 aggregate 列执行聚合
    pub fn pivot(
        headers: &[String],
        rows: &[Vec<String>],
        group_by: &str,
        aggregate_col: &str,
        aggregate_fn: &str,
    ) -> Value {
        let group_idx = headers.iter().position(|h| h == group_by);
        let agg_idx = headers.iter().position(|h| h == aggregate_col);

        let (group_idx, agg_idx) = match (group_idx, agg_idx) {
            (Some(g), Some(a)) => (g, a),
            _ => return serde_json::json!({"error": "column not found"}),
        };

        let mut groups: std::collections::HashMap<String, Vec<f64>> =
            std::collections::HashMap::new();
        for row in rows {
            if let (Some(key), Some(val_str)) = (row.get(group_idx), row.get(agg_idx)) {
                if let Ok(val) = val_str.parse::<f64>() {
                    groups.entry(key.clone()).or_default().push(val);
                }
            }
        }

        let mut result = serde_json::Map::new();
        for (key, values) in &groups {
            if let Some(v) = Self::formula(values, aggregate_fn) {
                result.insert(key.clone(), serde_json::json!(v));
            }
        }
        Value::Object(result)
    }
}

#[async_trait]
impl AgentTool for SpreadsheetTool {
    fn name(&self) -> &str {
        "spreadsheet"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "spreadsheet".into(),
            description: "读写 CSV 数据、公式计算、数据透视表".into(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["read_csv", "write_csv", "formula", "pivot"]},
                    "data": {"type": "string"},
                    "headers": {"type": "array", "items": {"type": "string"}},
                    "rows": {"type": "array"},
                    "values": {"type": "array", "items": {"type": "number"}},
                    "operation": {"type": "string"},
                    "group_by": {"type": "string"},
                    "aggregate_col": {"type": "string"},
                    "aggregate_fn": {"type": "string"}
                },
                "required": ["action"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let action = match params.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("missing 'action' parameter"),
        };

        match action {
            "read_csv" => {
                let data = params.get("data").and_then(|v| v.as_str()).unwrap_or("");
                ToolResult::ok(Self::read_csv(data))
            }
            "write_csv" => {
                let headers: Vec<String> = params
                    .get("headers")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let rows: Vec<Vec<String>> = params
                    .get("rows")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let csv = Self::write_csv(&headers, &rows);
                ToolResult::ok(serde_json::json!({"csv": csv}))
            }
            "formula" => {
                let values: Vec<f64> = params
                    .get("values")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let op = params
                    .get("operation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("SUM");
                match Self::formula(&values, op) {
                    Some(v) => ToolResult::ok(serde_json::json!({"result": v})),
                    None => ToolResult::err("formula computation failed"),
                }
            }
            "pivot" => {
                let data = params.get("data").and_then(|v| v.as_str()).unwrap_or("");
                let parsed = Self::read_csv(data);
                let headers: Vec<String> = parsed
                    .get("headers")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let rows: Vec<Vec<String>> = parsed
                    .get("rows")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let group_by = params
                    .get("group_by")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let agg_col = params
                    .get("aggregate_col")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let agg_fn = params
                    .get("aggregate_fn")
                    .and_then(|v| v.as_str())
                    .unwrap_or("SUM");
                let result = Self::pivot(&headers, &rows, group_by, agg_col, agg_fn);
                ToolResult::ok(result)
            }
            _ => ToolResult::err(format!("unknown action: {}", action)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_csv_basic() {
        let result = SpreadsheetTool::read_csv("name,score\nAlice,90\nBob,85");
        assert_eq!(result["headers"][0], "name");
        assert_eq!(result["rows"][0][1], "90");
    }

    #[test]
    fn read_csv_empty() {
        let result = SpreadsheetTool::read_csv("");
        assert!(result["headers"].as_array().unwrap().is_empty());
    }

    #[test]
    fn write_csv_basic() {
        let headers = vec!["name".into(), "score".into()];
        let rows = vec![vec!["Alice".into(), "90".into()]];
        let csv = SpreadsheetTool::write_csv(&headers, &rows);
        assert!(csv.contains("name,score"));
        assert!(csv.contains("Alice,90"));
    }

    #[test]
    fn formula_sum() {
        assert_eq!(
            SpreadsheetTool::formula(&[10.0, 20.0, 30.0], "SUM"),
            Some(60.0)
        );
    }

    #[test]
    fn formula_avg() {
        assert_eq!(
            SpreadsheetTool::formula(&[10.0, 20.0, 30.0], "AVG"),
            Some(20.0)
        );
    }

    #[test]
    fn formula_count() {
        assert_eq!(
            SpreadsheetTool::formula(&[1.0, 2.0, 3.0], "COUNT"),
            Some(3.0)
        );
    }

    #[test]
    fn formula_min() {
        assert_eq!(SpreadsheetTool::formula(&[5.0, 2.0, 8.0], "MIN"), Some(2.0));
    }

    #[test]
    fn formula_max() {
        assert_eq!(SpreadsheetTool::formula(&[5.0, 2.0, 8.0], "MAX"), Some(8.0));
    }

    #[test]
    fn formula_empty() {
        assert_eq!(SpreadsheetTool::formula(&[], "SUM"), None);
    }

    #[test]
    fn formula_unknown_op() {
        assert_eq!(SpreadsheetTool::formula(&[1.0], "SQRT"), None);
    }

    #[test]
    fn pivot_basic() {
        let headers = vec!["dept".into(), "sales".into()];
        let rows = vec![
            vec!["A".into(), "100".into()],
            vec!["B".into(), "200".into()],
            vec!["A".into(), "150".into()],
        ];
        let result = SpreadsheetTool::pivot(&headers, &rows, "dept", "sales", "SUM");
        assert_eq!(result["A"], 250.0);
        assert_eq!(result["B"], 200.0);
    }

    #[test]
    fn pivot_column_not_found() {
        let headers = vec!["name".into()];
        let rows = vec![vec!["Alice".into()]];
        let result = SpreadsheetTool::pivot(&headers, &rows, "dept", "sales", "SUM");
        assert!(result["error"].as_str().is_some());
    }

    #[tokio::test]
    async fn execute_read_csv() {
        let tool = SpreadsheetTool::new();
        let result = tool
            .execute(serde_json::json!({
                "action": "read_csv",
                "data": "a,b\n1,2"
            }))
            .await;
        assert!(result.success);
        assert_eq!(result.output["headers"][0], "a");
    }

    #[tokio::test]
    async fn execute_write_csv() {
        let tool = SpreadsheetTool::new();
        let result = tool
            .execute(serde_json::json!({
                "action": "write_csv",
                "headers": ["x", "y"],
                "rows": [["1", "2"]]
            }))
            .await;
        assert!(result.success);
        assert!(result.output["csv"].as_str().unwrap().contains("x,y"));
    }

    #[tokio::test]
    async fn execute_formula() {
        let tool = SpreadsheetTool::new();
        let result = tool
            .execute(serde_json::json!({
                "action": "formula",
                "values": [10, 20, 30],
                "operation": "AVG"
            }))
            .await;
        assert!(result.success);
        assert_eq!(result.output["result"], 20.0);
    }

    #[tokio::test]
    async fn execute_pivot() {
        let tool = SpreadsheetTool::new();
        let result = tool
            .execute(serde_json::json!({
                "action": "pivot",
                "data": "dept,sales\nA,100\nB,200\nA,50",
                "group_by": "dept",
                "aggregate_col": "sales",
                "aggregate_fn": "SUM"
            }))
            .await;
        assert!(result.success);
        assert_eq!(result.output["A"], 150.0);
    }

    #[tokio::test]
    async fn execute_missing_action() {
        let tool = SpreadsheetTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn execute_unknown_action() {
        let tool = SpreadsheetTool::new();
        let result = tool.execute(serde_json::json!({"action": "sort"})).await;
        assert!(!result.success);
    }

    #[test]
    fn tool_name_and_descriptor() {
        let tool = SpreadsheetTool::new();
        assert_eq!(tool.name(), "spreadsheet");
        let desc = tool.descriptor();
        assert_eq!(desc.name, "spreadsheet");
    }
}
