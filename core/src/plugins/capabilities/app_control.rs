//! 应用控制能力
//!
//! 跨平台应用控制：启动、切换、关闭、输入。

use super::app_control_platform::{AppInfo, PlatformController};

/// 应用控制能力
pub struct AppControlCapability {
    platform: Box<dyn PlatformController>,
}

impl AppControlCapability {
    pub fn new(platform: Box<dyn PlatformController>) -> Self {
        Self { platform }
    }

    pub async fn launch_app(&self, app_name: &str) -> anyhow::Result<()> {
        self.platform.launch(app_name).await
    }

    pub async fn switch_to_app(&self, app_name: &str) -> anyhow::Result<()> {
        self.platform.focus(app_name).await
    }

    pub async fn close_app(&self, app_name: &str) -> anyhow::Result<()> {
        self.platform.close(app_name).await
    }

    pub async fn type_text(&self, text: &str) -> anyhow::Result<()> {
        self.platform.input_text(text).await
    }

    pub async fn list_running_apps(&self) -> anyhow::Result<Vec<AppInfo>> {
        self.platform.running_apps().await
    }

    pub fn platform_name(&self) -> &str {
        self.platform.platform_name()
    }
}
