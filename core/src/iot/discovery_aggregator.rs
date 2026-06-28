//! 发现聚合器
//!
//! 聚合多个 `DiscoveryProvider`，并行扫描，去重合并结果。

use std::collections::HashMap;
use std::sync::Arc;

use super::discovery::DeviceInfo;
use super::discovery_provider::DiscoveryProvider;
use super::event_bus::{DeviceEvent, EventBus, EventKind};
use super::types::DeviceType;

/// 发现聚合器 — 并行运行多个 provider，去重
pub struct DiscoveryAggregator {
    providers: Vec<Box<dyn DiscoveryProvider>>,
    event_bus: Option<Arc<EventBus>>,
}

impl DiscoveryAggregator {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            event_bus: None,
        }
    }

    /// 设置事件总线（发现新设备时发布事件）
    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// 注册发现后端
    pub fn add_provider(&mut self, provider: Box<dyn DiscoveryProvider>) {
        self.providers.push(provider);
    }

    /// 并行扫描所有后端，去重合并
    pub async fn scan_all(&self, timeout_secs: u64) -> anyhow::Result<Vec<DeviceInfo>> {
        let mut handles = Vec::new();

        for provider in &self.providers {
            if !provider.is_available() {
                continue;
            }
            // 由于 provider 是 &dyn，不能 move 到 spawn，
            // 这里顺序执行（实际生产中可用 Arc 包装）
            let result = provider.scan(timeout_secs).await;
            handles.push((provider.name().to_string(), result));
        }

        let mut seen: HashMap<String, DeviceInfo> = HashMap::new();
        for (_name, result) in handles {
            if let Ok(devices) = result {
                for device in devices {
                    let key = format!("{}:{}", device.ip, device.port);
                    seen.entry(key).or_insert(device);
                }
            }
        }

        let devices: Vec<DeviceInfo> = seen.into_values().collect();

        // 发布发现事件
        if let Some(bus) = &self.event_bus {
            for device in &devices {
                let dt = DeviceType::from(device.device_type.as_str());
                bus.publish(DeviceEvent::new(
                    &device.id,
                    dt,
                    EventKind::DeviceDiscovered,
                    serde_json::json!({
                        "name": device.name,
                        "ip": device.ip,
                        "port": device.port,
                    }),
                ));
            }
        }

        Ok(devices)
    }

    /// 已注册的 provider 数量
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::discovery_provider::MockDiscoveryProvider;

    #[tokio::test]
    async fn aggregator_with_mock_provider() {
        let mut agg = DiscoveryAggregator::new();
        agg.add_provider(Box::new(MockDiscoveryProvider::new()));
        assert_eq!(agg.provider_count(), 1);

        let devices = agg.scan_all(1).await.unwrap();
        assert!(!devices.is_empty());
    }

    #[tokio::test]
    async fn aggregator_deduplicates() {
        let mut agg = DiscoveryAggregator::new();
        // 两个 mock provider 返回相同设备
        agg.add_provider(Box::new(MockDiscoveryProvider::new()));
        agg.add_provider(Box::new(MockDiscoveryProvider::new()));

        let devices = agg.scan_all(1).await.unwrap();
        // 去重后数量应等于单个 provider 的数量
        let single = MockDiscoveryProvider::new().scan(1).await.unwrap();
        assert_eq!(devices.len(), single.len());
    }

    #[tokio::test]
    async fn aggregator_empty_no_providers() {
        let agg = DiscoveryAggregator::new();
        let devices = agg.scan_all(1).await.unwrap();
        assert!(devices.is_empty());
    }

    #[tokio::test]
    async fn aggregator_publishes_events() {
        let bus = Arc::new(EventBus::new(64));
        let mut rx = bus.subscribe();

        let mut agg = DiscoveryAggregator::new().with_event_bus(bus);
        agg.add_provider(Box::new(MockDiscoveryProvider::new()));
        let devices = agg.scan_all(1).await.unwrap();

        // 应收到与设备数量相同的发现事件
        for _ in 0..devices.len() {
            let event = rx.try_recv().unwrap();
            assert_eq!(event.event_kind, EventKind::DeviceDiscovered);
        }
    }
}
