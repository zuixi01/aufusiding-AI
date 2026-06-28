//! 系统无障碍桥接适配器
//!
//! `SystemAccessibilityBridge` 通过 AT-SPI / D-Bus 调用实现
//! `AccessibilityBridge` trait，提供 Linux 桌面无障碍访问能力。

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::process::Command;

use super::accessibility::{AccessAction, AccessibilityBridge};
use super::types::{ElementType, ScreenRegion, UIElement};

/// AT-SPI 后端类型
#[derive(Debug, Clone, PartialEq)]
enum AccessibilityBackend {
    /// Linux AT-SPI2 via dbus-send / busctl
    AtSpi,
    /// macOS Accessibility API (osascript)
    MacOs,
}

/// 系统无障碍桥接
///
/// 通过 `tokio::process::Command` 调用系统无障碍工具
/// (AT-SPI2 / D-Bus on Linux, osascript on macOS) 获取 UI 元素树。
pub struct SystemAccessibilityBridge {
    backend: AccessibilityBackend,
}

impl SystemAccessibilityBridge {
    pub fn new() -> Self {
        let backend = if cfg!(target_os = "macos") {
            AccessibilityBackend::MacOs
        } else {
            AccessibilityBackend::AtSpi
        };
        Self { backend }
    }

    /// 解析 AT-SPI 输出为 UIElement 列表
    fn parse_atspi_output(output: &str) -> Vec<UIElement> {
        let mut elements = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // 期望格式: "role|name|x|y|w|h"
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 6 {
                continue;
            }

            let element_type = Self::map_role(parts[0]);
            let label = parts[1].to_string();
            let x = parts[2].parse().unwrap_or(0);
            let y = parts[3].parse().unwrap_or(0);
            let width = parts[4].parse().unwrap_or(0);
            let height = parts[5].parse().unwrap_or(0);

            elements.push(UIElement {
                id: format!("atspi-{}", elements.len()),
                element_type,
                label,
                bounds: ScreenRegion {
                    x,
                    y,
                    width,
                    height,
                },
                children: vec![],
                properties: HashMap::new(),
            });
        }

        elements
    }

    /// 将 AT-SPI 角色映射为 ElementType
    fn map_role(role: &str) -> ElementType {
        match role.to_lowercase().as_str() {
            "push button" | "button" => ElementType::Button,
            "text" | "entry" | "password text" => ElementType::TextField,
            "label" | "static" => ElementType::Label,
            "image" | "icon" => ElementType::Image,
            "panel" | "filler" | "frame" | "dialog" => ElementType::Container,
            "list" | "tree" | "table" => ElementType::List,
            "toggle button" | "check box" => ElementType::Toggle,
            "slider" | "spin button" => ElementType::Slider,
            "link" => ElementType::Link,
            _ => ElementType::Unknown,
        }
    }

    /// 将 AccessAction 转换为 AT-SPI 动作名
    fn action_to_atspi_name(action: &AccessAction) -> &str {
        match action {
            AccessAction::Click => "click",
            AccessAction::Focus => "focus",
            AccessAction::SetValue(_) => "set_value",
            AccessAction::Scroll(_, _) => "scroll",
        }
    }
}

impl Default for SystemAccessibilityBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AccessibilityBridge for SystemAccessibilityBridge {
    async fn get_element_tree(&self) -> anyhow::Result<Vec<UIElement>> {
        let output = match &self.backend {
            AccessibilityBackend::AtSpi => {
                Command::new("python3")
                    .args([
                        "-c",
                        "import subprocess; print(subprocess.check_output(['gdbus', 'call', '--session', '--dest', 'org.a11y.Bus', '--object-path', '/org/a11y/bus', '--method', 'org.a11y.Bus.GetAddress']).decode())",
                    ])
                    .output()
                    .await?
            }
            AccessibilityBackend::MacOs => {
                Command::new("osascript")
                    .args(["-e", "tell application \"System Events\" to get every UI element of front window of first application process whose frontmost is true"])
                    .output()
                    .await?
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let elements = Self::parse_atspi_output(&stdout);

        Ok(elements)
    }

    async fn get_focused_element(&self) -> anyhow::Result<Option<UIElement>> {
        let output = Command::new("xdotool")
            .args(["getactivewindow", "getwindowname"])
            .output()
            .await?;

        if !output.status.success() {
            return Ok(None);
        }

        let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if title.is_empty() {
            return Ok(None);
        }

        Ok(Some(UIElement {
            id: "focused-window".to_string(),
            element_type: ElementType::Container,
            label: title,
            bounds: ScreenRegion {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            children: vec![],
            properties: HashMap::new(),
        }))
    }

    async fn perform_action(
        &self,
        element_id: &str,
        action: AccessAction,
    ) -> anyhow::Result<()> {
        let action_name = Self::action_to_atspi_name(&action);

        let status = Command::new("dbus-send")
            .args([
                "--session",
                "--type=method_call",
                &format!("--dest=org.a11y.atspi.{}", element_id),
                "/org/a11y/atspi/accessible",
                "org.a11y.atspi.Action.DoAction",
                &format!("string:{}", action_name),
            ])
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!(
                "无障碍动作执行失败: element={}, action={}",
                element_id,
                action_name
            );
        }

        Ok(())
    }

    async fn find_element(&self, query: &str) -> anyhow::Result<Vec<UIElement>> {
        let tree = self.get_element_tree().await?;
        let q = query.to_lowercase();

        let matched = tree
            .into_iter()
            .filter(|e| {
                e.label.to_lowercase().contains(&q) || e.id.to_lowercase().contains(&q)
            })
            .collect();

        Ok(matched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_atspi_output_valid() {
        let output = "push button|OK|10|20|80|30\ntext|Name|10|60|200|25\n";
        let elements = SystemAccessibilityBridge::parse_atspi_output(output);

        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].element_type, ElementType::Button);
        assert_eq!(elements[0].label, "OK");
        assert_eq!(elements[0].bounds.x, 10);
        assert_eq!(elements[1].element_type, ElementType::TextField);
        assert_eq!(elements[1].label, "Name");
    }

    #[test]
    fn parse_atspi_output_empty() {
        let elements = SystemAccessibilityBridge::parse_atspi_output("");
        assert!(elements.is_empty());
    }

    #[test]
    fn map_role_covers_types() {
        assert_eq!(
            SystemAccessibilityBridge::map_role("push button"),
            ElementType::Button
        );
        assert_eq!(
            SystemAccessibilityBridge::map_role("entry"),
            ElementType::TextField
        );
        assert_eq!(
            SystemAccessibilityBridge::map_role("label"),
            ElementType::Label
        );
        assert_eq!(
            SystemAccessibilityBridge::map_role("panel"),
            ElementType::Container
        );
        assert_eq!(
            SystemAccessibilityBridge::map_role("list"),
            ElementType::List
        );
        assert_eq!(
            SystemAccessibilityBridge::map_role("toggle button"),
            ElementType::Toggle
        );
        assert_eq!(
            SystemAccessibilityBridge::map_role("slider"),
            ElementType::Slider
        );
        assert_eq!(
            SystemAccessibilityBridge::map_role("link"),
            ElementType::Link
        );
        assert_eq!(
            SystemAccessibilityBridge::map_role("unknown_role"),
            ElementType::Unknown
        );
    }

    #[test]
    fn action_to_atspi_name_maps_correctly() {
        assert_eq!(
            SystemAccessibilityBridge::action_to_atspi_name(&AccessAction::Click),
            "click"
        );
        assert_eq!(
            SystemAccessibilityBridge::action_to_atspi_name(&AccessAction::Focus),
            "focus"
        );
        assert_eq!(
            SystemAccessibilityBridge::action_to_atspi_name(&AccessAction::SetValue(
                "test".into()
            )),
            "set_value"
        );
        assert_eq!(
            SystemAccessibilityBridge::action_to_atspi_name(&AccessAction::Scroll(0, 100)),
            "scroll"
        );
    }
}
