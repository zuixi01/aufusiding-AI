//! 计算器插件
//!
//! 支持四则运算、括号、浮点数的数学表达式求值。
//! 输入格式：JSON `{"method":"eval","params":{"expr":"1+2*3"}}` 或纯表达式字符串。

use prometheus_plugin_sdk::{Plugin, PluginRequest, PluginResponse};
use std::ffi::c_void;

pub struct CalculatorPlugin;

impl Plugin for CalculatorPlugin {
    fn name(&self) -> &str {
        "calculator"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn execute(&self, input: &str) -> Result<String, String> {
        // 尝试解析为 PluginRequest JSON
        let expr = if let Ok(req) = serde_json::from_str::<PluginRequest>(input) {
            match req.method.as_str() {
                "eval" => req.params["expr"].as_str().unwrap_or("").to_string(),
                _ => return Err(format!("未知方法: {}", req.method)),
            }
        } else {
            // 纯表达式字符串
            input.trim().to_string()
        };

        if expr.is_empty() {
            return Err("表达式不能为空".into());
        }

        match eval_expr(&expr) {
            Ok(result) => {
                let resp = PluginResponse {
                    success: true,
                    data: serde_json::json!({ "result": result, "expr": expr }),
                    error: None,
                };
                serde_json::to_string(&resp).map_err(|e| e.to_string())
            }
            Err(e) => Err(e),
        }
    }
}

// ── 递归下降表达式解析器 ──

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_whitespace();
        self.chars.get(self.pos).copied()
    }

    fn consume(&mut self) -> Option<char> {
        self.skip_whitespace();
        if self.pos < self.chars.len() {
            let ch = self.chars[self.pos];
            self.pos += 1;
            Some(ch)
        } else {
            None
        }
    }

    // expr = term (('+' | '-') term)*
    fn parse_expr(&mut self) -> Result<f64, String> {
        let mut left = self.parse_term()?;
        loop {
            match self.peek() {
                Some('+') => {
                    self.consume();
                    left += self.parse_term()?;
                }
                Some('-') => {
                    self.consume();
                    left -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    // term = unary (('*' | '/') unary)*
    fn parse_term(&mut self) -> Result<f64, String> {
        let mut left = self.parse_unary()?;
        loop {
            match self.peek() {
                Some('*') => {
                    self.consume();
                    left *= self.parse_unary()?;
                }
                Some('/') => {
                    self.consume();
                    let right = self.parse_unary()?;
                    if right == 0.0 {
                        return Err("除零错误".into());
                    }
                    left /= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    // unary = '-' unary | primary
    fn parse_unary(&mut self) -> Result<f64, String> {
        if self.peek() == Some('-') {
            self.consume();
            Ok(-self.parse_unary()?)
        } else {
            self.parse_primary()
        }
    }

    // primary = '(' expr ')' | number
    fn parse_primary(&mut self) -> Result<f64, String> {
        if self.peek() == Some('(') {
            self.consume();
            let val = self.parse_expr()?;
            if self.consume() != Some(')') {
                return Err("缺少右括号".into());
            }
            return Ok(val);
        }
        self.parse_number()
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        let start = self.pos;
        while self.pos < self.chars.len()
            && (self.chars[self.pos].is_ascii_digit() || self.chars[self.pos] == '.')
        {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(format!("期望数字，位置 {}", self.pos));
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse::<f64>().map_err(|e| format!("数字解析错误: {e}"))
    }
}

fn eval_expr(input: &str) -> Result<f64, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("表达式不能为空".into());
    }
    let mut parser = Parser::new(trimmed);
    let result = parser.parse_expr()?;
    parser.skip_whitespace();
    if parser.pos < parser.chars.len() {
        return Err(format!("意外字符: '{}'", parser.chars[parser.pos]));
    }
    Ok(result)
}

#[no_mangle]
pub extern "C" fn create_plugin() -> *mut c_void {
    let plugin: Box<dyn Plugin> = Box::new(CalculatorPlugin);
    Box::into_raw(plugin) as *mut c_void
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_arithmetic() {
        assert_eq!(eval_expr("2+3").unwrap(), 5.0);
        assert_eq!(eval_expr("10-4").unwrap(), 6.0);
        assert_eq!(eval_expr("3*7").unwrap(), 21.0);
        assert_eq!(eval_expr("15/3").unwrap(), 5.0);
    }

    #[test]
    fn operator_precedence() {
        assert_eq!(eval_expr("2+3*4").unwrap(), 14.0);
        assert_eq!(eval_expr("10-2*3").unwrap(), 4.0);
    }

    #[test]
    fn parentheses() {
        assert_eq!(eval_expr("(2+3)*4").unwrap(), 20.0);
        assert_eq!(eval_expr("((1+2)*(3+4))").unwrap(), 21.0);
    }

    #[test]
    fn floating_point() {
        assert!((eval_expr("3.14*2").unwrap() - 6.280_000_000_000_001).abs() < 1e-10);
        assert!((eval_expr("1/3").unwrap() - 0.333333).abs() < 0.001);
    }

    #[test]
    fn negative_numbers() {
        assert_eq!(eval_expr("-5+3").unwrap(), -2.0);
        assert_eq!(eval_expr("-(2+3)").unwrap(), -5.0);
    }

    #[test]
    fn division_by_zero() {
        assert!(eval_expr("1/0").is_err());
    }

    #[test]
    fn empty_expr_error() {
        assert!(eval_expr("").is_err());
    }

    #[test]
    fn plugin_execute_json() {
        let plugin = CalculatorPlugin;
        let input = r#"{"method":"eval","params":{"expr":"2+3"}}"#;
        let result = plugin.execute(input).unwrap();
        let resp: PluginResponse = serde_json::from_str(&result).unwrap();
        assert!(resp.success);
        assert_eq!(resp.data["result"].as_f64().unwrap(), 5.0);
    }

    #[test]
    fn plugin_execute_plain() {
        let plugin = CalculatorPlugin;
        let result = plugin.execute("(1+2)*3").unwrap();
        let resp: PluginResponse = serde_json::from_str(&result).unwrap();
        assert!(resp.success);
        assert_eq!(resp.data["result"].as_f64().unwrap(), 9.0);
    }

    #[test]
    fn whitespace_handling() {
        assert_eq!(eval_expr(" 2 + 3 * 4 ").unwrap(), 14.0);
    }
}
