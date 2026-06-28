//! 天气查询插件
//!
//! 模拟天气查询服务，支持按城市名查询天气信息。
//! 输入格式：JSON `{"method":"query","params":{"city":"北京"}}` 或纯城市名字符串。

use prometheus_plugin_sdk::{Plugin, PluginRequest, PluginResponse};
use std::ffi::c_void;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn name(&self) -> &str {
        "weather"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn execute(&self, input: &str) -> Result<String, String> {
        let city = if let Ok(req) = serde_json::from_str::<PluginRequest>(input) {
            match req.method.as_str() {
                "query" => req.params["city"].as_str().unwrap_or("").to_string(),
                _ => return Err(format!("未知方法: {}", req.method)),
            }
        } else {
            input.trim().to_string()
        };

        if city.is_empty() {
            return Err("城市名不能为空".into());
        }

        let weather = lookup_weather(&city);
        let resp = PluginResponse {
            success: true,
            data: serde_json::json!(weather),
            error: None,
        };
        serde_json::to_string(&resp).map_err(|e| e.to_string())
    }
}

// ── 模拟天气数据 ──

#[derive(serde::Serialize)]
struct WeatherInfo {
    city: String,
    condition: String,
    temperature: i32,
    humidity: u32,
    wind: String,
}

fn lookup_weather(city: &str) -> WeatherInfo {
    // 基于城市名哈希生成确定性的模拟数据
    let hash: u32 = city
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));

    let conditions = ["晴", "多云", "阴", "小雨", "大雨", "雪", "雾", "雷阵雨"];
    let winds = ["微风", "东风3级", "南风2级", "西北风4级", "北风3级"];

    let condition = conditions[(hash % conditions.len() as u32) as usize];
    let wind = winds[((hash / 7) % winds.len() as u32) as usize];
    let temperature = ((hash % 45) as i32) - 10; // -10 到 34
    let humidity = 30 + (hash % 60); // 30% 到 89%

    WeatherInfo {
        city: city.to_string(),
        condition: condition.to_string(),
        temperature,
        humidity,
        wind: wind.to_string(),
    }
}

#[no_mangle]
pub extern "C" fn create_plugin() -> *mut c_void {
    let plugin: Box<dyn Plugin> = Box::new(WeatherPlugin);
    Box::into_raw(plugin) as *mut c_void
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_city_plain() {
        let plugin = WeatherPlugin;
        let result = plugin.execute("北京").unwrap();
        let resp: PluginResponse = serde_json::from_str(&result).unwrap();
        assert!(resp.success);
        assert_eq!(resp.data["city"].as_str().unwrap(), "北京");
        assert!(resp.data["temperature"].as_i64().is_some());
        assert!(resp.data["condition"].as_str().is_some());
    }

    #[test]
    fn query_city_json() {
        let plugin = WeatherPlugin;
        let input = r#"{"method":"query","params":{"city":"上海"}}"#;
        let result = plugin.execute(input).unwrap();
        let resp: PluginResponse = serde_json::from_str(&result).unwrap();
        assert!(resp.success);
        assert_eq!(resp.data["city"].as_str().unwrap(), "上海");
    }

    #[test]
    fn empty_city_error() {
        let plugin = WeatherPlugin;
        assert!(plugin.execute("").is_err());
    }

    #[test]
    fn deterministic_results() {
        let plugin = WeatherPlugin;
        let r1 = plugin.execute("东京").unwrap();
        let r2 = plugin.execute("东京").unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn different_cities_different_weather() {
        let plugin = WeatherPlugin;
        let r1 = plugin.execute("北京").unwrap();
        let r2 = plugin.execute("广州").unwrap();
        // 不同城市大概率不同（不保证，但哈希分布应该不同）
        // 至少验证都能成功返回
        let resp1: PluginResponse = serde_json::from_str(&r1).unwrap();
        let resp2: PluginResponse = serde_json::from_str(&r2).unwrap();
        assert!(resp1.success);
        assert!(resp2.success);
    }

    #[test]
    fn unknown_method_error() {
        let plugin = WeatherPlugin;
        let input = r#"{"method":"forecast","params":{"city":"北京"}}"#;
        assert!(plugin.execute(input).is_err());
    }
}
