//! 数据分析工具
//!
//! 提供 CSV/JSON 数据解析、统计计算（均值/中位数/增长率）、
//! 图表数据生成。实现 AgentTool trait。

use async_trait::async_trait;
use serde_json::Value;

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};

/// 数据分析工具
pub struct DataAnalysisTool;

impl DataAnalysisTool {
    pub fn new() -> Self {
        Self
    }

    /// 解析 CSV 字符串为行列数据
    pub fn parse_csv(data: &str) -> Vec<Vec<String>> {
        data.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| line.split(',').map(|s| s.trim().to_string()).collect())
            .collect()
    }

    /// 计算均值
    pub fn compute_mean(values: &[f64]) -> Option<f64> {
        if values.is_empty() {
            return None;
        }
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }

    /// 计算中位数
    pub fn compute_median(values: &[f64]) -> Option<f64> {
        if values.is_empty() {
            return None;
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = sorted.len() / 2;
        if sorted.len().is_multiple_of(2) {
            Some((sorted[mid - 1] + sorted[mid]) / 2.0)
        } else {
            Some(sorted[mid])
        }
    }

    /// 计算增长率 (%)
    pub fn compute_growth_rate(values: &[f64]) -> Option<f64> {
        if values.len() < 2 {
            return None;
        }
        let first = values[0];
        let last = values[values.len() - 1];
        if first.abs() < f64::EPSILON {
            return None;
        }
        Some((last - first) / first * 100.0)
    }

    /// 生成图表数据（labels + datasets）
    pub fn generate_chart_data(labels: &[String], values: &[f64]) -> Value {
        serde_json::json!({
            "labels": labels,
            "datasets": [{
                "data": values,
                "type": "line"
            }]
        })
    }

    /// 从 CSV 列提取数值
    fn extract_column_values(rows: &[Vec<String>], col_index: usize) -> Vec<f64> {
        rows.iter()
            .skip(1) // skip header
            .filter_map(|row| row.get(col_index)?.parse::<f64>().ok())
            .collect()
    }

    fn execute_analysis(data: &str, format: &str, operations: &[String]) -> Value {
        let values = match format {
            "csv" => {
                let rows = Self::parse_csv(data);
                if rows.len() < 2 || rows[0].is_empty() {
                    return serde_json::json!({"error": "insufficient data"});
                }
                // use last column as default numeric column
                let col = rows[0].len() - 1;
                Self::extract_column_values(&rows, col)
            }
            "json" => serde_json::from_str::<Vec<f64>>(data).unwrap_or_default(),
            _ => return serde_json::json!({"error": "unsupported format"}),
        };

        let mut result = serde_json::Map::new();
        for op in operations {
            match op.as_str() {
                "mean" => {
                    if let Some(v) = Self::compute_mean(&values) {
                        result.insert("mean".into(), serde_json::json!(v));
                    }
                }
                "median" => {
                    if let Some(v) = Self::compute_median(&values) {
                        result.insert("median".into(), serde_json::json!(v));
                    }
                }
                "growth" => {
                    if let Some(v) = Self::compute_growth_rate(&values) {
                        result.insert("growth_rate".into(), serde_json::json!(v));
                    }
                }
                _ => {}
            }
        }
        result.insert("count".into(), serde_json::json!(values.len()));
        Value::Object(result)
    }
}

#[async_trait]
impl AgentTool for DataAnalysisTool {
    fn name(&self) -> &str {
        "data_analysis"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "data_analysis".into(),
            description: "分析 CSV/JSON 数据，计算统计指标（均值、中位数、增长率）".into(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "data": {"type": "string", "description": "原始数据"},
                    "format": {"type": "string", "enum": ["csv", "json"]},
                    "operations": {"type": "array", "items": {"type": "string", "enum": ["mean", "median", "growth"]}}
                },
                "required": ["data", "format", "operations"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let data = match params.get("data").and_then(|v| v.as_str()) {
            Some(d) => d,
            None => return ToolResult::err("missing 'data' parameter"),
        };
        let format = params
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("csv");
        let operations: Vec<String> = params
            .get("operations")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let result = Self::execute_analysis(data, format, &operations);
        ToolResult::ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csv_basic() {
        let csv = "name,value\nA,10\nB,20\nC,30";
        let rows = DataAnalysisTool::parse_csv(csv);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], vec!["name", "value"]);
        assert_eq!(rows[1][1], "10");
    }

    #[test]
    fn parse_csv_empty() {
        let rows = DataAnalysisTool::parse_csv("");
        assert!(rows.is_empty());
    }

    #[test]
    fn compute_mean_basic() {
        assert_eq!(
            DataAnalysisTool::compute_mean(&[10.0, 20.0, 30.0]),
            Some(20.0)
        );
    }

    #[test]
    fn compute_mean_empty() {
        assert_eq!(DataAnalysisTool::compute_mean(&[]), None);
    }

    #[test]
    fn compute_median_odd() {
        assert_eq!(
            DataAnalysisTool::compute_median(&[3.0, 1.0, 2.0]),
            Some(2.0)
        );
    }

    #[test]
    fn compute_median_even() {
        assert_eq!(
            DataAnalysisTool::compute_median(&[1.0, 2.0, 3.0, 4.0]),
            Some(2.5)
        );
    }

    #[test]
    fn compute_median_empty() {
        assert_eq!(DataAnalysisTool::compute_median(&[]), None);
    }

    #[test]
    fn compute_growth_rate_basic() {
        let rate = DataAnalysisTool::compute_growth_rate(&[100.0, 150.0]).unwrap();
        assert!((rate - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_growth_rate_insufficient() {
        assert_eq!(DataAnalysisTool::compute_growth_rate(&[100.0]), None);
    }

    #[test]
    fn compute_growth_rate_zero_start() {
        assert_eq!(DataAnalysisTool::compute_growth_rate(&[0.0, 100.0]), None);
    }

    #[test]
    fn generate_chart_data_structure() {
        let labels = vec!["Q1".into(), "Q2".into()];
        let values = vec![100.0, 200.0];
        let chart = DataAnalysisTool::generate_chart_data(&labels, &values);
        assert_eq!(chart["labels"][0], "Q1");
        assert_eq!(chart["datasets"][0]["data"][1], 200.0);
    }

    #[tokio::test]
    async fn execute_csv_mean() {
        let tool = DataAnalysisTool::new();
        let result = tool
            .execute(serde_json::json!({
                "data": "name,score\nA,10\nB,20\nC,30",
                "format": "csv",
                "operations": ["mean"]
            }))
            .await;
        assert!(result.success);
        assert_eq!(result.output["mean"], 20.0);
        assert_eq!(result.output["count"], 3);
    }

    #[tokio::test]
    async fn execute_json_median() {
        let tool = DataAnalysisTool::new();
        let result = tool
            .execute(serde_json::json!({
                "data": "[3,1,2,5,4]",
                "format": "json",
                "operations": ["median"]
            }))
            .await;
        assert!(result.success);
        assert_eq!(result.output["median"], 3.0);
    }

    #[tokio::test]
    async fn execute_missing_data() {
        let tool = DataAnalysisTool::new();
        let result = tool.execute(serde_json::json!({"format": "csv"})).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn execute_unsupported_format() {
        let tool = DataAnalysisTool::new();
        let result = tool
            .execute(serde_json::json!({
                "data": "hello",
                "format": "xml",
                "operations": ["mean"]
            }))
            .await;
        assert!(result.success);
        assert!(result.output["error"]
            .as_str()
            .unwrap()
            .contains("unsupported"));
    }

    #[test]
    fn tool_name_and_descriptor() {
        let tool = DataAnalysisTool::new();
        assert_eq!(tool.name(), "data_analysis");
        let desc = tool.descriptor();
        assert_eq!(desc.name, "data_analysis");
    }
}
