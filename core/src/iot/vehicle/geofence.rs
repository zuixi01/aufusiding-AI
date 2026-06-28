//! 地理围栏引擎
//!
//! 基于 Haversine 公式的位置检测，支持进入/离开/停留事件。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 地理围栏区域
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoZone {
    /// 区域 ID
    pub id: String,
    /// 区域名称
    pub name: String,
    /// 中心纬度
    pub latitude: f64,
    /// 中心经度
    pub longitude: f64,
    /// 半径（米）
    pub radius_meters: f64,
}

/// 地理围栏转换事件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeoTransition {
    /// 进入区域
    Enter,
    /// 离开区域
    Exit,
    /// 停留在区域内
    Dwell,
}

/// 地理围栏引擎
pub struct GeofenceEngine {
    zones: Vec<GeoZone>,
    /// device_id → 当前所在的 zone_id 集合
    memberships: HashMap<String, HashSet<String>>,
}

impl GeofenceEngine {
    pub fn new() -> Self {
        Self {
            zones: Vec::new(),
            memberships: HashMap::new(),
        }
    }

    /// 添加地理围栏区域
    pub fn add_zone(&mut self, zone: GeoZone) {
        self.zones.push(zone);
    }

    /// 获取所有区域
    pub fn zones(&self) -> &[GeoZone] {
        &self.zones
    }

    /// 检查位置并返回转换事件
    ///
    /// 对每个区域判断设备是否在内部，与上次状态对比产生 Enter/Exit/Dwell 事件。
    pub fn check_location(
        &mut self,
        device_id: &str,
        lat: f64,
        lng: f64,
    ) -> Vec<(String, GeoTransition)> {
        let current_set = self.memberships.entry(device_id.to_string()).or_default();

        let mut transitions = Vec::new();

        for zone in &self.zones {
            let distance = haversine_distance(lat, lng, zone.latitude, zone.longitude);
            let inside = distance <= zone.radius_meters;
            let was_inside = current_set.contains(&zone.id);

            match (was_inside, inside) {
                (false, true) => {
                    current_set.insert(zone.id.clone());
                    transitions.push((zone.id.clone(), GeoTransition::Enter));
                }
                (true, false) => {
                    current_set.remove(&zone.id);
                    transitions.push((zone.id.clone(), GeoTransition::Exit));
                }
                (true, true) => {
                    transitions.push((zone.id.clone(), GeoTransition::Dwell));
                }
                (false, false) => {
                    // 不在区域内，无事件
                }
            }
        }

        transitions
    }
}

/// Haversine 公式计算两点间距离（米）
pub fn haversine_distance(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlng = (lng2 - lng1).to_radians();

    let a =
        (dlat / 2.0).sin().powi(2) + lat1_rad.cos() * lat2_rad.cos() * (dlng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_M * c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_paris_to_london() {
        // Paris (48.8566, 2.3522) → London (51.5074, -0.1278) ≈ 343 km
        let dist = haversine_distance(48.8566, 2.3522, 51.5074, -0.1278);
        let dist_km = dist / 1000.0;
        assert!(
            (dist_km - 343.0).abs() < 5.0,
            "expected ~343 km, got {dist_km}"
        );
    }

    #[test]
    fn haversine_same_point_is_zero() {
        let dist = haversine_distance(37.7749, -122.4194, 37.7749, -122.4194);
        assert!(dist.abs() < 0.01);
    }

    #[test]
    fn add_zone_and_zones() {
        let mut engine = GeofenceEngine::new();
        assert!(engine.zones().is_empty());

        engine.add_zone(GeoZone {
            id: "home".into(),
            name: "Home".into(),
            latitude: 37.7749,
            longitude: -122.4194,
            radius_meters: 100.0,
        });
        assert_eq!(engine.zones().len(), 1);
        assert_eq!(engine.zones()[0].id, "home");
    }

    #[test]
    fn check_location_enter_transition() {
        let mut engine = GeofenceEngine::new();
        engine.add_zone(GeoZone {
            id: "office".into(),
            name: "Office".into(),
            latitude: 37.7749,
            longitude: -122.4194,
            radius_meters: 200.0,
        });

        // 进入区域
        let transitions = engine.check_location("car-1", 37.7749, -122.4194);
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].0, "office");
        assert_eq!(transitions[0].1, GeoTransition::Enter);
    }

    #[test]
    fn check_location_exit_transition() {
        let mut engine = GeofenceEngine::new();
        engine.add_zone(GeoZone {
            id: "office".into(),
            name: "Office".into(),
            latitude: 37.7749,
            longitude: -122.4194,
            radius_meters: 200.0,
        });

        // 先进入
        engine.check_location("car-1", 37.7749, -122.4194);
        // 再离开（远处坐标）
        let transitions = engine.check_location("car-1", 38.0, -123.0);
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].0, "office");
        assert_eq!(transitions[0].1, GeoTransition::Exit);
    }

    #[test]
    fn check_location_dwell_transition() {
        let mut engine = GeofenceEngine::new();
        engine.add_zone(GeoZone {
            id: "office".into(),
            name: "Office".into(),
            latitude: 37.7749,
            longitude: -122.4194,
            radius_meters: 200.0,
        });

        // 进入
        engine.check_location("car-1", 37.7749, -122.4194);
        // 仍在区域内（微小移动）
        let transitions = engine.check_location("car-1", 37.7750, -122.4195);
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].0, "office");
        assert_eq!(transitions[0].1, GeoTransition::Dwell);
    }

    #[test]
    fn multiple_zones_simultaneously() {
        let mut engine = GeofenceEngine::new();
        engine.add_zone(GeoZone {
            id: "zone-a".into(),
            name: "Zone A".into(),
            latitude: 37.7749,
            longitude: -122.4194,
            radius_meters: 500.0,
        });
        engine.add_zone(GeoZone {
            id: "zone-b".into(),
            name: "Zone B".into(),
            latitude: 37.7750,
            longitude: -122.4190,
            radius_meters: 500.0,
        });

        // 两个区域重叠，同时进入
        let transitions = engine.check_location("car-1", 37.7749, -122.4194);
        assert_eq!(transitions.len(), 2);
        assert!(transitions.iter().all(|(_, t)| *t == GeoTransition::Enter));

        let zone_ids: Vec<&str> = transitions.iter().map(|(id, _)| id.as_str()).collect();
        assert!(zone_ids.contains(&"zone-a"));
        assert!(zone_ids.contains(&"zone-b"));
    }

    #[test]
    fn no_transition_when_outside() {
        let mut engine = GeofenceEngine::new();
        engine.add_zone(GeoZone {
            id: "office".into(),
            name: "Office".into(),
            latitude: 37.7749,
            longitude: -122.4194,
            radius_meters: 100.0,
        });

        // 远离区域
        let transitions = engine.check_location("car-1", 40.0, -120.0);
        assert!(transitions.is_empty());
    }
}
