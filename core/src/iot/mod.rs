//! IoT 设备管理模块
//!
//! 支持智能家居设备的发现、配对和控制。
//! 提供类型安全的设备模型、能力声明和异步事件总线。

pub mod adapter_factory;
pub mod adapters;
pub mod auto_provision;
pub mod automation;
pub mod casting;
pub mod content_analyzer;
pub mod control;
pub mod cron_scheduler;
pub mod device_handoff;
pub mod device_state_cache;
pub mod discovery;
pub mod discovery_aggregator;
pub mod discovery_ble;
pub mod discovery_mdns;
pub mod discovery_provider;
pub mod discovery_simulated;
pub mod discovery_ssdp;
pub mod enhanced_info;
pub mod event_bus;
pub mod event_bus_controller;
pub mod geofence_trigger;
pub mod health;
pub mod output_router;
pub mod pairing;
pub mod registry;
pub mod rule_store;
pub mod scene_executor;
pub mod scenes;
pub mod semantic;
pub mod types;
pub mod vehicle;
pub mod inventory;
pub mod persistent_state_cache;
pub mod persistent_profile_store;

#[cfg(test)]
mod tests_integration;
#[cfg(test)]
mod tests_stress;
#[cfg(test)]
mod tests_error_recovery;
#[cfg(test)]
mod tests_output_router;

pub use control::{DeviceController, GenericController, MockDeviceController};
pub use discovery::{DeviceDiscovery, DeviceInfo};
pub use discovery_aggregator::DiscoveryAggregator;
pub use discovery_provider::DiscoveryProvider;
pub use enhanced_info::EnhancedDeviceInfo;
pub use event_bus::{DeviceEvent, EventBus, EventKind};
pub use pairing::SecurePairing;
pub use registry::DeviceRegistry;
pub use types::{Capability, CapabilitySet, DeviceType, Protocol};
