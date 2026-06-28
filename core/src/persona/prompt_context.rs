use crate::memory::profile::UserProfile;
use crate::persona::Persona;
use crate::privacy::consent::{ConsentManager, ConsentPurpose};

pub struct PromptContextBuilder<'a> {
    user_id: &'a str,
    consent_manager: Option<&'a ConsentManager>,
    profile: Option<&'a UserProfile>,
    persona: Option<&'a Persona>,
}

impl<'a> PromptContextBuilder<'a> {
    pub fn new(user_id: &'a str) -> Self {
        Self {
            user_id,
            consent_manager: None,
            profile: None,
            persona: None,
        }
    }

    pub fn with_consent_manager(mut self, consent_manager: &'a ConsentManager) -> Self {
        self.consent_manager = Some(consent_manager);
        self
    }

    pub fn with_profile(mut self, profile: &'a UserProfile) -> Self {
        self.profile = Some(profile);
        self
    }

    pub fn with_persona(mut self, persona: &'a Persona) -> Self {
        self.persona = Some(persona);
        self
    }

    pub fn build(&self) -> anyhow::Result<String> {
        let mut sections = vec![format!("User ID: {}", self.user_id)];
        let personalization_enabled = self
            .consent_manager
            .map(|manager| manager.check_consent(self.user_id, ConsentPurpose::Personalization))
            .unwrap_or(false);

        if !personalization_enabled {
            sections.push("Personalization disabled by consent settings.".to_string());
            return Ok(sections.join("\n"));
        }

        sections.push("Personalization enabled.".to_string());

        if let Some(persona) = self.persona {
            sections.push(format!("Persona Name: {}", persona.name));
            sections.push(format!("Persona System Prompt: {}", persona.system_prompt));
            sections.push(format!("Persona Tone: {}", persona.tone));
            if !persona.expertise.is_empty() {
                sections.push(format!(
                    "Persona Expertise: {}",
                    sorted_values(&persona.expertise).join(", ")
                ));
            }
        }

        if let Some(profile) = self.profile {
            if !profile.display_name.is_empty() {
                sections.push(format!("User Display Name: {}", profile.display_name));
            }
            sections.push(format!("Preferred Language: {}", profile.language));

            if !profile.preferences.is_empty() {
                let mut preferences = profile
                    .preferences
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>();
                preferences.sort();
                sections.push(format!("User Preferences: {}", preferences.join(", ")));
            }

            if !profile.interests.is_empty() {
                sections.push(format!(
                    "User Interests: {}",
                    sorted_values(&profile.interests).join(", ")
                ));
            }
        }

        Ok(sections.join("\n"))
    }
}

fn sorted_values(values: &[String]) -> Vec<String> {
    let mut sorted = values.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::PersonaEngine;
    use crate::privacy::consent::ConsentPurpose;
    use uuid::Uuid;

    fn test_consent_manager() -> ConsentManager {
        ConsentManager::new(crate::test_support::temp_db_path("prompt-consent")).unwrap()
    }

    fn test_profile() -> UserProfile {
        let mut profile = UserProfile::new("user-1");
        profile.display_name = "Alex".to_string();
        profile.language = "en-US".to_string();
        profile
            .preferences
            .insert("response_style".to_string(), "concise".to_string());
        profile.interests = vec!["robotics".to_string(), "home automation".to_string()];
        profile
    }

    fn test_persona() -> Persona {
        Persona {
            id: Uuid::new_v4(),
            name: "automation-expert".to_string(),
            system_prompt: "You are a precise home automation assistant.".to_string(),
            tone: "direct".to_string(),
            expertise: vec!["automation".to_string(), "iot".to_string()],
        }
    }

    #[test]
    fn builds_prompt_context_with_profile_and_persona_when_personalization_consent_exists() {
        let consent_manager = test_consent_manager();
        consent_manager
            .record_consent("user-1", ConsentPurpose::Personalization, true)
            .unwrap();
        let profile = test_profile();
        let persona = test_persona();

        let prompt = PromptContextBuilder::new("user-1")
            .with_consent_manager(&consent_manager)
            .with_profile(&profile)
            .with_persona(&persona)
            .build()
            .unwrap();

        assert!(prompt.contains("Alex"));
        assert!(prompt.contains("en-US"));
        assert!(prompt.contains("concise"));
        assert!(prompt.contains("robotics"));
        assert!(prompt.contains("home automation"));
        assert!(prompt.contains(&persona.system_prompt));
        assert!(prompt.contains(&persona.tone));
    }

    #[test]
    fn omits_profile_and_persona_without_personalization_consent() {
        let consent_manager = test_consent_manager();
        let profile = test_profile();
        let persona = test_persona();

        let prompt = PromptContextBuilder::new("user-1")
            .with_consent_manager(&consent_manager)
            .with_profile(&profile)
            .with_persona(&persona)
            .build()
            .unwrap();

        assert!(!prompt.contains("Alex"));
        assert!(!prompt.contains(&persona.system_prompt));
        assert!(prompt.contains("Personalization disabled"));
    }

    #[test]
    fn includes_persona_only_when_profile_is_absent_but_personalization_consent_exists() {
        let consent_manager = test_consent_manager();
        consent_manager
            .record_consent("user-1", ConsentPurpose::Personalization, true)
            .unwrap();
        let persona = test_persona();

        let prompt = PromptContextBuilder::new("user-1")
            .with_consent_manager(&consent_manager)
            .with_persona(&persona)
            .build()
            .unwrap();

        assert!(prompt.contains(&persona.system_prompt));
        assert!(prompt.contains(&persona.tone));
        assert!(!prompt.contains("Alex"));
    }

    #[test]
    fn active_persona_from_engine_can_be_used_as_prompt_context_source() {
        let consent_manager = test_consent_manager();
        consent_manager
            .record_consent("user-1", ConsentPurpose::Personalization, true)
            .unwrap();
        let mut engine = PersonaEngine::new();
        let persona = test_persona();
        let persona_prompt = persona.system_prompt.clone();
        engine.load_persona(persona);

        let prompt = PromptContextBuilder::new("user-1")
            .with_consent_manager(&consent_manager)
            .with_persona(engine.current_persona().unwrap())
            .build()
            .unwrap();

        assert!(prompt.contains(&persona_prompt));
    }
}
