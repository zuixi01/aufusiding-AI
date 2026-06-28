//! 人格模板
//!
//! 预设的人格模板，用户可以基于模板创建自定义人格。

use super::engine::Persona;
use uuid::Uuid;

/// 人格模板
pub struct PersonaTemplate;

impl PersonaTemplate {
    /// 默认助手人格
    pub fn default_assistant() -> Persona {
        Persona {
            id: Uuid::new_v4(),
            name: "灵犀".to_string(),
            system_prompt: "你是灵犀，一个友好、专业的 AI 助手。".to_string(),
            tone: "友好、专业".to_string(),
            expertise: vec!["通用知识".to_string()],
        }
    }

    /// 技术专家人格
    pub fn tech_expert() -> Persona {
        Persona {
            id: Uuid::new_v4(),
            name: "技术顾问".to_string(),
            system_prompt: "你是一位资深技术专家，擅长编程、架构设计和技术咨询。".to_string(),
            tone: "专业、严谨".to_string(),
            expertise: vec!["编程".to_string(), "架构".to_string(), "DevOps".to_string()],
        }
    }

    /// 创意写作人格
    pub fn creative_writer() -> Persona {
        Persona {
            id: Uuid::new_v4(),
            name: "创意作家".to_string(),
            system_prompt: "你是一位富有想象力的作家，擅长创意写作和故事创作。".to_string(),
            tone: "生动、富有想象力".to_string(),
            expertise: vec!["写作".to_string(), "故事创作".to_string()],
        }
    }

    /// 生活助手人格
    pub fn life_assistant() -> Persona {
        Persona {
            id: Uuid::new_v4(),
            name: "生活管家".to_string(),
            system_prompt: "你是一位贴心的生活助手，帮助用户管理日常事务。".to_string(),
            tone: "温暖、贴心".to_string(),
            expertise: vec!["日程管理".to_string(), "生活建议".to_string()],
        }
    }
}
