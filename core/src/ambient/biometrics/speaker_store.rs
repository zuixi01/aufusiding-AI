//! 声纹库持久化
//!
//! 基于内存存储的声纹库（生产环境可替换为 SQLite）。

use uuid::Uuid;

use super::speaker_profile::SpeakerProfile;
use super::voiceprint::VoiceprintEngine;

/// 声纹库
pub struct SpeakerStore {
    profiles: Vec<SpeakerProfile>,
    engine: VoiceprintEngine,
}

impl SpeakerStore {
    pub fn new(engine: VoiceprintEngine) -> Self {
        Self {
            profiles: Vec::new(),
            engine,
        }
    }

    /// 注册说话人
    pub fn register(&mut self, profile: SpeakerProfile) -> anyhow::Result<Uuid> {
        if profile.name.trim().is_empty() {
            anyhow::bail!("说话人名称不能为空");
        }
        if self.profiles.iter().any(|p| p.id == profile.id) {
            anyhow::bail!("说话人已存在: {}", profile.id);
        }
        let id = profile.id;
        self.profiles.push(profile);
        Ok(id)
    }

    /// 通过声纹识别说话人
    pub fn identify(&self, voiceprint: &[f32]) -> Option<&SpeakerProfile> {
        let mut best_match: Option<(&SpeakerProfile, f32)> = None;

        for profile in &self.profiles {
            if profile.voiceprint.is_empty() {
                continue;
            }
            let similarity = self.engine.compare(voiceprint, &profile.voiceprint);
            if similarity >= self.engine.match_threshold() {
                match best_match {
                    Some((_, best_sim)) if similarity > best_sim => {
                        best_match = Some((profile, similarity));
                    }
                    None => {
                        best_match = Some((profile, similarity));
                    }
                    _ => {}
                }
            }
        }

        best_match.map(|(p, _)| p)
    }

    /// 通过 ID 查找
    pub fn find_by_id(&self, id: Uuid) -> Option<&SpeakerProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// 通过名称查找
    pub fn find_by_name(&self, name: &str) -> Option<&SpeakerProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// 列出所有说话人
    pub fn list(&self) -> &[SpeakerProfile] {
        &self.profiles
    }

    /// 删除说话人
    pub fn remove(&mut self, id: Uuid) -> anyhow::Result<()> {
        let idx = self.profiles.iter().position(|p| p.id == id)
            .ok_or_else(|| anyhow::anyhow!("说话人不存在: {}", id))?;
        self.profiles.remove(idx);
        Ok(())
    }

    /// 更新声纹
    pub fn update_voiceprint(&mut self, id: Uuid, voiceprint: Vec<f32>) -> anyhow::Result<()> {
        let profile = self.profiles.iter_mut().find(|p| p.id == id)
            .ok_or_else(|| anyhow::anyhow!("说话人不存在: {}", id))?;
        profile.voiceprint = voiceprint;
        Ok(())
    }

    /// 已注册说话人数量
    pub fn count(&self) -> usize {
        self.profiles.len()
    }
}
