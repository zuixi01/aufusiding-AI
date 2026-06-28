//! 系统屏幕捕获适配器
//!
//! `SystemScreenCapture` 通过调用系统截图工具 (scrot/grim/screencapture)
//! 实现 `ScreenCapture` trait。

use async_trait::async_trait;
use chrono::Utc;
use std::path::PathBuf;
use tokio::process::Command;

use super::capture::{ImageFormat, ScreenCapture, ScreenSnapshot, WindowInfo};
use super::types::ScreenRegion;

/// 系统截图工具类型
#[derive(Debug, Clone, PartialEq)]
enum CaptureBackend {
    /// Linux X11 (scrot)
    Scrot,
    /// Linux Wayland (grim)
    Grim,
    /// macOS (screencapture)
    Screencapture,
}

/// 系统屏幕捕获适配器
///
/// 自动检测可用的截图后端，通过 `tokio::process::Command`
/// 调用系统工具完成截图。
pub struct SystemScreenCapture {
    backend: CaptureBackend,
    temp_dir: PathBuf,
}

impl SystemScreenCapture {
    /// 创建新实例，自动检测后端
    pub fn new() -> Self {
        let backend = Self::detect_backend();
        Self {
            backend,
            temp_dir: std::env::temp_dir(),
        }
    }

    /// 指定后端和临时目录
    pub fn with_temp_dir(mut self, dir: PathBuf) -> Self {
        self.temp_dir = dir;
        self
    }

    /// 检测可用的截图后端
    fn detect_backend() -> CaptureBackend {
        if cfg!(target_os = "macos") {
            CaptureBackend::Screencapture
        } else if std::env::var("WAYLAND_DISPLAY").is_ok() {
            CaptureBackend::Grim
        } else {
            CaptureBackend::Scrot
        }
    }

    /// 生成临时文件路径
    fn temp_path(&self) -> PathBuf {
        let name = format!("prometheus_capture_{}.png", uuid::Uuid::new_v4());
        self.temp_dir.join(name)
    }

    /// 构建全屏截图命令
    fn build_full_command(&self, output: &std::path::Path) -> Command {
        let out = output.to_string_lossy().to_string();
        match &self.backend {
            CaptureBackend::Scrot => {
                let mut cmd = Command::new("scrot");
                cmd.arg(&out);
                cmd
            }
            CaptureBackend::Grim => {
                let mut cmd = Command::new("grim");
                cmd.arg(&out);
                cmd
            }
            CaptureBackend::Screencapture => {
                let mut cmd = Command::new("screencapture");
                cmd.args(["-x", &out]);
                cmd
            }
        }
    }

    /// 构建区域截图命令
    fn build_region_command(&self, region: &ScreenRegion, output: &std::path::Path) -> Command {
        let out = output.to_string_lossy().to_string();
        let geometry = format!(
            "{}x{}+{}+{}",
            region.width, region.height, region.x, region.y
        );
        match &self.backend {
            CaptureBackend::Scrot => {
                let mut cmd = Command::new("scrot");
                cmd.args(["-a", &geometry, &out]);
                cmd
            }
            CaptureBackend::Grim => {
                let mut cmd = Command::new("grim");
                cmd.args(["-g", &geometry, &out]);
                cmd
            }
            CaptureBackend::Screencapture => {
                let rect = format!(
                    "{},{},{},{}",
                    region.x,
                    region.y,
                    region.x + region.width,
                    region.y + region.height
                );
                let mut cmd = Command::new("screencapture");
                cmd.args(["-x", "-R", &rect, &out]);
                cmd
            }
        }
    }

    /// 执行命令并读取输出文件
    async fn run_and_read(&self, mut cmd: Command, path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
        let status = cmd
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!(
                "截图命令失败 (backend={:?}, exit={:?})",
                self.backend,
                status.code()
            );
        }

        let data = tokio::fs::read(path).await?;
        let _ = tokio::fs::remove_file(path).await;
        Ok(data)
    }
}

#[async_trait]
impl ScreenCapture for SystemScreenCapture {
    async fn capture_full(&self) -> anyhow::Result<ScreenSnapshot> {
        let path = self.temp_path();
        let cmd = self.build_full_command(&path);
        let data = self.run_and_read(cmd, &path).await?;

        Ok(ScreenSnapshot {
            width: 0,  // PNG 解码后填充实际值
            height: 0,
            data,
            format: ImageFormat::Png,
            captured_at: Utc::now(),
        })
    }

    async fn capture_region(&self, region: &ScreenRegion) -> anyhow::Result<ScreenSnapshot> {
        let path = self.temp_path();
        let cmd = self.build_region_command(region, &path);
        let data = self.run_and_read(cmd, &path).await?;

        Ok(ScreenSnapshot {
            width: region.width,
            height: region.height,
            data,
            format: ImageFormat::Png,
            captured_at: Utc::now(),
        })
    }

    async fn get_active_window(&self) -> anyhow::Result<WindowInfo> {
        let output = Command::new("xdotool")
            .args(["getactivewindow", "getwindowname"])
            .output()
            .await?;

        if !output.status.success() {
            anyhow::bail!("xdotool 获取活动窗口失败");
        }

        let title = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let geo_output = Command::new("xdotool")
            .args(["getactivewindow", "getwindowgeometry", "--shell"])
            .output()
            .await?;

        let geo_text = String::from_utf8_lossy(&geo_output.stdout);
        let (x, y, w, h) = Self::parse_xdotool_geometry(&geo_text);

        Ok(WindowInfo {
            title,
            app_name: String::new(),
            bounds: ScreenRegion {
                x,
                y,
                width: w,
                height: h,
            },
            is_focused: true,
        })
    }
}

impl SystemScreenCapture {
    /// 解析 xdotool --shell 几何输出
    fn parse_xdotool_geometry(text: &str) -> (u32, u32, u32, u32) {
        let mut x = 0u32;
        let mut y = 0u32;
        let mut w = 0u32;
        let mut h = 0u32;

        for line in text.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("X=") {
                x = val.parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("Y=") {
                y = val.parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("WIDTH=") {
                w = val.parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("HEIGHT=") {
                h = val.parse().unwrap_or(0);
            }
        }
        (x, y, w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_backend_returns_valid() {
        let cap = SystemScreenCapture::new();
        // 在任何平台上都应返回一个有效后端
        assert!(
            cap.backend == CaptureBackend::Scrot
                || cap.backend == CaptureBackend::Grim
                || cap.backend == CaptureBackend::Screencapture
        );
    }

    #[test]
    fn parse_xdotool_geometry_valid() {
        let text = "WINDOW=12345\nX=100\nY=200\nWIDTH=800\nHEIGHT=600\n";
        let (x, y, w, h) = SystemScreenCapture::parse_xdotool_geometry(text);
        assert_eq!(x, 100);
        assert_eq!(y, 200);
        assert_eq!(w, 800);
        assert_eq!(h, 600);
    }

    #[test]
    fn parse_xdotool_geometry_empty() {
        let (x, y, w, h) = SystemScreenCapture::parse_xdotool_geometry("");
        assert_eq!((x, y, w, h), (0, 0, 0, 0));
    }

    #[test]
    fn temp_path_is_unique() {
        let cap = SystemScreenCapture::new();
        let p1 = cap.temp_path();
        let p2 = cap.temp_path();
        assert_ne!(p1, p2);
    }
}
