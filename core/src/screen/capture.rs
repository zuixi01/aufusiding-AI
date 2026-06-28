//! 屏幕捕获 trait + Mock

use super::types::ScreenRegion;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// 图像格式
#[derive(Debug, Clone, PartialEq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Raw,
}

/// 屏幕截图
#[derive(Debug, Clone)]
pub struct ScreenSnapshot {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub format: ImageFormat,
    pub captured_at: DateTime<Utc>,
}

/// 窗口信息
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub title: String,
    pub app_name: String,
    pub bounds: ScreenRegion,
    pub is_focused: bool,
}

#[async_trait]
pub trait ScreenCapture: Send + Sync {
    async fn capture_full(&self) -> anyhow::Result<ScreenSnapshot>;
    async fn capture_region(&self, region: &ScreenRegion) -> anyhow::Result<ScreenSnapshot>;
    async fn get_active_window(&self) -> anyhow::Result<WindowInfo>;
}

/// Mock 屏幕捕获
pub struct MockScreenCapture {
    width: u32,
    height: u32,
    window_title: String,
    app_name: String,
}

impl MockScreenCapture {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            window_title: "Mock Window".into(),
            app_name: "MockApp".into(),
        }
    }

    pub fn with_window(mut self, title: &str, app: &str) -> Self {
        self.window_title = title.into();
        self.app_name = app.into();
        self
    }
}

#[async_trait]
impl ScreenCapture for MockScreenCapture {
    async fn capture_full(&self) -> anyhow::Result<ScreenSnapshot> {
        let size = (self.width * self.height * 4) as usize;
        Ok(ScreenSnapshot {
            width: self.width,
            height: self.height,
            data: vec![0u8; size],
            format: ImageFormat::Raw,
            captured_at: Utc::now(),
        })
    }

    async fn capture_region(&self, region: &ScreenRegion) -> anyhow::Result<ScreenSnapshot> {
        let size = (region.width * region.height * 4) as usize;
        Ok(ScreenSnapshot {
            width: region.width,
            height: region.height,
            data: vec![0u8; size],
            format: ImageFormat::Raw,
            captured_at: Utc::now(),
        })
    }

    async fn get_active_window(&self) -> anyhow::Result<WindowInfo> {
        Ok(WindowInfo {
            title: self.window_title.clone(),
            app_name: self.app_name.clone(),
            bounds: ScreenRegion {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            },
            is_focused: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn capture_full_returns_snapshot() {
        let cap = MockScreenCapture::new(1920, 1080);
        let snap = cap.capture_full().await.unwrap();
        assert_eq!(snap.width, 1920);
        assert_eq!(snap.height, 1080);
        assert_eq!(snap.format, ImageFormat::Raw);
    }

    #[tokio::test]
    async fn capture_region_returns_subset() {
        let cap = MockScreenCapture::new(1920, 1080);
        let region = ScreenRegion {
            x: 100,
            y: 100,
            width: 200,
            height: 150,
        };
        let snap = cap.capture_region(&region).await.unwrap();
        assert_eq!(snap.width, 200);
        assert_eq!(snap.height, 150);
    }

    #[tokio::test]
    async fn get_active_window_info() {
        let cap = MockScreenCapture::new(1920, 1080).with_window("Editor", "VSCode");
        let win = cap.get_active_window().await.unwrap();
        assert_eq!(win.title, "Editor");
        assert_eq!(win.app_name, "VSCode");
        assert!(win.is_focused);
    }

    #[tokio::test]
    async fn mock_configurable() {
        let cap = MockScreenCapture::new(800, 600);
        let snap = cap.capture_full().await.unwrap();
        assert_eq!(snap.data.len(), 800 * 600 * 4);
    }
}
