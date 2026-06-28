//! 能力矩阵 — 定义 Agent 的核心能力

use serde::{Deserialize, Serialize};

/// 能力类别
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompetencyCategory {
    Technical,
    Analytical,
    Communication,
    Leadership,
    Domain,
}

/// 单项能力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Competency {
    pub name: String,
    pub category: CompetencyCategory,
    pub description: String,
}

impl Competency {
    pub fn new(
        name: impl Into<String>,
        category: CompetencyCategory,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            category,
            description: description.into(),
        }
    }

    pub fn technical(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(name, CompetencyCategory::Technical, description)
    }

    pub fn analytical(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(name, CompetencyCategory::Analytical, description)
    }

    pub fn communication(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(name, CompetencyCategory::Communication, description)
    }

    pub fn leadership(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(name, CompetencyCategory::Leadership, description)
    }

    pub fn domain(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(name, CompetencyCategory::Domain, description)
    }
}
