//! 儿童模式
//!
//! 内容过滤 + 使用时长限制 + 功能白名单。

use std::collections::HashSet;
use std::time::Duration;
use uuid::Uuid;

use super::content_filter::ContentFilter;
use crate::ambient::biometrics::SpeakerProfile;

/// 儿童模式
pub struct ChildrenMode {
    enabled: bool,
    /// 允许的能力白名单
    allowed_capabilities: HashSet<String>,
    /// 每日使用时长限制
    daily_usage_limit: Duration,
    /// 已使用时长（按用户 ID）
    usage_map: std::collections::HashMap<Uuid, Duration>,
    /// 内容过滤器
    content_filter: ContentFilter,
}

impl ChildrenMode {
    pub fn new(daily_limit: Duration) -> Self {
        let mut allowed = HashSet::new();
        allowed.insert("music".into());
        allowed.insert("weather".into());
        allowed.insert("timer".into());
        allowed.insert("story".into());
        allowed.insert("math".into());

        Self {
            enabled: true,
            allowed_capabilities: allowed,
            daily_usage_limit: daily_limit,
            usage_map: std::collections::HashMap::new(),
            content_filter: ContentFilter::new(),
        }
    }

    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 启用/禁用
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// 添加允许的能力
    pub fn allow_capability(&mut self, cap: &str) {
        self.allowed_capabilities.insert(cap.to_string());
    }

    /// 移除允许的能力
    pub fn disallow_capability(&mut self, cap: &str) {
        self.allowed_capabilities.remove(cap);
    }

    /// 检查请求是否应被阻止
    pub fn should_block_request(&self, text: &str, speaker: &SpeakerProfile) -> bool {
        if !self.enabled || !speaker.is_child {
            return false;
        }

        // 检查内容过滤
        if self.content_filter.contains_blocked(text) {
            return true;
        }

        false
    }

    /// 检查能力是否允许
    pub fn is_capability_allowed(&self, capability: &str, speaker: &SpeakerProfile) -> bool {
        if !self.enabled || !speaker.is_child {
            return true;
        }
        self.allowed_capabilities.contains(capability)
    }

    /// 过滤响应内容
    pub fn filter_response(&self, response: &str) -> String {
        if !self.enabled {
            return response.to_string();
        }
        self.content_filter.filter_text(response)
    }

    /// 检查使用时长限制
    pub fn check_usage_limit(&self, speaker_id: Uuid) -> bool {
        if !self.enabled {
            return true;
        }
        let used = self.usage_map.get(&speaker_id).copied().unwrap_or(Duration::ZERO);
        used < self.daily_usage_limit
    }

    /// 记录使用时长
    pub fn record_usage(&mut self, speaker_id: Uuid, duration: Duration) {
        let entry = self.usage_map.entry(speaker_id).or_insert(Duration::ZERO);
        *entry += duration;
    }

    /// 获取已使用时长
    pub fn usage_for(&self, speaker_id: Uuid) -> Duration {
        self.usage_map.get(&speaker_id).copied().unwrap_or(Duration::ZERO)
    }

    /// 重置每日使用统计
    pub fn reset_daily_usage(&mut self) {
        self.usage_map.clear();
    }

    /// 获取内容过滤器的可变引用
    pub fn content_filter_mut(&mut self) -> &mut ContentFilter {
        &mut self.content_filter
    }
}
