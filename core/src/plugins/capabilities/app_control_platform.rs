//! 平台适配层
//!
//! `PlatformController` trait + Mock 实现。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 应用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub pid: u32,
    pub is_foreground: bool,
}

/// 平台控制器 trait
#[async_trait]
pub trait PlatformController: Send + Sync {
    async fn launch(&self, app_id: &str) -> anyhow::Result<()>;
    async fn focus(&self, app_id: &str) -> anyhow::Result<()>;
    async fn close(&self, app_id: &str) -> anyhow::Result<()>;
    async fn input_text(&self, text: &str) -> anyhow::Result<()>;
    async fn running_apps(&self) -> anyhow::Result<Vec<AppInfo>>;
    fn platform_name(&self) -> &str;
}

/// Mock 平台控制器（测试用）
pub struct MockPlatformController {
    apps: std::sync::Mutex<Vec<AppInfo>>,
    typed: std::sync::Mutex<Vec<String>>,
}

impl MockPlatformController {
    pub fn new() -> Self {
        Self {
            apps: std::sync::Mutex::new(Vec::new()),
            typed: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn typed_texts(&self) -> Vec<String> {
        self.typed.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[async_trait]
impl PlatformController for MockPlatformController {
    async fn launch(&self, app_id: &str) -> anyhow::Result<()> {
        let mut apps = self.apps.lock().unwrap_or_else(|e| e.into_inner());
        if apps.iter().any(|a| a.name == app_id) {
            anyhow::bail!("应用已运行: {}", app_id);
        }
        let pid = apps.len() as u32 + 1000;
        apps.push(AppInfo {
            name: app_id.to_string(),
            pid,
            is_foreground: true,
        });
        // 其他应用设为后台
        let len = apps.len();
        for (i, app) in apps.iter_mut().enumerate() {
            app.is_foreground = i == len - 1;
        }
        Ok(())
    }

    async fn focus(&self, app_id: &str) -> anyhow::Result<()> {
        let mut apps = self.apps.lock().unwrap_or_else(|e| e.into_inner());
        let found = apps.iter().any(|a| a.name == app_id);
        if !found {
            anyhow::bail!("应用未运行: {}", app_id);
        }
        for app in apps.iter_mut() {
            app.is_foreground = app.name == app_id;
        }
        Ok(())
    }

    async fn close(&self, app_id: &str) -> anyhow::Result<()> {
        let mut apps = self.apps.lock().unwrap_or_else(|e| e.into_inner());
        let idx = apps
            .iter()
            .position(|a| a.name == app_id)
            .ok_or_else(|| anyhow::anyhow!("应用未运行: {}", app_id))?;
        apps.remove(idx);
        Ok(())
    }

    async fn input_text(&self, text: &str) -> anyhow::Result<()> {
        self.typed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(text.to_string());
        Ok(())
    }

    async fn running_apps(&self) -> anyhow::Result<Vec<AppInfo>> {
        Ok(self.apps.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    fn platform_name(&self) -> &str {
        "mock"
    }
}
