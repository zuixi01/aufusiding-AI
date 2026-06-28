//! 自我进化系统
//!
//! 四维能力追踪（对话、推理、创意、工具）、能力评分模型、成长报告生成。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 能力维度
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum Dimension {
    /// 对话能力：理解、共情、表达
    Conversation,
    /// 推理能力：逻辑、分析、规划
    Reasoning,
    /// 创意能力：生成、联想、创新
    Creativity,
    /// 工具能力：代码、IoT、API 调用
    ToolUse,
}

/// 能力评估事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvent {
    pub dimension: Dimension,
    pub score: f64,
    pub context: String,
    pub timestamp: DateTime<Utc>,
}

/// 单维度能力状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionState {
    /// 当前能力分 [0.0, 100.0]
    pub score: f64,
    /// 历史最高分
    pub peak_score: f64,
    /// 评估次数
    pub eval_count: u64,
    /// EMA 平滑分数
    pub ema_score: f64,
    /// 最近趋势（正=成长，负=退化）
    pub trend: f64,
    /// 最近 N 次评分
    recent_scores: Vec<f64>,
}

const MAX_RECENT: usize = 20;
const EMA_ALPHA: f64 = 0.3;

impl DimensionState {
    fn new() -> Self {
        Self {
            score: 50.0,
            peak_score: 50.0,
            eval_count: 0,
            ema_score: 50.0,
            trend: 0.0,
            recent_scores: Vec::new(),
        }
    }

    fn record(&mut self, score: f64) {
        let clamped = score.clamp(0.0, 100.0);
        self.eval_count += 1;

        // EMA 更新
        if self.eval_count == 1 {
            self.ema_score = clamped;
        } else {
            self.ema_score = EMA_ALPHA * clamped + (1.0 - EMA_ALPHA) * self.ema_score;
        }

        self.score = self.ema_score;
        if self.score > self.peak_score {
            self.peak_score = self.score;
        }

        // 维护最近分数窗口
        self.recent_scores.push(clamped);
        if self.recent_scores.len() > MAX_RECENT {
            self.recent_scores.remove(0);
        }

        // 计算趋势：后半段均值 - 前半段均值
        self.trend = self.calculate_trend();
    }

    fn calculate_trend(&self) -> f64 {
        let n = self.recent_scores.len();
        if n < 4 {
            return 0.0;
        }
        let mid = n / 2;
        let first_half: f64 =
            self.recent_scores[..mid].iter().sum::<f64>() / mid as f64;
        let second_half: f64 =
            self.recent_scores[mid..].iter().sum::<f64>() / (n - mid) as f64;
        second_half - first_half
    }
}

/// 成长报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthReport {
    /// 报告生成时间
    pub generated_at: DateTime<Utc>,
    /// 各维度状态快照
    pub dimensions: HashMap<String, DimensionSnapshot>,
    /// 综合成长分
    pub overall_score: f64,
    /// 成长亮点
    pub highlights: Vec<String>,
    /// 改进建议
    pub suggestions: Vec<String>,
    /// 总评估次数
    pub total_evaluations: u64,
}

/// 维度快照（用于报告）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionSnapshot {
    pub score: f64,
    pub peak: f64,
    pub trend: f64,
    pub eval_count: u64,
}

/// 自我进化引擎
pub struct EvolutionEngine {
    dimensions: HashMap<Dimension, DimensionState>,
    events: Vec<SkillEvent>,
    max_events: usize,
}

impl EvolutionEngine {
    pub fn new() -> Self {
        let mut dimensions = HashMap::new();
        dimensions.insert(Dimension::Conversation, DimensionState::new());
        dimensions.insert(Dimension::Reasoning, DimensionState::new());
        dimensions.insert(Dimension::Creativity, DimensionState::new());
        dimensions.insert(Dimension::ToolUse, DimensionState::new());

        Self {
            dimensions,
            events: Vec::new(),
            max_events: 5000,
        }
    }

    /// 记录能力评估事件
    pub fn record_event(&mut self, dimension: Dimension, score: f64, context: &str) {
        let event = SkillEvent {
            dimension: dimension.clone(),
            score,
            context: context.to_string(),
            timestamp: Utc::now(),
        };

        if let Some(state) = self.dimensions.get_mut(&dimension) {
            state.record(score);
        }

        self.events.push(event);
        if self.events.len() > self.max_events {
            self.events.remove(0);
        }
    }

    /// 获取某维度的当前分数
    pub fn get_score(&self, dimension: &Dimension) -> f64 {
        self.dimensions
            .get(dimension)
            .map(|s| s.score)
            .unwrap_or(0.0)
    }

    /// 获取某维度的趋势
    pub fn get_trend(&self, dimension: &Dimension) -> f64 {
        self.dimensions
            .get(dimension)
            .map(|s| s.trend)
            .unwrap_or(0.0)
    }

    /// 获取综合分数（四维加权平均）
    pub fn overall_score(&self) -> f64 {
        let weights: HashMap<Dimension, f64> = [
            (Dimension::Conversation, 0.3),
            (Dimension::Reasoning, 0.3),
            (Dimension::Creativity, 0.2),
            (Dimension::ToolUse, 0.2),
        ]
        .into_iter()
        .collect();

        let mut total = 0.0;
        let mut weight_sum = 0.0;
        for (dim, state) in &self.dimensions {
            let w = weights.get(dim).copied().unwrap_or(0.25);
            total += state.score * w;
            weight_sum += w;
        }
        if weight_sum > 0.0 {
            total / weight_sum
        } else {
            0.0
        }
    }

    /// 找出最强维度
    pub fn strongest_dimension(&self) -> Option<(Dimension, f64)> {
        self.dimensions
            .iter()
            .max_by(|a, b| a.1.score.partial_cmp(&b.1.score).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(d, s)| (d.clone(), s.score))
    }

    /// 找出最弱维度
    pub fn weakest_dimension(&self) -> Option<(Dimension, f64)> {
        self.dimensions
            .iter()
            .min_by(|a, b| a.1.score.partial_cmp(&b.1.score).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(d, s)| (d.clone(), s.score))
    }

    /// 生成成长报告
    pub fn generate_report(&self) -> GrowthReport {
        let mut highlights = Vec::new();
        let mut suggestions = Vec::new();
        let mut dim_snapshots = HashMap::new();

        for (dim, state) in &self.dimensions {
            let dim_name = format!("{:?}", dim);
            dim_snapshots.insert(
                dim_name.clone(),
                DimensionSnapshot {
                    score: state.score,
                    peak: state.peak_score,
                    trend: state.trend,
                    eval_count: state.eval_count,
                },
            );

            // 亮点：分数高或趋势正
            if state.score >= 70.0 {
                highlights.push(format!("{}能力表现优秀（{:.1}分）", dim_name, state.score));
            }
            if state.trend > 5.0 {
                highlights.push(format!("{}能力持续成长（趋势 +{:.1}）", dim_name, state.trend));
            }

            // 建议：分数低或趋势负
            if state.score < 40.0 {
                suggestions.push(format!("{}能力需要加强（{:.1}分）", dim_name, state.score));
            }
            if state.trend < -5.0 {
                suggestions.push(format!(
                    "{}能力有退化趋势（{:.1}），建议增加相关练习",
                    dim_name, state.trend
                ));
            }
        }

        let total_evals = self.dimensions.values().map(|s| s.eval_count).sum();

        GrowthReport {
            generated_at: Utc::now(),
            dimensions: dim_snapshots,
            overall_score: self.overall_score(),
            highlights,
            suggestions,
            total_evaluations: total_evals,
        }
    }

    /// 获取事件历史
    pub fn events(&self) -> &[SkillEvent] {
        &self.events
    }

    /// 获取维度状态
    pub fn dimension_state(&self, dim: &Dimension) -> Option<&DimensionState> {
        self.dimensions.get(dim)
    }
}

impl Default for EvolutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_engine() {
        let engine = EvolutionEngine::new();
        assert_eq!(engine.dimensions.len(), 4);
        // 初始分数都是 50
        assert_eq!(engine.get_score(&Dimension::Conversation), 50.0);
        assert_eq!(engine.get_score(&Dimension::Reasoning), 50.0);
    }

    #[test]
    fn test_record_event_updates_score() {
        let mut engine = EvolutionEngine::new();
        engine.record_event(Dimension::Reasoning, 80.0, "solved complex problem");

        let score = engine.get_score(&Dimension::Reasoning);
        // 第一次记录直接设为输入值
        assert!((score - 80.0).abs() < 0.1);
        assert_eq!(engine.events().len(), 1);
    }

    #[test]
    fn test_score_converges_with_repeated_high() {
        let mut engine = EvolutionEngine::new();
        for _ in 0..20 {
            engine.record_event(Dimension::Creativity, 90.0, "creative task");
        }
        let score = engine.get_score(&Dimension::Creativity);
        // 应该接近 90
        assert!(score > 85.0);
    }

    #[test]
    fn test_peak_score_tracked() {
        let mut engine = EvolutionEngine::new();
        engine.record_event(Dimension::ToolUse, 90.0, "");
        engine.record_event(Dimension::ToolUse, 30.0, "");

        let state = engine.dimension_state(&Dimension::ToolUse).unwrap();
        // peak 应该记录了第一次高分后的 EMA
        assert!(state.peak_score > 50.0);
        // 当前分应该因为低分而下降
        assert!(state.score < state.peak_score);
    }

    #[test]
    fn test_trend_calculation() {
        let mut engine = EvolutionEngine::new();
        // 先低后高 → 正趋势
        for i in 0..10 {
            let score = 30.0 + (i as f64) * 7.0;
            engine.record_event(Dimension::Conversation, score, "");
        }
        let trend = engine.get_trend(&Dimension::Conversation);
        assert!(trend > 0.0, "trend should be positive: {}", trend);
    }

    #[test]
    fn test_trend_negative() {
        let mut engine = EvolutionEngine::new();
        // 先高后低 → 负趋势
        for i in 0..10 {
            let score = 90.0 - (i as f64) * 7.0;
            engine.record_event(Dimension::Reasoning, score, "");
        }
        let trend = engine.get_trend(&Dimension::Reasoning);
        assert!(trend < 0.0, "trend should be negative: {}", trend);
    }

    #[test]
    fn test_overall_score() {
        let engine = EvolutionEngine::new();
        // 初始所有维度 50 分，综合也应该是 50
        let overall = engine.overall_score();
        assert!((overall - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_strongest_weakest() {
        let mut engine = EvolutionEngine::new();
        engine.record_event(Dimension::Creativity, 95.0, "");
        engine.record_event(Dimension::ToolUse, 20.0, "");

        let (strongest, _) = engine.strongest_dimension().unwrap();
        assert_eq!(strongest, Dimension::Creativity);

        let (weakest, _) = engine.weakest_dimension().unwrap();
        assert_eq!(weakest, Dimension::ToolUse);
    }

    #[test]
    fn test_generate_report() {
        let mut engine = EvolutionEngine::new();
        // 推理能力高
        for _ in 0..10 {
            engine.record_event(Dimension::Reasoning, 85.0, "logic");
        }
        // 创意能力低
        for _ in 0..10 {
            engine.record_event(Dimension::Creativity, 25.0, "creative");
        }

        let report = engine.generate_report();
        assert_eq!(report.dimensions.len(), 4);
        assert!(report.total_evaluations >= 20);

        // 推理应该有亮点
        assert!(report.highlights.iter().any(|h| h.contains("Reasoning")));
        // 创意应该有建议
        assert!(report.suggestions.iter().any(|s| s.contains("Creativity")));
    }

    #[test]
    fn test_report_with_trends() {
        let mut engine = EvolutionEngine::new();
        // 对话能力持续成长
        for i in 0..10 {
            engine.record_event(Dimension::Conversation, 50.0 + i as f64 * 5.0, "");
        }

        let report = engine.generate_report();
        let conv = report.dimensions.get("Conversation").unwrap();
        assert!(conv.trend > 0.0);
    }

    #[test]
    fn test_max_events_eviction() {
        let mut engine = EvolutionEngine::new();
        engine.max_events = 10;

        for i in 0..20 {
            engine.record_event(Dimension::ToolUse, i as f64 * 5.0, "");
        }
        assert_eq!(engine.events().len(), 10);
    }

    #[test]
    fn test_score_clamped() {
        let mut engine = EvolutionEngine::new();
        engine.record_event(Dimension::Conversation, 200.0, "");
        let score = engine.get_score(&Dimension::Conversation);
        assert!(score <= 100.0);

        engine.record_event(Dimension::Conversation, -50.0, "");
        let score = engine.get_score(&Dimension::Conversation);
        assert!(score >= 0.0);
    }
}
