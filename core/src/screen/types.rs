//! 屏幕代理类型定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 屏幕区域
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScreenRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ScreenRegion {
    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// UI 元素类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ElementType {
    Button,
    TextField,
    Label,
    Image,
    Container,
    List,
    Toggle,
    Slider,
    Link,
    Unknown,
}

/// UI 元素
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIElement {
    pub id: String,
    pub element_type: ElementType,
    pub label: String,
    pub bounds: ScreenRegion,
    pub children: Vec<UIElement>,
    pub properties: std::collections::HashMap<String, String>,
}

/// 屏幕上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenContext {
    pub timestamp: DateTime<Utc>,
    pub app_name: String,
    pub title: String,
    pub elements: Vec<UIElement>,
    pub focused_element_id: Option<String>,
    pub screen_size: (u32, u32),
}

/// 屏幕动作目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScreenTarget {
    Window { app: String, title: Option<String> },
    Element { selector: String },
}

/// 屏幕动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScreenAction {
    Observe {
        target: ScreenTarget,
    },
    Click {
        target: ScreenTarget,
    },
    Extract {
        target: ScreenTarget,
        schema: String,
    },
}

impl ScreenAction {
    pub fn required_permission(&self) -> crate::safety::permissions::Permission {
        match self {
            ScreenAction::Observe { .. } | ScreenAction::Extract { .. } => {
                crate::safety::permissions::Permission::ObserveScreen
            }
            ScreenAction::Click { .. } => crate::safety::permissions::Permission::ControlScreen,
        }
    }
}

/// 屏幕动作节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenActionNode {
    pub id: uuid::Uuid,
    pub action: ScreenAction,
}

/// 屏幕动作边
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenActionEdge {
    pub from: uuid::Uuid,
    pub to: uuid::Uuid,
}

/// 屏幕动作图（DAG）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScreenActionGraph {
    nodes: Vec<ScreenActionNode>,
    edges: Vec<ScreenActionEdge>,
}

impl ScreenActionGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: ScreenActionNode) -> anyhow::Result<()> {
        if self.nodes.iter().any(|n| n.id == node.id) {
            anyhow::bail!("duplicate node id: {}", node.id);
        }
        self.nodes.push(node);
        Ok(())
    }

    pub fn add_edge(&mut self, edge: ScreenActionEdge) -> anyhow::Result<()> {
        if !self.nodes.iter().any(|n| n.id == edge.from) {
            anyhow::bail!("unknown source node: {}", edge.from);
        }
        if !self.nodes.iter().any(|n| n.id == edge.to) {
            anyhow::bail!("unknown target node: {}", edge.to);
        }
        self.edges.push(edge);
        Ok(())
    }

    pub fn nodes(&self) -> &[ScreenActionNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[ScreenActionEdge] {
        &self.edges
    }

    pub fn validate(
        &self,
        pm: &crate::safety::permissions::PermissionManager,
    ) -> anyhow::Result<()> {
        for node in &self.nodes {
            let perm = node.action.required_permission();
            if !pm.check(&perm) {
                anyhow::bail!("missing permission: {:?}", perm);
            }
        }
        // Check for cycles
        let _ = self.topological_order()?;
        Ok(())
    }

    pub fn topological_order(&self) -> anyhow::Result<Vec<&ScreenActionNode>> {
        use std::collections::{HashMap, HashSet, VecDeque};
        let mut in_degree: HashMap<uuid::Uuid, usize> = HashMap::new();
        for node in &self.nodes {
            in_degree.insert(node.id, 0);
        }
        for edge in &self.edges {
            *in_degree.entry(edge.to).or_insert(0) += 1;
        }
        let mut queue: VecDeque<uuid::Uuid> = in_degree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();
        let mut ordered = Vec::new();
        let mut visited = HashSet::new();
        while let Some(id) = queue.pop_front() {
            visited.insert(id);
            ordered.push(id);
            for edge in &self.edges {
                if edge.from == id {
                    if let Some(deg) = in_degree.get_mut(&edge.to) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(edge.to);
                        }
                    }
                }
            }
        }
        if visited.len() != self.nodes.len() {
            anyhow::bail!("cycle detected in screen action graph");
        }
        Ok(ordered
            .iter()
            .filter_map(|id| self.nodes.iter().find(|n| n.id == *id))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_region_contains() {
        let r = ScreenRegion {
            x: 10,
            y: 10,
            width: 100,
            height: 50,
        };
        assert!(r.contains(10, 10));
        assert!(r.contains(50, 30));
        assert!(!r.contains(0, 0));
        assert!(!r.contains(110, 60));
    }

    #[test]
    fn ui_element_hierarchy() {
        let child = UIElement {
            id: "btn-1".into(),
            element_type: ElementType::Button,
            label: "OK".into(),
            bounds: ScreenRegion {
                x: 0,
                y: 0,
                width: 80,
                height: 30,
            },
            children: vec![],
            properties: std::collections::HashMap::new(),
        };
        let parent = UIElement {
            id: "container-1".into(),
            element_type: ElementType::Container,
            label: "Dialog".into(),
            bounds: ScreenRegion {
                x: 0,
                y: 0,
                width: 400,
                height: 300,
            },
            children: vec![child],
            properties: std::collections::HashMap::new(),
        };
        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0].id, "btn-1");
    }

    #[test]
    fn screen_context_serialize() {
        let ctx = ScreenContext {
            timestamp: Utc::now(),
            app_name: "TestApp".into(),
            title: "Main Window".into(),
            elements: vec![],
            focused_element_id: None,
            screen_size: (1920, 1080),
        };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("TestApp"));
        let deserialized: ScreenContext = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.app_name, "TestApp");
    }

    #[test]
    fn element_type_variants() {
        let types = vec![
            ElementType::Button,
            ElementType::TextField,
            ElementType::Label,
            ElementType::Image,
            ElementType::Container,
            ElementType::List,
            ElementType::Toggle,
            ElementType::Slider,
            ElementType::Link,
            ElementType::Unknown,
        ];
        assert_eq!(types.len(), 10);
    }
}
