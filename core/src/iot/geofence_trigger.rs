//! 地理围栏触发器
//!
//! 到家/离家自动化触发。

use serde::{Deserialize, Serialize};

/// 地理围栏区域
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeofenceZone {
    pub name: String,
    pub lat: f64,
    pub lng: f64,
    pub radius_meters: f64,
}

/// 围栏事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeofenceEvent {
    Enter,
    Exit,
    Dwell,
}

/// 围栏触发规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeofenceRule {
    pub zone_name: String,
    pub event: GeofenceEvent,
    pub action_description: String,
    pub enabled: bool,
}

/// 地理围栏触发器
pub struct GeofenceTrigger {
    zones: Vec<GeofenceZone>,
    rules: Vec<GeofenceRule>,
    /// 当前用户位置
    current_lat: f64,
    current_lng: f64,
    /// 上一次所在区域
    last_zone: Option<String>,
}

impl GeofenceTrigger {
    pub fn new() -> Self {
        Self {
            zones: Vec::new(),
            rules: Vec::new(),
            current_lat: 0.0,
            current_lng: 0.0,
            last_zone: None,
        }
    }

    /// 添加围栏区域
    pub fn add_zone(&mut self, zone: GeofenceZone) {
        self.zones.push(zone);
    }

    /// 添加触发规则
    pub fn add_rule(&mut self, rule: GeofenceRule) {
        self.rules.push(rule);
    }

    /// 更新用户位置，返回触发的事件
    pub fn update_location(&mut self, lat: f64, lng: f64) -> Vec<(GeofenceEvent, String)> {
        self.current_lat = lat;
        self.current_lng = lng;

        let current_zone = self.find_zone(lat, lng);
        let mut events = Vec::new();

        match (&self.last_zone, &current_zone) {
            (None, Some(zone)) => {
                events.push((GeofenceEvent::Enter, zone.clone()));
            }
            (Some(prev), None) => {
                events.push((GeofenceEvent::Exit, prev.clone()));
            }
            (Some(prev), Some(curr)) if prev != curr => {
                events.push((GeofenceEvent::Exit, prev.clone()));
                events.push((GeofenceEvent::Enter, curr.clone()));
            }
            _ => {}
        }

        self.last_zone = current_zone;
        events
    }

    /// 获取匹配事件的规则
    pub fn matching_rules(&self, event: GeofenceEvent, zone_name: &str) -> Vec<&GeofenceRule> {
        self.rules
            .iter()
            .filter(|r| r.enabled && r.event == event && r.zone_name == zone_name)
            .collect()
    }

    /// 查找用户当前所在区域
    fn find_zone(&self, lat: f64, lng: f64) -> Option<String> {
        for zone in &self.zones {
            let dist = haversine_distance(lat, lng, zone.lat, zone.lng);
            if dist <= zone.radius_meters {
                return Some(zone.name.clone());
            }
        }
        None
    }

    /// 已注册区域数
    pub fn zone_count(&self) -> usize {
        self.zones.len()
    }

    /// 已注册规则数
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

/// Haversine 公式计算两点距离（米）
fn haversine_distance(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let r = 6_371_000.0; // 地球半径（米）
    let d_lat = (lat2 - lat1).to_radians();
    let d_lng = (lng2 - lng1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}
