//! 导航服务提供者
//!
//! `NavigationProvider` trait + Mock 实现。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 地理位置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub lat: f64,
    pub lng: f64,
    pub name: Option<String>,
}

/// 路线
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub steps: Vec<RouteStep>,
    pub distance_m: f64,
    #[serde(with = "duration_serde")]
    pub duration: Duration,
    pub polyline: String,
}

/// 路线步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteStep {
    pub instruction: String,
    pub distance_m: f64,
    #[serde(with = "duration_serde")]
    pub duration: Duration,
    pub maneuver: String,
}

/// 出行方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelMode {
    Driving,
    Walking,
    Transit,
    Cycling,
}

/// 交通信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficInfo {
    pub congestion_level: f32, // 0.0-1.0
    pub delay_minutes: u32,
}

/// 导航服务提供者 trait
#[async_trait]
pub trait NavigationProvider: Send + Sync {
    async fn geocode(&self, address: &str) -> anyhow::Result<Location>;
    async fn route(&self, from: &Location, to: &Location, mode: TravelMode) -> anyhow::Result<Route>;
    async fn traffic(&self, route: &Route) -> anyhow::Result<TrafficInfo>;
}

/// Mock 导航提供者（测试用）
pub struct MockNavigationProvider;

#[async_trait]
impl NavigationProvider for MockNavigationProvider {
    async fn geocode(&self, address: &str) -> anyhow::Result<Location> {
        Ok(Location {
            lat: 39.9042,
            lng: 116.4074,
            name: Some(address.to_string()),
        })
    }

    async fn route(&self, _from: &Location, _to: &Location, _mode: TravelMode) -> anyhow::Result<Route> {
        Ok(Route {
            steps: vec![
                RouteStep {
                    instruction: "向北行驶 500 米".into(),
                    distance_m: 500.0,
                    duration: Duration::from_secs(60),
                    maneuver: "straight".into(),
                },
                RouteStep {
                    instruction: "右转进入长安街".into(),
                    distance_m: 2000.0,
                    duration: Duration::from_secs(300),
                    maneuver: "turn_right".into(),
                },
            ],
            distance_m: 2500.0,
            duration: Duration::from_secs(360),
            polyline: "mock_polyline".into(),
        })
    }

    async fn traffic(&self, _route: &Route) -> anyhow::Result<TrafficInfo> {
        Ok(TrafficInfo {
            congestion_level: 0.3,
            delay_minutes: 5,
        })
    }
}

mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        d.as_secs().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}
