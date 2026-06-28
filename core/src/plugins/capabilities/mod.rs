//! 插件能力子模块
//!
//! 浏览器自动化、地图导航、应用控制、文件管理四大能力。

pub mod app_control;
pub mod app_control_platform;
pub mod browser;
pub mod browser_actions;
pub mod browser_pool;
pub mod file_indexer;
pub mod file_manager;
pub mod navigation;
pub mod navigation_provider;
pub mod email_client;
pub mod email_rest;

#[cfg(test)]
mod tests_browser;
#[cfg(test)]
mod tests_navigation;
