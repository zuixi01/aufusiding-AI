//! 人格管理引擎
//!
//! 管理多个人格实例，支持动态切换和人格状态持久化。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 人格定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    /// 人格 ID
    pub id: Uuid,
    /// 人格名称
    pub name: String,
    /// 系统提示词
    pub system_prompt: String,
    /// 语气风格
    pub tone: String,
    /// 专业领域
    pub expertise: Vec<String>,
}

/// 人格引擎
pub struct PersonaEngine {
    _active_persona: Option<Persona>,
    _personas: Vec<Persona>,
}

impl PersonaEngine {
    pub fn new() -> Self {
        Self {
            _active_persona: None,
            _personas: Vec::new(),
        }
    }

    /// 加载人格
    pub fn load_persona(&mut self, persona: Persona) {
        // 避免重复加载
        if !self._personas.iter().any(|p| p.id == persona.id) {
            self._personas.push(persona);
        }
    }

    /// 切换人格
    pub fn switch_persona(&mut self, persona_id: Uuid) -> anyhow::Result<()> {
        let persona = self
            ._personas
            .iter()
            .find(|p| p.id == persona_id)
            .ok_or_else(|| anyhow::anyhow!("persona not found: {}", persona_id))?
            .clone();
        self._active_persona = Some(persona);
        Ok(())
    }

    /// 获取当前人格
    pub fn current_persona(&self) -> Option<&Persona> {
        self._active_persona.as_ref()
    }

    /// 列出所有人格
    pub fn list_personas(&self) -> &[Persona] {
        &self._personas
    }

    /// 移除人格
    pub fn remove_persona(&mut self, persona_id: Uuid) -> anyhow::Result<()> {
        let before = self._personas.len();
        self._personas.retain(|p| p.id != persona_id);
        if self._personas.len() == before {
            anyhow::bail!("persona not found: {}", persona_id);
        }
        // 如果移除的是当前激活的人格，清除激活状态
        if self
            ._active_persona
            .as_ref()
            .is_some_and(|p| p.id == persona_id)
        {
            self._active_persona = None;
        }
        Ok(())
    }

    /// 创建自定义人格
    pub fn create_custom(
        &mut self,
        name: &str,
        system_prompt: &str,
        tone: &str,
        expertise: Vec<String>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let persona = Persona {
            id,
            name: name.to_string(),
            system_prompt: system_prompt.to_string(),
            tone: tone.to_string(),
            expertise,
        };
        self._personas.push(persona);
        id
    }

    /// 获取当前人格的系统提示词
    pub fn get_system_prompt(&self) -> Option<String> {
        self._active_persona
            .as_ref()
            .map(|p| p.system_prompt.clone())
    }

    /// 根据上下文自动选择最佳人格（关键词匹配 expertise）
    pub fn auto_select(&self, context: &str) -> Option<Uuid> {
        let lower = context.to_lowercase();
        let mut best: Option<(Uuid, usize)> = None;
        for persona in &self._personas {
            let score = persona
                .expertise
                .iter()
                .filter(|e| lower.contains(&e.to_lowercase()))
                .count();
            if score > 0 && best.as_ref().is_none_or(|(_, s)| score > *s) {
                best = Some((persona.id, score));
            }
        }
        best.map(|(id, _)| id)
    }
}

/// 提示词上下文构建器
pub struct PromptContextBuilder<'a> {
    user_id: String,
    consent_manager: Option<&'a crate::privacy::consent::ConsentManager>,
    profile: Option<&'a crate::memory::profile::UserProfile>,
    persona: Option<&'a Persona>,
}

impl<'a> PromptContextBuilder<'a> {
    pub fn new(user_id: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            consent_manager: None,
            profile: None,
            persona: None,
        }
    }

    pub fn with_consent_manager(mut self, cm: &'a crate::privacy::consent::ConsentManager) -> Self {
        self.consent_manager = Some(cm);
        self
    }

    pub fn with_profile(mut self, profile: &'a crate::memory::profile::UserProfile) -> Self {
        self.profile = Some(profile);
        self
    }

    pub fn with_persona(mut self, persona: &'a Persona) -> Self {
        self.persona = Some(persona);
        self
    }

    pub fn build(&self) -> anyhow::Result<String> {
        let mut parts = Vec::new();

        // Check personalization consent
        let personalization_allowed = self
            .consent_manager
            .map(|cm| {
                cm.check_consent(
                    &self.user_id,
                    crate::privacy::consent::ConsentPurpose::Personalization,
                )
            })
            .unwrap_or(false);

        if personalization_allowed {
            if let Some(profile) = self.profile {
                parts.push(format!(
                    "User: {} ({})",
                    profile.display_name, profile.language
                ));
                for (k, v) in &profile.preferences {
                    parts.push(format!("Preference {}: {}", k, v));
                }
            }
            if let Some(persona) = self.persona {
                parts.push(persona.system_prompt.clone());
            }
        } else {
            parts.push("Personalization disabled".to_string());
        }

        Ok(parts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::templates::PersonaTemplate;

    #[test]
    fn load_persona_works() {
        let mut engine = PersonaEngine::new();
        let p = PersonaTemplate::default_assistant();
        engine.load_persona(p);
        assert_eq!(engine.list_personas().len(), 1);
    }

    #[test]
    fn switch_persona_works() {
        let mut engine = PersonaEngine::new();
        let p = PersonaTemplate::tech_expert();
        let id = p.id;
        engine.load_persona(p);
        engine.switch_persona(id).unwrap();
        assert_eq!(engine.current_persona().unwrap().id, id);
    }

    #[test]
    fn switch_nonexistent_fails() {
        let mut engine = PersonaEngine::new();
        let result = engine.switch_persona(Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn remove_persona_works() {
        let mut engine = PersonaEngine::new();
        let p = PersonaTemplate::default_assistant();
        let id = p.id;
        engine.load_persona(p);
        engine.remove_persona(id).unwrap();
        assert!(engine.list_personas().is_empty());
    }

    #[test]
    fn create_custom_persona() {
        let mut engine = PersonaEngine::new();
        let id = engine.create_custom(
            "数学家",
            "你是一位数学专家",
            "严谨",
            vec!["数学".into(), "统计".into()],
        );
        assert_eq!(engine.list_personas().len(), 1);
        engine.switch_persona(id).unwrap();
        assert_eq!(engine.current_persona().unwrap().name, "数学家");
    }

    #[test]
    fn get_system_prompt_works() {
        let mut engine = PersonaEngine::new();
        assert!(engine.get_system_prompt().is_none());
        let p = PersonaTemplate::tech_expert();
        let id = p.id;
        engine.load_persona(p);
        engine.switch_persona(id).unwrap();
        let prompt = engine.get_system_prompt().unwrap();
        assert!(prompt.contains("技术专家"));
    }

    #[test]
    fn auto_select_by_context() {
        let mut engine = PersonaEngine::new();
        let tech = PersonaTemplate::tech_expert();
        let tech_id = tech.id;
        engine.load_persona(tech);
        engine.load_persona(PersonaTemplate::creative_writer());
        let selected = engine.auto_select("我需要帮助编程和架构设计");
        assert_eq!(selected, Some(tech_id));
    }

    #[test]
    fn template_loading() {
        let mut engine = PersonaEngine::new();
        engine.load_persona(PersonaTemplate::default_assistant());
        engine.load_persona(PersonaTemplate::tech_expert());
        engine.load_persona(PersonaTemplate::creative_writer());
        engine.load_persona(PersonaTemplate::life_assistant());
        assert_eq!(engine.list_personas().len(), 4);
    }
}
