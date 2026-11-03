//! 协议适配器子模块
//!
//! 每种协议/设备类型一个适配器，全部实现 `DeviceController` trait。

pub mod ble_wearable;
pub mod dlna;
pub mod energy;
pub mod google_home;
pub mod haier;
pub mod hdmi_cec;
pub mod homeassistant;
pub mod homekit;
pub mod intercom;
pub mod ipp;
pub mod ir_bridge;
pub mod matter;
pub mod midea;
pub mod mihome;
pub mod mqtt;
pub mod onvif;
pub mod printer3d;
pub mod projector;
pub mod smart_tv;
pub mod smb;
pub mod tuya;
pub mod vacuum;
pub mod wol;
