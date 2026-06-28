//! 导航能力
//!
//! 路线规划 + 语音导航播报。

use std::time::Duration;

use super::navigation_provider::{Location, NavigationProvider, Route, TravelMode};

/// 导航能力
pub struct NavigationCapability {
    provider: Box<dyn NavigationProvider>,
}

impl NavigationCapability {
    pub fn new(provider: Box<dyn NavigationProvider>) -> Self {
        Self { provider }
    }

    /// 规划路线
    pub async fn plan_route(
        &self,
        from: &Location,
        to: &Location,
        mode: TravelMode,
    ) -> anyhow::Result<Route> {
        self.provider.route(from, to, mode).await
    }

    /// 地理编码（地址 → 坐标）
    pub async fn geocode(&self, address: &str) -> anyhow::Result<Location> {
        self.provider.geocode(address).await
    }

    /// 预计到达时间
    pub fn eta(&self, route: &Route) -> Duration {
        route.duration
    }

    /// 生成语音导航播报文本
    pub fn voice_instructions(&self, route: &Route) -> Vec<String> {
        let mut instructions = Vec::new();
        let total_km = route.distance_m / 1000.0;
        let mins = route.duration.as_secs() / 60;

        instructions.push(format!(
            "全程 {:.1} 公里，预计 {} 分钟到达。",
            total_km, mins
        ));

        for step in &route.steps {
            instructions.push(step.instruction.clone());
        }

        instructions.push("您已到达目的地。".into());
        instructions
    }
}
