//! 模型能力矩阵
//!
//! 定义不同模型对不同意图的处理能力，用于质量预测和路由决策。
//! 支持动态加载、运行时更新和持久化。

use super::classifier::IntentType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 模型标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelId {
    /// Qwen2.5-3B 本地模型
    Qwen3B,
    /// Qwen2.5-7B 本地模型
    Qwen7B,
    /// Qwen2.5-14B 本地模型
    Qwen14B,
    /// Qwen Turbo（阿里云低价）
    QwenTurbo,
    /// Qwen Max（阿里云中端）
    QwenMax,
    /// DeepSeek Chat
    DeepSeekChat,
    /// GPT-4o-mini
    GPT4oMini,
    /// GPT-4o
    GPT4o,
    /// Claude-3.5-Sonnet
    Claude35Sonnet,
    /// Claude Opus
    ClaudeOpus,
    /// Gemini Flash
    GeminiFlash,
    /// Gemini Pro
    GeminiPro,
}

impl ModelId {
    /// 判断是否为本地模型
    pub fn is_local(&self) -> bool {
        matches!(self, ModelId::Qwen3B | ModelId::Qwen7B | ModelId::Qwen14B)
    }

    /// 桥接到 ModelCatalog 的 ID
    pub fn to_catalog_id(&self) -> &str {
        match self {
            ModelId::Qwen3B => "qwen-3b",
            ModelId::Qwen7B => "qwen-7b",
            ModelId::Qwen14B => "qwen-14b",
            ModelId::QwenTurbo => "qwen-turbo",
            ModelId::QwenMax => "qwen-max",
            ModelId::DeepSeekChat => "deepseek-chat",
            ModelId::GPT4oMini => "gpt-4o-mini",
            ModelId::GPT4o => "gpt-4o",
            ModelId::Claude35Sonnet => "claude-sonnet",
            ModelId::ClaudeOpus => "claude-opus",
            ModelId::GeminiFlash => "gemini-flash",
            ModelId::GeminiPro => "gemini-pro",
        }
    }
}

/// 模型能力矩阵
pub struct CapabilityMatrix {
    /// 能力评分表：(模型, 意图) -> 能力得分 [0.0, 1.0]
    capabilities: HashMap<(ModelId, IntentType), f64>,
    /// 历史性能追踪器
    performance_tracker: PerformanceTracker,
    /// 配置文件路径（用于持久化）
    config_path: Option<PathBuf>,
}

/// 可序列化的能力矩阵配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityConfig {
    /// 能力评分列表
    pub capabilities: Vec<CapabilityEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityEntry {
    pub model: ModelId,
    pub intent: IntentType,
    pub score: f64,
}

impl CapabilityMatrix {
    /// 创建新的能力矩阵
    pub fn new() -> Self {
        let mut capabilities = HashMap::new();

        // 定义能力矩阵：6 种意图 × 6 个模型
        // 格式：(模型, 意图, 能力得分)
        let matrix_data = vec![
            // Qwen2.5-3B
            (ModelId::Qwen3B, IntentType::Casual, 0.85),
            (ModelId::Qwen3B, IntentType::Knowledge, 0.70),
            (ModelId::Qwen3B, IntentType::CodeGen, 0.60),
            (ModelId::Qwen3B, IntentType::Creative, 0.50),
            (ModelId::Qwen3B, IntentType::Reasoning, 0.40),
            (ModelId::Qwen3B, IntentType::ToolUse, 0.65),
            // Qwen2.5-7B
            (ModelId::Qwen7B, IntentType::Casual, 0.90),
            (ModelId::Qwen7B, IntentType::Knowledge, 0.85),
            (ModelId::Qwen7B, IntentType::CodeGen, 0.80),
            (ModelId::Qwen7B, IntentType::Creative, 0.75),
            (ModelId::Qwen7B, IntentType::Reasoning, 0.65),
            (ModelId::Qwen7B, IntentType::ToolUse, 0.80),
            // Qwen2.5-14B
            (ModelId::Qwen14B, IntentType::Casual, 0.95),
            (ModelId::Qwen14B, IntentType::Knowledge, 0.90),
            (ModelId::Qwen14B, IntentType::CodeGen, 0.90),
            (ModelId::Qwen14B, IntentType::Creative, 0.85),
            (ModelId::Qwen14B, IntentType::Reasoning, 0.80),
            (ModelId::Qwen14B, IntentType::ToolUse, 0.85),
            // GPT-4o-mini
            (ModelId::GPT4oMini, IntentType::Casual, 0.92),
            (ModelId::GPT4oMini, IntentType::Knowledge, 0.88),
            (ModelId::GPT4oMini, IntentType::CodeGen, 0.85),
            (ModelId::GPT4oMini, IntentType::Creative, 0.82),
            (ModelId::GPT4oMini, IntentType::Reasoning, 0.78),
            (ModelId::GPT4oMini, IntentType::ToolUse, 0.85),
            // GPT-4o
            (ModelId::GPT4o, IntentType::Casual, 0.98),
            (ModelId::GPT4o, IntentType::Knowledge, 0.95),
            (ModelId::GPT4o, IntentType::CodeGen, 0.95),
            (ModelId::GPT4o, IntentType::Creative, 0.95),
            (ModelId::GPT4o, IntentType::Reasoning, 0.95),
            (ModelId::GPT4o, IntentType::ToolUse, 0.95),
            // Claude-3.5-Sonnet
            (ModelId::Claude35Sonnet, IntentType::Casual, 0.97),
            (ModelId::Claude35Sonnet, IntentType::Knowledge, 0.96),
            (ModelId::Claude35Sonnet, IntentType::CodeGen, 0.96),
            (ModelId::Claude35Sonnet, IntentType::Creative, 0.98),
            (ModelId::Claude35Sonnet, IntentType::Reasoning, 0.97),
            (ModelId::Claude35Sonnet, IntentType::ToolUse, 0.96),
            // Qwen Turbo
            (ModelId::QwenTurbo, IntentType::Casual, 0.82),
            (ModelId::QwenTurbo, IntentType::Knowledge, 0.70),
            (ModelId::QwenTurbo, IntentType::CodeGen, 0.70),
            (ModelId::QwenTurbo, IntentType::Creative, 0.60),
            (ModelId::QwenTurbo, IntentType::Reasoning, 0.55),
            (ModelId::QwenTurbo, IntentType::ToolUse, 0.45),
            // Qwen Max
            (ModelId::QwenMax, IntentType::Casual, 0.93),
            (ModelId::QwenMax, IntentType::Knowledge, 0.88),
            (ModelId::QwenMax, IntentType::CodeGen, 0.90),
            (ModelId::QwenMax, IntentType::Creative, 0.82),
            (ModelId::QwenMax, IntentType::Reasoning, 0.85),
            (ModelId::QwenMax, IntentType::ToolUse, 0.78),
            // DeepSeek Chat
            (ModelId::DeepSeekChat, IntentType::Casual, 0.88),
            (ModelId::DeepSeekChat, IntentType::Knowledge, 0.85),
            (ModelId::DeepSeekChat, IntentType::CodeGen, 0.90),
            (ModelId::DeepSeekChat, IntentType::Creative, 0.75),
            (ModelId::DeepSeekChat, IntentType::Reasoning, 0.88),
            (ModelId::DeepSeekChat, IntentType::ToolUse, 0.72),
            // Claude Opus
            (ModelId::ClaudeOpus, IntentType::Casual, 0.97),
            (ModelId::ClaudeOpus, IntentType::Knowledge, 0.95),
            (ModelId::ClaudeOpus, IntentType::CodeGen, 0.96),
            (ModelId::ClaudeOpus, IntentType::Creative, 0.96),
            (ModelId::ClaudeOpus, IntentType::Reasoning, 0.97),
            (ModelId::ClaudeOpus, IntentType::ToolUse, 0.93),
            // Gemini Flash
            (ModelId::GeminiFlash, IntentType::Casual, 0.88),
            (ModelId::GeminiFlash, IntentType::Knowledge, 0.85),
            (ModelId::GeminiFlash, IntentType::CodeGen, 0.80),
            (ModelId::GeminiFlash, IntentType::Creative, 0.76),
            (ModelId::GeminiFlash, IntentType::Reasoning, 0.82),
            (ModelId::GeminiFlash, IntentType::ToolUse, 0.84),
            // Gemini Pro
            (ModelId::GeminiPro, IntentType::Casual, 0.94),
            (ModelId::GeminiPro, IntentType::Knowledge, 0.92),
            (ModelId::GeminiPro, IntentType::CodeGen, 0.90),
            (ModelId::GeminiPro, IntentType::Creative, 0.85),
            (ModelId::GeminiPro, IntentType::Reasoning, 0.93),
            (ModelId::GeminiPro, IntentType::ToolUse, 0.90),
        ];

        for (model, intent, score) in matrix_data {
            capabilities.insert((model, intent), score);
        }

        Self {
            capabilities,
            performance_tracker: PerformanceTracker::new(),
            config_path: None,
        }
    }

    /// 从配置文件加载能力矩阵
    pub fn from_config_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;
        let config: CapabilityConfig = serde_json::from_str(&content)?;

        let mut capabilities = HashMap::new();
        for entry in config.capabilities {
            capabilities.insert((entry.model, entry.intent), entry.score);
        }

        Ok(Self {
            capabilities,
            performance_tracker: PerformanceTracker::new(),
            config_path: Some(path.to_path_buf()),
        })
    }

    /// 保存能力矩阵到配置文件
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let entries: Vec<CapabilityEntry> = self
            .capabilities
            .iter()
            .map(|((model, intent), score)| CapabilityEntry {
                model: *model,
                intent: intent.clone(),
                score: *score,
            })
            .collect();

        let config = CapabilityConfig {
            capabilities: entries,
        };

        let json = serde_json::to_string_pretty(&config)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// 保存到默认配置路径（如果设置了）
    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(path) = &self.config_path {
            self.save_to_file(path)
        } else {
            anyhow::bail!("No config path set")
        }
    }

    /// 运行时更新能力得分
    pub fn update_capability(&mut self, model: ModelId, intent: IntentType, score: f64) {
        let score = score.clamp(0.0, 1.0);
        self.capabilities.insert((model, intent), score);
    }

    /// 批量更新能力得分
    pub fn update_capabilities(&mut self, updates: Vec<(ModelId, IntentType, f64)>) {
        for (model, intent, score) in updates {
            self.update_capability(model, intent, score);
        }
    }

    /// 获取模型对特定意图的能力得分
    pub fn get_capability(&self, model: ModelId, intent: &IntentType) -> f64 {
        self.capabilities
            .get(&(model, intent.clone()))
            .copied()
            .unwrap_or(0.5) // 默认中等能力
    }

    /// 预测模型质量：能力 × 0.7 + 历史性能 × 0.3
    pub fn predict_quality(&self, model: ModelId, intent: &IntentType) -> f64 {
        let capability = self.get_capability(model, intent);
        let historical = self.performance_tracker.get_ema(model, intent);

        0.7 * capability + 0.3 * historical
    }

    /// 记录模型执行结果，更新历史性能
    pub fn record_performance(&mut self, model: ModelId, intent: &IntentType, success: bool) {
        self.performance_tracker.record(model, intent, success);
    }

    /// 获取所有支持的模型
    pub fn all_models(&self) -> Vec<ModelId> {
        vec![
            ModelId::Qwen3B,
            ModelId::Qwen7B,
            ModelId::Qwen14B,
            ModelId::QwenTurbo,
            ModelId::QwenMax,
            ModelId::DeepSeekChat,
            ModelId::GPT4oMini,
            ModelId::GPT4o,
            ModelId::Claude35Sonnet,
            ModelId::ClaudeOpus,
            ModelId::GeminiFlash,
            ModelId::GeminiPro,
        ]
    }
}

/// 历史性能追踪器（使用 EMA - 指数移动平均）
struct PerformanceTracker {
    /// EMA 值：(模型, 意图) -> EMA 得分
    ema_scores: HashMap<(ModelId, IntentType), f64>,
    /// 平滑系数
    alpha: f64,
}

impl PerformanceTracker {
    fn new() -> Self {
        Self {
            ema_scores: HashMap::new(),
            alpha: 0.2, // 平滑系数
        }
    }

    /// 获取 EMA 得分
    fn get_ema(&self, model: ModelId, intent: &IntentType) -> f64 {
        self.ema_scores
            .get(&(model, intent.clone()))
            .copied()
            .unwrap_or(0.8) // 默认假设 80% 成功率
    }

    /// 记录新的性能数据，更新 EMA
    /// EMA_t = α · success_t + (1-α) · EMA_{t-1}
    fn record(&mut self, model: ModelId, intent: &IntentType, success: bool) {
        let current_ema = self.get_ema(model, intent);
        let success_value = if success { 1.0 } else { 0.0 };

        let new_ema = self.alpha * success_value + (1.0 - self.alpha) * current_ema;

        self.ema_scores.insert((model, intent.clone()), new_ema);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_matrix_creation() {
        let matrix = CapabilityMatrix::new();

        // 测试能力矩阵是否正确初始化
        assert_eq!(
            matrix.get_capability(ModelId::Qwen3B, &IntentType::Casual),
            0.85
        );
        assert_eq!(
            matrix.get_capability(ModelId::GPT4o, &IntentType::Reasoning),
            0.95
        );
        assert_eq!(
            matrix.get_capability(ModelId::Claude35Sonnet, &IntentType::Creative),
            0.98
        );
    }

    #[test]
    fn test_model_is_local() {
        assert!(ModelId::Qwen3B.is_local());
        assert!(ModelId::Qwen7B.is_local());
        assert!(ModelId::Qwen14B.is_local());
        assert!(!ModelId::GPT4o.is_local());
        assert!(!ModelId::Claude35Sonnet.is_local());
    }

    #[test]
    fn test_predict_quality_without_history() {
        let matrix = CapabilityMatrix::new();

        // 没有历史数据时：0.7 * capability + 0.3 * 0.8
        let quality = matrix.predict_quality(ModelId::Qwen7B, &IntentType::CodeGen);
        let expected = 0.7 * 0.80 + 0.3 * 0.8;
        assert!((quality - expected).abs() < 0.01);
    }

    #[test]
    fn test_predict_quality_with_history() {
        let mut matrix = CapabilityMatrix::new();

        // 记录一些成功的执行
        matrix.record_performance(ModelId::Qwen7B, &IntentType::CodeGen, true);
        matrix.record_performance(ModelId::Qwen7B, &IntentType::CodeGen, true);
        matrix.record_performance(ModelId::Qwen7B, &IntentType::CodeGen, true);

        let quality = matrix.predict_quality(ModelId::Qwen7B, &IntentType::CodeGen);

        // 质量应该提升（因为历史性能好）
        let baseline = 0.7 * 0.80 + 0.3 * 0.8;
        assert!(quality > baseline);
    }

    #[test]
    fn test_ema_update() {
        let mut matrix = CapabilityMatrix::new();

        // 初始 EMA = 0.8
        let initial_ema = matrix
            .performance_tracker
            .get_ema(ModelId::Qwen3B, &IntentType::Casual);
        assert_eq!(initial_ema, 0.8);

        // 记录一次成功
        matrix.record_performance(ModelId::Qwen3B, &IntentType::Casual, true);
        let ema_after_success = matrix
            .performance_tracker
            .get_ema(ModelId::Qwen3B, &IntentType::Casual);

        // EMA 应该上升
        // EMA_new = 0.2 * 1.0 + 0.8 * 0.8 = 0.84
        assert!((ema_after_success - 0.84).abs() < 0.01);

        // 记录一次失败
        matrix.record_performance(ModelId::Qwen3B, &IntentType::Casual, false);
        let ema_after_failure = matrix
            .performance_tracker
            .get_ema(ModelId::Qwen3B, &IntentType::Casual);

        // EMA 应该下降
        // EMA_new = 0.2 * 0.0 + 0.8 * 0.84 = 0.672
        assert!((ema_after_failure - 0.672).abs() < 0.01);
    }

    #[test]
    fn test_all_models() {
        let matrix = CapabilityMatrix::new();
        let models = matrix.all_models();

        assert_eq!(models.len(), 12);
        assert!(models.contains(&ModelId::Qwen3B));
        assert!(models.contains(&ModelId::GPT4o));
        assert!(models.contains(&ModelId::Claude35Sonnet));
    }

    #[test]
    fn test_capability_scores_range() {
        let matrix = CapabilityMatrix::new();

        // 测试所有能力得分都在 [0, 1] 范围内
        for model in matrix.all_models() {
            for intent in &[
                IntentType::Casual,
                IntentType::Knowledge,
                IntentType::CodeGen,
                IntentType::Creative,
                IntentType::Reasoning,
                IntentType::ToolUse,
            ] {
                let score = matrix.get_capability(model, intent);
                assert!(
                    score >= 0.0 && score <= 1.0,
                    "Score for {:?} on {:?} is out of range: {}",
                    model,
                    intent,
                    score
                );
            }
        }
    }

    #[test]
    fn test_quality_prediction_bounds() {
        let matrix = CapabilityMatrix::new();

        // 质量预测应该在 [0, 1] 范围内
        for model in matrix.all_models() {
            let quality = matrix.predict_quality(model, &IntentType::Reasoning);
            assert!(quality >= 0.0 && quality <= 1.0);
        }
    }

    #[test]
    fn test_save_and_load_config() {
        use std::fs;
        let temp_path = crate::test_support::temp_json_path("capability-test");

        // 创建并保存矩阵
        let matrix = CapabilityMatrix::new();
        matrix.save_to_file(&temp_path).unwrap();

        // 从文件加载
        let loaded = CapabilityMatrix::from_config_file(&temp_path).unwrap();

        // 验证数据一致
        assert_eq!(
            loaded.get_capability(ModelId::Qwen3B, &IntentType::Casual),
            0.85
        );
        assert_eq!(
            loaded.get_capability(ModelId::GPT4o, &IntentType::Reasoning),
            0.95
        );

        // 清理
        fs::remove_file(&temp_path).ok();
    }

    #[test]
    fn test_update_capability() {
        let mut matrix = CapabilityMatrix::new();

        // 更新能力得分
        matrix.update_capability(ModelId::Qwen3B, IntentType::Casual, 0.95);

        assert_eq!(
            matrix.get_capability(ModelId::Qwen3B, &IntentType::Casual),
            0.95
        );
    }

    #[test]
    fn test_update_capability_clamp() {
        let mut matrix = CapabilityMatrix::new();

        // 测试超出范围的值会被 clamp
        matrix.update_capability(ModelId::Qwen3B, IntentType::Casual, 1.5);
        assert_eq!(
            matrix.get_capability(ModelId::Qwen3B, &IntentType::Casual),
            1.0
        );

        matrix.update_capability(ModelId::Qwen3B, IntentType::Casual, -0.5);
        assert_eq!(
            matrix.get_capability(ModelId::Qwen3B, &IntentType::Casual),
            0.0
        );
    }

    #[test]
    fn test_batch_update_capabilities() {
        let mut matrix = CapabilityMatrix::new();

        let updates = vec![
            (ModelId::Qwen3B, IntentType::Casual, 0.90),
            (ModelId::Qwen3B, IntentType::Knowledge, 0.75),
            (ModelId::Qwen7B, IntentType::CodeGen, 0.85),
        ];

        matrix.update_capabilities(updates);

        assert_eq!(
            matrix.get_capability(ModelId::Qwen3B, &IntentType::Casual),
            0.90
        );
        assert_eq!(
            matrix.get_capability(ModelId::Qwen3B, &IntentType::Knowledge),
            0.75
        );
        assert_eq!(
            matrix.get_capability(ModelId::Qwen7B, &IntentType::CodeGen),
            0.85
        );
    }

    #[test]
    fn test_config_serialization() {
        let config = CapabilityConfig {
            capabilities: vec![
                CapabilityEntry {
                    model: ModelId::Qwen3B,
                    intent: IntentType::Casual,
                    score: 0.85,
                },
                CapabilityEntry {
                    model: ModelId::GPT4o,
                    intent: IntentType::Reasoning,
                    score: 0.95,
                },
            ],
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: CapabilityConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.capabilities.len(), 2);
        assert_eq!(decoded.capabilities[0].score, 0.85);
    }

    #[test]
    fn test_save_with_config_path() {
        use std::fs;
        let temp_path = crate::test_support::temp_json_path("capability-test");

        // 从文件加载（设置 config_path）
        let matrix = CapabilityMatrix::new();
        matrix.save_to_file(&temp_path).unwrap();

        let mut loaded = CapabilityMatrix::from_config_file(&temp_path).unwrap();

        // 修改并保存到默认路径
        loaded.update_capability(ModelId::Qwen3B, IntentType::Casual, 0.99);
        loaded.save().unwrap();

        // 重新加载验证
        let reloaded = CapabilityMatrix::from_config_file(&temp_path).unwrap();
        assert_eq!(
            reloaded.get_capability(ModelId::Qwen3B, &IntentType::Casual),
            0.99
        );

        // 清理
        fs::remove_file(&temp_path).ok();
    }
}
