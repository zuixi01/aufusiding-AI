//! 模型目录 — 可扩展的模型注册与查询系统
//!
//! 提供细粒度能力维度、模型层级、厂商标识、定价信息，
//! 支持动态注册新模型，按层级/厂商/模态/能力域筛选。

use super::classifier::IntentType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── 能力维度 ───────────────────────────────────────────

/// 能力维度（比 IntentType 更细粒度）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapabilityDomain {
    /// 日常对话
    TextChat,
    /// 编程（生成、调试、审查）
    Coding,
    /// 数学推导、计算、统计
    Mathematics,
    /// 多步推理、逻辑分析
    Reasoning,
    /// 创意写作、文案
    CreativeWriting,
    /// 跨语言翻译
    Translation,
    /// 知识问答、百科
    Knowledge,
    /// 工具调用、API 编排
    ToolUse,
    /// 图像生成
    ImageGen,
    /// 视频生成
    VideoGen,
}

// ─── 模型层级 ───────────────────────────────────────────

/// 模型层级（难度→层级映射的核心）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ModelTier {
    /// 本地模型（Qwen3B/7B/14B）— 免费、快速
    Local = 0,
    /// 低价云端（QwenTurbo、DeepSeekChat）
    Budget = 1,
    /// 中端（GPT4oMini、QwenMax、Haiku）
    Standard = 2,
    /// 顶尖（Claude Opus、GPT 最新旗舰）
    Premium = 3,
}

// ─── 厂商标识 ───────────────────────────────────────────

/// 厂商标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VendorId {
    /// 本地 Ollama/llama.cpp
    Local,
    /// 阿里云（通义千问）
    Alibaba,
    /// GPT 系列
    OpenAI,
    /// Claude 系列
    Anthropic,
    /// 深度求索
    DeepSeek,
    /// Gemini 系列
    Google,
    /// MiniMax（视频/图像）
    MiniMax,
}

// ─── 认证方式 ───────────────────────────────────────────

/// 认证方式
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AuthMethod {
    /// 本地模型无需认证
    None,
    /// 环境变量中的 API Key
    ApiKey { env_var: String },
    /// OAuth 订阅登录
    OAuth { provider: String },
}

// ─── 模态 ───────────────────────────────────────────────

/// 模型支持的模态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modality {
    Text,
    Image,
    Video,
    Multimodal,
}

// ─── 模型档案 ───────────────────────────────────────────

/// 模型档案（每个模型的完整画像）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    /// 唯一标识，如 "claude-opus", "gpt-4o"
    pub id: String,
    /// 显示名称
    pub display_name: String,
    /// 厂商
    pub vendor: VendorId,
    /// 层级
    pub tier: ModelTier,
    /// 认证方式
    pub auth: AuthMethod,
    /// 是否本地模型
    pub is_local: bool,
    /// 模态
    pub modality: Modality,
    /// 各维度能力评分 [0.0, 1.0]
    pub domain_scores: HashMap<CapabilityDomain, f64>,
    /// 前 3 强项
    pub strengths: Vec<CapabilityDomain>,
    /// 输入价格 (USD / 1M tokens)
    pub input_price_per_m: f64,
    /// 输出价格 (USD / 1M tokens)
    pub output_price_per_m: f64,
    /// 基础延迟 (ms)
    pub base_latency_ms: u64,
    /// 最大上下文长度
    pub context_window: u32,
    /// 用户是否已配置/启用
    pub enabled: bool,
}

// ─── 模型目录 ───────────────────────────────────────────

/// 模型目录（可动态注册）
pub struct ModelCatalog {
    profiles: HashMap<String, ModelProfile>,
}

impl ModelCatalog {
    /// 创建包含默认模型目录的实例
    pub fn new() -> Self {
        let mut catalog = Self {
            profiles: HashMap::new(),
        };
        catalog.register_defaults();
        catalog
    }

    /// 动态注册模型
    pub fn register(&mut self, profile: ModelProfile) {
        self.profiles.insert(profile.id.clone(), profile);
    }

    /// 按 ID 查询
    pub fn get(&self, id: &str) -> Option<&ModelProfile> {
        self.profiles.get(id)
    }

    /// 按层级筛选
    pub fn by_tier(&self, tier: ModelTier) -> Vec<&ModelProfile> {
        self.profiles.values().filter(|p| p.tier == tier).collect()
    }

    /// 按厂商筛选
    pub fn by_vendor(&self, vendor: VendorId) -> Vec<&ModelProfile> {
        self.profiles
            .values()
            .filter(|p| p.vendor == vendor)
            .collect()
    }

    /// 按模态筛选
    pub fn by_modality(&self, modality: Modality) -> Vec<&ModelProfile> {
        self.profiles
            .values()
            .filter(|p| p.modality == modality)
            .collect()
    }

    /// 在指定层级中找某能力域最强的模型
    pub fn best_for_domain(
        &self,
        domain: CapabilityDomain,
        tier: ModelTier,
    ) -> Option<&ModelProfile> {
        self.profiles
            .values()
            .filter(|p| p.tier == tier && p.enabled)
            .max_by(|a, b| {
                let sa = a.domain_scores.get(&domain).copied().unwrap_or(0.0);
                let sb = b.domain_scores.get(&domain).copied().unwrap_or(0.0);
                sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// 返回所有已启用的模型
    pub fn enabled_models(&self) -> Vec<&ModelProfile> {
        self.profiles.values().filter(|p| p.enabled).collect()
    }

    /// IntentType → CapabilityDomain 桥接
    pub fn intent_to_domain(intent: &IntentType) -> CapabilityDomain {
        match intent {
            IntentType::Casual => CapabilityDomain::TextChat,
            IntentType::Knowledge => CapabilityDomain::Knowledge,
            IntentType::ToolUse => CapabilityDomain::ToolUse,
            IntentType::CodeGen => CapabilityDomain::Coding,
            IntentType::Creative => CapabilityDomain::CreativeWriting,
            IntentType::Reasoning => CapabilityDomain::Reasoning,
            IntentType::Unknown => CapabilityDomain::TextChat,
        }
    }

    /// 构建 ModelProfile 的辅助方法
    #[allow(clippy::too_many_arguments)]
    fn make_profile(
        &self,
        id: &str,
        display_name: &str,
        vendor: VendorId,
        tier: ModelTier,
        auth: AuthMethod,
        is_local: bool,
        modality: Modality,
        scores: &[(CapabilityDomain, f64)],
        strengths: &[CapabilityDomain],
        input_price: f64,
        output_price: f64,
        base_latency_ms: u64,
        context_window: u32,
    ) -> ModelProfile {
        ModelProfile {
            id: id.to_string(),
            display_name: display_name.to_string(),
            vendor,
            tier,
            auth,
            is_local,
            modality,
            domain_scores: scores.iter().copied().collect(),
            strengths: strengths.to_vec(),
            input_price_per_m: input_price,
            output_price_per_m: output_price,
            base_latency_ms,
            context_window,
            enabled: true,
        }
    }

    /// 注册内置默认模型
    fn register_defaults(&mut self) {
        use CapabilityDomain::*;
        let defaults = vec![
            self.make_profile(
                "qwen-3b",
                "Qwen 3B",
                VendorId::Local,
                ModelTier::Local,
                AuthMethod::None,
                true,
                Modality::Text,
                &[
                    (TextChat, 0.85),
                    (Coding, 0.55),
                    (Mathematics, 0.35),
                    (Reasoning, 0.35),
                    (CreativeWriting, 0.40),
                    (Translation, 0.50),
                    (Knowledge, 0.60),
                    (ToolUse, 0.30),
                ],
                &[TextChat],
                0.0,
                0.0,
                50,
                4096,
            ),
            self.make_profile(
                "qwen-7b",
                "Qwen 7B",
                VendorId::Local,
                ModelTier::Local,
                AuthMethod::None,
                true,
                Modality::Text,
                &[
                    (TextChat, 0.90),
                    (Coding, 0.75),
                    (Mathematics, 0.55),
                    (Reasoning, 0.60),
                    (CreativeWriting, 0.65),
                    (Translation, 0.70),
                    (Knowledge, 0.75),
                    (ToolUse, 0.55),
                ],
                &[TextChat, Coding],
                0.0,
                0.0,
                100,
                8192,
            ),
            self.make_profile(
                "qwen-14b",
                "Qwen 14B",
                VendorId::Local,
                ModelTier::Local,
                AuthMethod::None,
                true,
                Modality::Text,
                &[
                    (TextChat, 0.92),
                    (Coding, 0.85),
                    (Mathematics, 0.70),
                    (Reasoning, 0.75),
                    (CreativeWriting, 0.78),
                    (Translation, 0.80),
                    (Knowledge, 0.82),
                    (ToolUse, 0.70),
                ],
                &[Coding, Reasoning],
                0.0,
                0.0,
                200,
                16384,
            ),
            self.make_profile(
                "qwen-turbo",
                "Qwen Turbo",
                VendorId::Alibaba,
                ModelTier::Budget,
                AuthMethod::ApiKey {
                    env_var: "DASHSCOPE_API_KEY".into(),
                },
                false,
                Modality::Text,
                &[
                    (TextChat, 0.82),
                    (Coding, 0.70),
                    (Mathematics, 0.60),
                    (Reasoning, 0.55),
                    (CreativeWriting, 0.60),
                    (Translation, 0.72),
                    (Knowledge, 0.70),
                    (ToolUse, 0.45),
                ],
                &[TextChat],
                0.30,
                0.60,
                300,
                8192,
            ),
            self.make_profile(
                "qwen-max",
                "Qwen Max",
                VendorId::Alibaba,
                ModelTier::Standard,
                AuthMethod::ApiKey {
                    env_var: "DASHSCOPE_API_KEY".into(),
                },
                false,
                Modality::Text,
                &[
                    (TextChat, 0.93),
                    (Coding, 0.90),
                    (Mathematics, 0.85),
                    (Reasoning, 0.85),
                    (CreativeWriting, 0.82),
                    (Translation, 0.88),
                    (Knowledge, 0.88),
                    (ToolUse, 0.78),
                ],
                &[Coding, Mathematics],
                2.00,
                6.00,
                500,
                32768,
            ),
            self.make_profile(
                "deepseek-chat",
                "DeepSeek Chat",
                VendorId::DeepSeek,
                ModelTier::Standard,
                AuthMethod::ApiKey {
                    env_var: "DEEPSEEK_API_KEY".into(),
                },
                false,
                Modality::Text,
                &[
                    (TextChat, 0.88),
                    (Coding, 0.90),
                    (Mathematics, 0.92),
                    (Reasoning, 0.88),
                    (CreativeWriting, 0.75),
                    (Translation, 0.80),
                    (Knowledge, 0.85),
                    (ToolUse, 0.72),
                ],
                &[Mathematics, Coding],
                0.14,
                0.28,
                400,
                65536,
            ),
            self.make_profile(
                "gpt-4o-mini",
                "GPT-4o Mini",
                VendorId::OpenAI,
                ModelTier::Standard,
                AuthMethod::ApiKey {
                    env_var: "OPENAI_API_KEY".into(),
                },
                false,
                Modality::Multimodal,
                &[
                    (TextChat, 0.90),
                    (Coding, 0.82),
                    (Mathematics, 0.75),
                    (Reasoning, 0.78),
                    (CreativeWriting, 0.80),
                    (Translation, 0.82),
                    (Knowledge, 0.85),
                    (ToolUse, 0.88),
                ],
                &[ToolUse],
                0.15,
                0.60,
                500,
                128000,
            ),
            self.make_profile(
                "gpt-4o",
                "GPT-4o",
                VendorId::OpenAI,
                ModelTier::Premium,
                AuthMethod::ApiKey {
                    env_var: "OPENAI_API_KEY".into(),
                },
                false,
                Modality::Multimodal,
                &[
                    (TextChat, 0.96),
                    (Coding, 0.92),
                    (Mathematics, 0.88),
                    (Reasoning, 0.95),
                    (CreativeWriting, 0.90),
                    (Translation, 0.92),
                    (Knowledge, 0.93),
                    (ToolUse, 0.95),
                ],
                &[Reasoning, ToolUse],
                2.50,
                10.00,
                800,
                128000,
            ),
            self.make_profile(
                "claude-sonnet",
                "Claude Sonnet",
                VendorId::Anthropic,
                ModelTier::Standard,
                AuthMethod::ApiKey {
                    env_var: "ANTHROPIC_API_KEY".into(),
                },
                false,
                Modality::Text,
                &[
                    (TextChat, 0.95),
                    (Coding, 0.93),
                    (Mathematics, 0.82),
                    (Reasoning, 0.90),
                    (CreativeWriting, 0.95),
                    (Translation, 0.88),
                    (Knowledge, 0.90),
                    (ToolUse, 0.90),
                ],
                &[Coding, CreativeWriting],
                3.00,
                15.00,
                700,
                200000,
            ),
            self.make_profile(
                "claude-opus",
                "Claude Opus",
                VendorId::Anthropic,
                ModelTier::Premium,
                AuthMethod::ApiKey {
                    env_var: "ANTHROPIC_API_KEY".into(),
                },
                false,
                Modality::Text,
                &[
                    (TextChat, 0.97),
                    (Coding, 0.96),
                    (Mathematics, 0.88),
                    (Reasoning, 0.97),
                    (CreativeWriting, 0.96),
                    (Translation, 0.92),
                    (Knowledge, 0.95),
                    (ToolUse, 0.93),
                ],
                &[Coding, Reasoning],
                15.00,
                75.00,
                1000,
                200000,
            ),
            self.make_profile(
                "gemini-flash",
                "Gemini Flash",
                VendorId::Google,
                ModelTier::Standard,
                AuthMethod::ApiKey {
                    env_var: "GOOGLE_API_KEY".into(),
                },
                false,
                Modality::Multimodal,
                &[
                    (TextChat, 0.88),
                    (Coding, 0.80),
                    (Mathematics, 0.78),
                    (Reasoning, 0.82),
                    (CreativeWriting, 0.76),
                    (Translation, 0.80),
                    (Knowledge, 0.85),
                    (ToolUse, 0.84),
                ],
                &[Knowledge, ToolUse],
                0.075,
                0.30,
                300,
                1000000,
            ),
            self.make_profile(
                "gemini-pro",
                "Gemini Pro",
                VendorId::Google,
                ModelTier::Premium,
                AuthMethod::ApiKey {
                    env_var: "GOOGLE_API_KEY".into(),
                },
                false,
                Modality::Multimodal,
                &[
                    (TextChat, 0.94),
                    (Coding, 0.90),
                    (Mathematics, 0.88),
                    (Reasoning, 0.93),
                    (CreativeWriting, 0.85),
                    (Translation, 0.90),
                    (Knowledge, 0.92),
                    (ToolUse, 0.90),
                ],
                &[Reasoning, Knowledge],
                1.25,
                5.00,
                600,
                2000000,
            ),
            self.make_profile(
                "minimax-image",
                "MiniMax Image",
                VendorId::MiniMax,
                ModelTier::Standard,
                AuthMethod::ApiKey {
                    env_var: "MINIMAX_API_KEY".into(),
                },
                false,
                Modality::Image,
                &[(ImageGen, 0.90)],
                &[ImageGen],
                0.02,
                0.02,
                2000,
                0,
            ),
            self.make_profile(
                "minimax-video",
                "MiniMax Video",
                VendorId::MiniMax,
                ModelTier::Standard,
                AuthMethod::ApiKey {
                    env_var: "MINIMAX_API_KEY".into(),
                },
                false,
                Modality::Video,
                &[(VideoGen, 0.88)],
                &[VideoGen],
                0.10,
                0.10,
                10000,
                0,
            ),
        ];
        for p in defaults {
            self.register(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_default_has_all_models() {
        let catalog = ModelCatalog::new();
        let expected = [
            "qwen-3b",
            "qwen-7b",
            "qwen-14b",
            "qwen-turbo",
            "qwen-max",
            "deepseek-chat",
            "gpt-4o-mini",
            "gpt-4o",
            "claude-sonnet",
            "claude-opus",
            "gemini-flash",
            "gemini-pro",
            "minimax-image",
            "minimax-video",
        ];
        for id in &expected {
            assert!(catalog.get(id).is_some(), "缺少模型: {}", id);
        }
        assert_eq!(catalog.enabled_models().len(), expected.len());
    }

    #[test]
    fn catalog_register_custom_model() {
        let mut catalog = ModelCatalog::new();
        let before = catalog.enabled_models().len();
        catalog.register(ModelProfile {
            id: "custom-llm".into(),
            display_name: "Custom LLM".into(),
            vendor: VendorId::Local,
            tier: ModelTier::Budget,
            auth: AuthMethod::None,
            is_local: true,
            modality: Modality::Text,
            domain_scores: [(CapabilityDomain::TextChat, 0.80)].into_iter().collect(),
            strengths: vec![CapabilityDomain::TextChat],
            input_price_per_m: 0.0,
            output_price_per_m: 0.0,
            base_latency_ms: 80,
            context_window: 4096,
            enabled: true,
        });
        assert_eq!(catalog.enabled_models().len(), before + 1);
        assert!(catalog.get("custom-llm").is_some());
    }

    #[test]
    fn catalog_filter_by_tier() {
        let catalog = ModelCatalog::new();
        let locals = catalog.by_tier(ModelTier::Local);
        assert_eq!(locals.len(), 3); // qwen-3b, 7b, 14b
        for p in &locals {
            assert!(p.is_local);
        }
        let premiums = catalog.by_tier(ModelTier::Premium);
        assert!(premiums.len() >= 3); // gpt-4o, claude-opus, gemini-pro
    }

    #[test]
    fn catalog_filter_by_vendor() {
        let catalog = ModelCatalog::new();
        let alibaba = catalog.by_vendor(VendorId::Alibaba);
        assert_eq!(alibaba.len(), 2); // qwen-turbo, qwen-max
        let google = catalog.by_vendor(VendorId::Google);
        assert_eq!(google.len(), 2); // gemini-flash, gemini-pro
    }

    #[test]
    fn catalog_best_for_coding_premium() {
        let catalog = ModelCatalog::new();
        let best = catalog
            .best_for_domain(CapabilityDomain::Coding, ModelTier::Premium)
            .unwrap();
        // claude-opus 的 Coding=0.96 是 Premium 层最高
        assert_eq!(best.id, "claude-opus");
    }

    #[test]
    fn catalog_best_for_math_standard() {
        let catalog = ModelCatalog::new();
        let best = catalog
            .best_for_domain(CapabilityDomain::Mathematics, ModelTier::Standard)
            .unwrap();
        // deepseek-chat 的 Mathematics=0.92 是 Standard 层最高
        assert_eq!(best.id, "deepseek-chat");
    }

    #[test]
    fn catalog_intent_to_domain_mapping() {
        assert_eq!(
            ModelCatalog::intent_to_domain(&IntentType::Casual),
            CapabilityDomain::TextChat
        );
        assert_eq!(
            ModelCatalog::intent_to_domain(&IntentType::CodeGen),
            CapabilityDomain::Coding
        );
        assert_eq!(
            ModelCatalog::intent_to_domain(&IntentType::Reasoning),
            CapabilityDomain::Reasoning
        );
        assert_eq!(
            ModelCatalog::intent_to_domain(&IntentType::Knowledge),
            CapabilityDomain::Knowledge
        );
        assert_eq!(
            ModelCatalog::intent_to_domain(&IntentType::Creative),
            CapabilityDomain::CreativeWriting
        );
        assert_eq!(
            ModelCatalog::intent_to_domain(&IntentType::ToolUse),
            CapabilityDomain::ToolUse
        );
    }

    #[test]
    fn catalog_filter_by_modality() {
        let catalog = ModelCatalog::new();
        let images = catalog.by_modality(Modality::Image);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].id, "minimax-image");
        let videos = catalog.by_modality(Modality::Video);
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].id, "minimax-video");
    }
}
