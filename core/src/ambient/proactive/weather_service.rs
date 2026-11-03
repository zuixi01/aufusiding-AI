//! 天气服务
//!
//! 天气查询接口 + Mock 实现。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 天气信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherInfo {
    pub condition: String,
    pub temperature_c: f32,
    pub humidity_percent: u8,
    pub wind_speed_kmh: f32,
    pub description: String,
}

/// 天气服务 trait
#[async_trait]
pub trait WeatherProvider: Send + Sync {
    async fn current(&self, city: &str) -> anyhow::Result<WeatherInfo>;
    async fn forecast(&self, city: &str, days: usize) -> anyhow::Result<Vec<WeatherInfo>>;
}

/// 天气服务（包装 provider）
pub struct WeatherService {
    provider: Box<dyn WeatherProvider>,
}

impl WeatherService {
    pub fn new(provider: Box<dyn WeatherProvider>) -> Self {
        Self { provider }
    }

    pub async fn current(&self, city: &str) -> anyhow::Result<WeatherInfo> {
        self.provider.current(city).await
    }

    pub async fn forecast(&self, city: &str, days: usize) -> anyhow::Result<Vec<WeatherInfo>> {
        self.provider.forecast(city, days).await
    }

    /// 生成语音友好的天气摘要
    pub async fn voice_summary(&self, city: &str) -> anyhow::Result<String> {
        let w = self.provider.current(city).await?;
        Ok(format!(
            "{}，气温 {} 度，湿度 {}%，{}。",
            w.condition, w.temperature_c as i32, w.humidity_percent, w.description
        ))
    }
}

/// OpenWeatherMap HTTP 天气提供者
pub struct HttpWeatherProvider {
    client: reqwest::Client,
    api_key: String,
    default_city: String,
}

impl HttpWeatherProvider {
    pub fn new(api_key: &str, default_city: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.to_string(),
            default_city: default_city.to_string(),
        }
    }

    pub fn default_city(&self) -> &str {
        &self.default_city
    }
}

/// OpenWeatherMap API 响应结构
#[derive(Debug, Deserialize)]
struct OwmResponse {
    weather: Vec<OwmWeather>,
    main: OwmMain,
    wind: OwmWind,
}

#[derive(Debug, Deserialize)]
struct OwmWeather {
    main: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct OwmMain {
    temp: f32,
    humidity: u8,
}

#[derive(Debug, Deserialize)]
struct OwmWind {
    speed: f32,
}

/// OpenWeatherMap 5-day forecast 响应
#[derive(Debug, Deserialize)]
struct OwmForecastResponse {
    list: Vec<OwmForecastItem>,
}

#[derive(Debug, Deserialize)]
struct OwmForecastItem {
    weather: Vec<OwmWeather>,
    main: OwmMain,
    wind: OwmWind,
    dt_txt: String,
}

#[async_trait]
impl WeatherProvider for HttpWeatherProvider {
    async fn current(&self, city: &str) -> anyhow::Result<WeatherInfo> {
        if self.api_key.is_empty() {
            anyhow::bail!(
                "OpenWeatherMap API key 未设置，请通过环境变量 OWM_API_KEY 或构造函数提供"
            );
        }

        let url = format!(
            "https://api.openweathermap.org/data/2.5/weather?q={}&appid={}&units=metric&lang=zh_cn",
            city, self.api_key
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("天气 API 请求失败: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("天气 API 返回错误 ({}): {}", status, body);
        }

        let data: OwmResponse = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("天气 API 响应解析失败: {}", e))?;

        let weather = data.weather.first();
        let condition = weather
            .map(|w| w.main.clone())
            .unwrap_or_else(|| "未知".into());
        let description = weather
            .map(|w| w.description.clone())
            .unwrap_or_else(|| format!("{}天气信息", city));

        Ok(WeatherInfo {
            condition,
            temperature_c: data.main.temp,
            humidity_percent: data.main.humidity,
            wind_speed_kmh: data.wind.speed * 3.6, // m/s -> km/h
            description,
        })
    }

    async fn forecast(&self, city: &str, days: usize) -> anyhow::Result<Vec<WeatherInfo>> {
        if self.api_key.is_empty() {
            anyhow::bail!("OpenWeatherMap API key 未设置");
        }

        let url = format!(
            "https://api.openweathermap.org/data/2.5/forecast?q={}&appid={}&units=metric&lang=zh_cn",
            city, self.api_key
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("OpenWeatherMap forecast API 请求失败: {}", resp.status());
        }

        let data: OwmForecastResponse = resp.json().await?;

        // 每天取 12:00 的数据点（或当天第一个数据点）
        let mut daily: Vec<WeatherInfo> = Vec::new();
        let mut seen_dates = std::collections::HashSet::new();

        for item in &data.list {
            let date = item.dt_txt.split(' ').next().unwrap_or("");
            if seen_dates.contains(date) && !item.dt_txt.contains("12:00") {
                continue;
            }
            if seen_dates.insert(date.to_string()) || item.dt_txt.contains("12:00") {
                let weather = item.weather.first();
                daily.push(WeatherInfo {
                    condition: weather
                        .map(|w| w.main.clone())
                        .unwrap_or_else(|| "未知".into()),
                    temperature_c: item.main.temp,
                    humidity_percent: item.main.humidity,
                    wind_speed_kmh: item.wind.speed * 3.6,
                    description: weather.map(|w| w.description.clone()).unwrap_or_default(),
                });
            }
            if daily.len() >= days.min(5) {
                break;
            }
        }

        Ok(daily)
    }
}

/// Mock 天气提供者
pub struct MockWeatherProvider;

#[async_trait]
impl WeatherProvider for MockWeatherProvider {
    async fn current(&self, city: &str) -> anyhow::Result<WeatherInfo> {
        Ok(WeatherInfo {
            condition: "多云".into(),
            temperature_c: 22.0,
            humidity_percent: 65,
            wind_speed_kmh: 12.0,
            description: format!("{}今天多云转晴", city),
        })
    }

    async fn forecast(&self, city: &str, days: usize) -> anyhow::Result<Vec<WeatherInfo>> {
        Ok((0..days)
            .map(|i| WeatherInfo {
                condition: if i % 2 == 0 {
                    "晴".into()
                } else {
                    "多云".into()
                },
                temperature_c: 20.0 + i as f32,
                humidity_percent: 60 + i as u8 * 5,
                wind_speed_kmh: 10.0,
                description: format!("{}第{}天天气", city, i + 1),
            })
            .collect())
    }
}
