//! 能力桥接器
//!
//! 将能力插件桥接为 Agent 可用工具。

use super::capability_registry::CapabilityRegistry;

/// 工具描述（简化版，桥接到 Agent 工具系统）
#[derive(Debug, Clone)]
pub struct BridgedTool {
    pub name: String,
    pub description: String,
    pub capability_name: String,
}

/// 能力桥接器
pub struct CapabilityBridge {
    tools: Vec<BridgedTool>,
}

impl CapabilityBridge {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// 从注册表生成桥接工具
    pub fn build_from_registry(&mut self, registry: &CapabilityRegistry) {
        self.tools.clear();

        for cap in registry.list_enabled() {
            let tools = Self::tools_for_capability(&cap.name);
            self.tools.extend(tools);
        }
    }

    /// 获取所有桥接工具
    pub fn tools(&self) -> &[BridgedTool] {
        &self.tools
    }

    /// 按能力名称查找工具
    pub fn tools_for(&self, capability_name: &str) -> Vec<&BridgedTool> {
        self.tools
            .iter()
            .filter(|t| t.capability_name == capability_name)
            .collect()
    }

    /// 工具总数
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    fn tools_for_capability(name: &str) -> Vec<BridgedTool> {
        match name {
            "browser" => vec![
                BridgedTool {
                    name: "browser_navigate".into(),
                    description: "打开网页".into(),
                    capability_name: "browser".into(),
                },
                BridgedTool {
                    name: "browser_extract".into(),
                    description: "提取网页内容".into(),
                    capability_name: "browser".into(),
                },
                BridgedTool {
                    name: "browser_screenshot".into(),
                    description: "网页截图".into(),
                    capability_name: "browser".into(),
                },
            ],
            "navigation" => vec![
                BridgedTool {
                    name: "nav_plan_route".into(),
                    description: "规划路线".into(),
                    capability_name: "navigation".into(),
                },
                BridgedTool {
                    name: "nav_geocode".into(),
                    description: "地址转坐标".into(),
                    capability_name: "navigation".into(),
                },
            ],
            "app_control" => vec![
                BridgedTool {
                    name: "app_launch".into(),
                    description: "启动应用".into(),
                    capability_name: "app_control".into(),
                },
                BridgedTool {
                    name: "app_type_text".into(),
                    description: "输入文本".into(),
                    capability_name: "app_control".into(),
                },
            ],
            "file_manager" => vec![
                BridgedTool {
                    name: "file_search".into(),
                    description: "搜索文件".into(),
                    capability_name: "file_manager".into(),
                },
                BridgedTool {
                    name: "file_organize".into(),
                    description: "整理文件".into(),
                    capability_name: "file_manager".into(),
                },
            ],
            _ => vec![],
        }
    }
}
