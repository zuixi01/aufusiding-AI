//! 经济皮层 - 成本追踪与 ROI 优化
//!
//! 实现：
//! - 实时成本追踪（EMA 平滑）
//! - ROI 计算与优化
//! - 预算管理与告警

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 成本记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostRecord {
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 模型 ID
    pub model_id: String,
    /// 输入 token 数
    pub input_tokens: usize,
    /// 输出 token 数
    pub output_tokens: usize,
    /// 成本（美元）
    pub cost: f64,
    /// 延迟（毫秒）
    pub latency_ms: u64,
    /// 用户满意度 [0.0, 1.0]
    pub satisfaction: Option<f64>,
}

/// 模型统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    /// 调用次数
    pub call_count: usize,
    /// 总成本
    pub total_cost: f64,
    /// 平均成本（EMA）
    pub avg_cost_ema: f64,
    /// 平均延迟（EMA）
    pub avg_latency_ema: f64,
    /// 平均满意度（EMA）
    pub avg_satisfaction_ema: f64,
    /// ROI 分数
    pub roi_score: f64,
}

impl ModelStats {
    fn new() -> Self {
        Self {
            call_count: 0,
            total_cost: 0.0,
            avg_cost_ema: 0.0,
            avg_latency_ema: 0.0,
            avg_satisfaction_ema: 0.5,
            roi_score: 0.0,
        }
    }
}

/// 预算配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// 每日预算（美元）
    pub daily_budget: f64,
    /// 每月预算（美元）
    pub monthly_budget: f64,
    /// 告警阈值（百分比）
    pub alert_threshold: f64,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            daily_budget: 10.0,
            monthly_budget: 300.0,
            alert_threshold: 0.8,
        }
    }
}

/// 经济皮层
pub struct EconomicCortex {
    /// 成本历史
    cost_history: Vec<CostRecord>,
    /// 模型统计
    model_stats: HashMap<String, ModelStats>,
    /// EMA 平滑因子
    alpha: f64,
    /// 预算配置
    budget: BudgetConfig,
    /// 当日累计成本
    daily_cost: f64,
    /// 当月累计成本
    monthly_cost: f64,
}

impl EconomicCortex {
    /// 创建经济皮层
    pub fn new(budget: BudgetConfig) -> Self {
        Self {
            cost_history: Vec::new(),
            model_stats: HashMap::new(),
            alpha: 0.2,
            budget,
            daily_cost: 0.0,
            monthly_cost: 0.0,
        }
    }

    /// 记录成本
    pub fn record_cost(&mut self, record: CostRecord) {
        let model_id = record.model_id.clone();
        let cost = record.cost;

        // 更新累计成本
        self.daily_cost += cost;
        self.monthly_cost += cost;

        // 更新模型统计
        let stats = self
            .model_stats
            .entry(model_id.clone())
            .or_insert_with(ModelStats::new);
        stats.call_count += 1;
        stats.total_cost += cost;

        // EMA 更新
        if stats.call_count == 1 {
            stats.avg_cost_ema = cost;
            stats.avg_latency_ema = record.latency_ms as f64;
            if let Some(sat) = record.satisfaction {
                stats.avg_satisfaction_ema = sat;
            }
        } else {
            stats.avg_cost_ema = self.alpha * cost + (1.0 - self.alpha) * stats.avg_cost_ema;
            stats.avg_latency_ema =
                self.alpha * record.latency_ms as f64 + (1.0 - self.alpha) * stats.avg_latency_ema;
            if let Some(sat) = record.satisfaction {
                stats.avg_satisfaction_ema =
                    self.alpha * sat + (1.0 - self.alpha) * stats.avg_satisfaction_ema;
            }
        }

        // 计算 ROI（先提取值，避免借用冲突）
        let avg_cost = stats.avg_cost_ema;
        let avg_latency = stats.avg_latency_ema;
        let avg_satisfaction = stats.avg_satisfaction_ema;

        // 成本为 0 时使用极小值避免除零，获得极高 ROI
        let cost_factor = 1.0 / (avg_cost + 0.001);
        let latency_factor = 1.0 / (avg_latency / 1000.0 + 0.1);
        let roi_score = avg_satisfaction * cost_factor * latency_factor;

        stats.roi_score = roi_score;

        // 保存记录
        self.cost_history.push(record);
    }

    /// 获取模型统计
    pub fn get_model_stats(&self, model_id: &str) -> Option<&ModelStats> {
        self.model_stats.get(model_id)
    }

    /// 获取所有模型统计
    pub fn get_all_stats(&self) -> &HashMap<String, ModelStats> {
        &self.model_stats
    }

    /// 获取最佳 ROI 模型
    pub fn get_best_roi_model(&self) -> Option<(String, f64)> {
        self.model_stats
            .iter()
            .max_by(|a, b| {
                a.1.roi_score
                    .partial_cmp(&b.1.roi_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(id, stats)| (id.clone(), stats.roi_score))
    }

    /// 推荐模型（基于 ROI 和预算约束）
    pub fn recommend_model(&self, max_cost: Option<f64>) -> Option<String> {
        let mut candidates: Vec<_> = self
            .model_stats
            .iter()
            .filter(|(_, stats)| {
                // 过滤掉成本超过预算的模型
                if let Some(max) = max_cost {
                    stats.avg_cost_ema <= max
                } else {
                    true
                }
            })
            .collect();

        // 按 ROI 排序
        candidates.sort_by(|a, b| {
            b.1.roi_score
                .partial_cmp(&a.1.roi_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates.first().map(|(id, _)| id.to_string())
    }

    /// 优化预算分配（返回各模型建议使用比例）
    pub fn optimize_budget_allocation(&self) -> HashMap<String, f64> {
        let total_roi: f64 = self.model_stats.values().map(|s| s.roi_score).sum();

        if total_roi <= 0.0 {
            return HashMap::new();
        }

        // 按 ROI 比例分配预算
        self.model_stats
            .iter()
            .map(|(id, stats)| {
                let allocation = stats.roi_score / total_roi;
                (id.clone(), allocation)
            })
            .collect()
    }

    /// 预测未来成本（基于历史趋势）
    pub fn predict_future_cost(&self, days: usize) -> f64 {
        if self.cost_history.is_empty() {
            return 0.0;
        }

        // 简化：使用最近的平均日成本
        let recent_daily_avg = self.daily_cost;
        recent_daily_avg * days as f64
    }

    /// 获取成本效益最高的模型（质量/成本比）
    pub fn get_most_cost_effective_model(&self) -> Option<(String, f64)> {
        self.model_stats
            .iter()
            .filter(|(_, stats)| stats.avg_cost_ema > 0.0)
            .map(|(id, stats)| {
                let cost_effectiveness = stats.avg_satisfaction_ema / stats.avg_cost_ema;
                (id.clone(), cost_effectiveness)
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// 動的予算調整（使用状況に基づく）
    pub fn suggest_budget_adjustment(&self) -> Option<String> {
        let daily_usage = self.daily_cost / self.budget.daily_budget.max(f64::EPSILON);
        let monthly_usage = self.monthly_cost / self.budget.monthly_budget.max(f64::EPSILON);

        if daily_usage > 1.2 || monthly_usage > 1.2 {
            Some(format!(
                "建议增加预算：日均使用 {:.1}%，月度使用 {:.1}%",
                daily_usage * 100.0,
                monthly_usage * 100.0
            ))
        } else if daily_usage < 0.3 && monthly_usage < 0.3 {
            Some(format!(
                "预算利用率较低：日均 {:.1}%，月度 {:.1}%，可考虑降低预算",
                daily_usage * 100.0,
                monthly_usage * 100.0
            ))
        } else {
            None
        }
    }

    /// 检查预算告警
    pub fn check_budget_alert(&self) -> Vec<String> {
        let mut alerts = Vec::new();

        let daily_usage = self.daily_cost / self.budget.daily_budget.max(f64::EPSILON);
        if daily_usage >= self.budget.alert_threshold {
            alerts.push(format!(
                "每日预算使用 {:.1}%（${:.2}/${:.2}）",
                daily_usage * 100.0,
                self.daily_cost,
                self.budget.daily_budget
            ));
        }

        let monthly_usage = self.monthly_cost / self.budget.monthly_budget.max(f64::EPSILON);
        if monthly_usage >= self.budget.alert_threshold {
            alerts.push(format!(
                "每月预算使用 {:.1}%（${:.2}/${:.2}）",
                monthly_usage * 100.0,
                self.monthly_cost,
                self.budget.monthly_budget
            ));
        }

        alerts
    }

    /// 获取当日成本
    pub fn get_daily_cost(&self) -> f64 {
        self.daily_cost
    }

    /// 获取当月成本
    pub fn get_monthly_cost(&self) -> f64 {
        self.monthly_cost
    }

    /// 重置每日成本
    pub fn reset_daily_cost(&mut self) {
        self.daily_cost = 0.0;
    }

    /// 重置每月成本
    pub fn reset_monthly_cost(&mut self) {
        self.monthly_cost = 0.0;
    }

    /// 获取成本历史
    pub fn get_cost_history(&self) -> &[CostRecord] {
        &self.cost_history
    }

    /// 清空历史
    pub fn clear_history(&mut self) {
        self.cost_history.clear();
    }

    /// 生成价值仪表盘快照
    pub fn dashboard(&self) -> ValueDashboard {
        let model_count = self.model_stats.len();
        let total_calls: usize = self.model_stats.values().map(|s| s.call_count).sum();
        let total_cost: f64 = self.model_stats.values().map(|s| s.total_cost).sum();
        let avg_satisfaction: f64 = if model_count > 0 {
            self.model_stats
                .values()
                .map(|s| s.avg_satisfaction_ema)
                .sum::<f64>()
                / model_count as f64
        } else {
            0.0
        };

        let best_roi = self.get_best_roi_model();
        let most_cost_effective = self.get_most_cost_effective_model();
        let alerts = self.check_budget_alert();

        ValueDashboard {
            daily_cost: self.daily_cost,
            monthly_cost: self.monthly_cost,
            daily_budget: self.budget.daily_budget,
            monthly_budget: self.budget.monthly_budget,
            daily_utilization: self.daily_cost / self.budget.daily_budget,
            monthly_utilization: self.monthly_cost / self.budget.monthly_budget,
            total_calls,
            total_cost,
            model_count,
            avg_satisfaction,
            best_roi_model: best_roi,
            most_cost_effective_model: most_cost_effective,
            active_alerts: alerts,
        }
    }

    /// 经济代理：自动优化建议
    pub fn agent_recommendations(&self) -> Vec<EconomicRecommendation> {
        let mut recs = Vec::new();

        // 1. 预算告警
        let daily_util = self.daily_cost / self.budget.daily_budget;
        let monthly_util = self.monthly_cost / self.budget.monthly_budget;

        if daily_util > 1.0 {
            recs.push(EconomicRecommendation {
                priority: RecommendationPriority::Critical,
                category: "budget".to_string(),
                message: format!(
                    "日预算已超支 {:.1}%，建议切换到更经济的模型",
                    (daily_util - 1.0) * 100.0
                ),
                action: RecommendedAction::SwitchModel {
                    from: self.most_expensive_model().unwrap_or_default(),
                    to: self
                        .recommend_model(Some(self.budget.daily_budget * 0.1))
                        .unwrap_or_default(),
                },
            });
        } else if daily_util > self.budget.alert_threshold {
            recs.push(EconomicRecommendation {
                priority: RecommendationPriority::High,
                category: "budget".to_string(),
                message: format!("日预算使用 {:.1}%，接近上限", daily_util * 100.0),
                action: RecommendedAction::ReduceUsage,
            });
        }

        // 2. ROI 优化
        if let (Some((best, best_roi)), Some((worst, worst_roi))) =
            (self.get_best_roi_model(), self.get_worst_roi_model())
        {
            if best_roi > worst_roi * 3.0 && best != worst {
                recs.push(EconomicRecommendation {
                    priority: RecommendationPriority::Medium,
                    category: "optimization".to_string(),
                    message: format!(
                        "模型「{}」ROI 是「{}」的 {:.1} 倍，建议迁移低价值调用",
                        best,
                        worst,
                        best_roi / worst_roi.max(0.001)
                    ),
                    action: RecommendedAction::SwitchModel {
                        from: worst,
                        to: best,
                    },
                });
            }
        }

        // 3. 低利用率提醒
        if monthly_util < 0.2 && !self.cost_history.is_empty() {
            recs.push(EconomicRecommendation {
                priority: RecommendationPriority::Low,
                category: "budget".to_string(),
                message: format!(
                    "月预算利用率仅 {:.1}%，可考虑降低预算或升级模型",
                    monthly_util * 100.0
                ),
                action: RecommendedAction::AdjustBudget,
            });
        }

        recs
    }

    /// 获取 ROI 最差的模型
    fn get_worst_roi_model(&self) -> Option<(String, f64)> {
        self.model_stats
            .iter()
            .min_by(|a, b| {
                a.1.roi_score
                    .partial_cmp(&b.1.roi_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(id, stats)| (id.clone(), stats.roi_score))
    }

    /// 获取成本最高的模型
    fn most_expensive_model(&self) -> Option<String> {
        self.model_stats
            .iter()
            .max_by(|a, b| {
                a.1.avg_cost_ema
                    .partial_cmp(&b.1.avg_cost_ema)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(id, _)| id.clone())
    }
}

impl Default for EconomicCortex {
    fn default() -> Self {
        Self::new(BudgetConfig::default())
    }
}

/// 价值仪表盘快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueDashboard {
    pub daily_cost: f64,
    pub monthly_cost: f64,
    pub daily_budget: f64,
    pub monthly_budget: f64,
    pub daily_utilization: f64,
    pub monthly_utilization: f64,
    pub total_calls: usize,
    pub total_cost: f64,
    pub model_count: usize,
    pub avg_satisfaction: f64,
    pub best_roi_model: Option<(String, f64)>,
    pub most_cost_effective_model: Option<(String, f64)>,
    pub active_alerts: Vec<String>,
}

/// 经济推荐
#[derive(Debug, Clone)]
pub struct EconomicRecommendation {
    pub priority: RecommendationPriority,
    pub category: String,
    pub message: String,
    pub action: RecommendedAction,
}

/// 推荐优先级
#[derive(Debug, Clone, PartialEq)]
pub enum RecommendationPriority {
    Critical,
    High,
    Medium,
    Low,
}

/// 推荐行动
#[derive(Debug, Clone)]
pub enum RecommendedAction {
    SwitchModel { from: String, to: String },
    ReduceUsage,
    AdjustBudget,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_record(
        model_id: &str,
        cost: f64,
        latency_ms: u64,
        satisfaction: Option<f64>,
    ) -> CostRecord {
        CostRecord {
            timestamp: Utc::now(),
            model_id: model_id.to_string(),
            input_tokens: 100,
            output_tokens: 50,
            cost,
            latency_ms,
            satisfaction,
        }
    }

    #[test]
    fn test_create_economic_cortex() {
        let cortex = EconomicCortex::default();
        assert_eq!(cortex.daily_cost, 0.0);
        assert_eq!(cortex.monthly_cost, 0.0);
    }

    #[test]
    fn test_record_cost() {
        let mut cortex = EconomicCortex::default();
        let record = create_test_record("gpt-4", 0.05, 1000, Some(0.9));

        cortex.record_cost(record);

        assert_eq!(cortex.daily_cost, 0.05);
        assert_eq!(cortex.monthly_cost, 0.05);
        assert_eq!(cortex.cost_history.len(), 1);
    }

    #[test]
    fn test_model_stats_update() {
        let mut cortex = EconomicCortex::default();

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        cortex.record_cost(create_test_record("gpt-4", 0.06, 1200, Some(0.8)));

        let stats = cortex.get_model_stats("gpt-4").unwrap();
        assert_eq!(stats.call_count, 2);
        assert_eq!(stats.total_cost, 0.11);
        assert!(stats.avg_cost_ema > 0.0);
    }

    #[test]
    fn test_ema_calculation() {
        let mut cortex = EconomicCortex::default();
        cortex.alpha = 0.2;

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        let stats1 = cortex.get_model_stats("gpt-4").unwrap();
        assert_eq!(stats1.avg_cost_ema, 0.05);

        cortex.record_cost(create_test_record("gpt-4", 0.10, 1000, Some(0.9)));
        let stats2 = cortex.get_model_stats("gpt-4").unwrap();
        // EMA = 0.2 * 0.10 + 0.8 * 0.05 = 0.02 + 0.04 = 0.06
        assert!((stats2.avg_cost_ema - 0.06).abs() < 0.001);
    }

    #[test]
    fn test_roi_calculation() {
        let mut cortex = EconomicCortex::default();

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        cortex.record_cost(create_test_record("claude", 0.03, 800, Some(0.85)));

        let gpt4_stats = cortex.get_model_stats("gpt-4").unwrap();
        let claude_stats = cortex.get_model_stats("claude").unwrap();

        assert!(gpt4_stats.roi_score > 0.0);
        assert!(claude_stats.roi_score > 0.0);
    }

    #[test]
    fn test_recommend_model() {
        let mut cortex = EconomicCortex::default();

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        cortex.record_cost(create_test_record("claude", 0.03, 800, Some(0.85)));
        cortex.record_cost(create_test_record("gemini", 0.02, 900, Some(0.75)));

        // 无预算限制，应推荐 ROI 最高的
        let recommended = cortex.recommend_model(None);
        assert!(recommended.is_some());

        // 预算限制 0.04，gpt-4 被排除
        let recommended = cortex.recommend_model(Some(0.04)).unwrap();
        assert!(recommended == "claude" || recommended == "gemini");

        // 严格预算限制 0.025，只有 gemini 满足
        let recommended = cortex.recommend_model(Some(0.025)).unwrap();
        assert_eq!(recommended, "gemini");
    }

    #[test]
    fn test_optimize_budget_allocation() {
        let mut cortex = EconomicCortex::default();

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        cortex.record_cost(create_test_record("claude", 0.03, 800, Some(0.85)));

        let allocation = cortex.optimize_budget_allocation();

        assert_eq!(allocation.len(), 2);
        // 分配比例之和应为 1.0
        let sum: f64 = allocation.values().sum();
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_predict_future_cost() {
        let mut cortex = EconomicCortex::default();

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));

        let predicted = cortex.predict_future_cost(7);
        assert_eq!(predicted, 0.05 * 7.0);
    }

    #[test]
    fn test_cost_effective_model() {
        let mut cortex = EconomicCortex::default();

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        cortex.record_cost(create_test_record("claude", 0.03, 800, Some(0.85)));

        let (model, _score) = cortex.get_most_cost_effective_model().unwrap();
        // claude: 0.85/0.03 = 28.33, gpt-4: 0.9/0.05 = 18
        assert_eq!(model, "claude");
    }

    #[test]
    fn test_budget_adjustment_suggestion() {
        let budget = BudgetConfig {
            daily_budget: 1.0,
            monthly_budget: 30.0,
            alert_threshold: 0.8,
        };
        let mut cortex = EconomicCortex::new(budget);

        // 低使用率
        cortex.record_cost(create_test_record("gpt-4", 0.2, 1000, Some(0.9)));
        let suggestion = cortex.suggest_budget_adjustment();
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("利用率较低"));

        // 高使用率
        cortex.reset_daily_cost();
        cortex.reset_monthly_cost();
        cortex.record_cost(create_test_record("gpt-4", 1.5, 1000, Some(0.9)));
        let suggestion = cortex.suggest_budget_adjustment();
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("建议增加预算"));
    }

    #[test]
    fn test_best_roi_model() {
        let mut cortex = EconomicCortex::default();

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        cortex.record_cost(create_test_record("claude", 0.03, 800, Some(0.85)));
        cortex.record_cost(create_test_record("local", 0.0, 500, Some(0.7)));

        let (best_model, _score) = cortex.get_best_roi_model().unwrap();
        // local 应该有最高 ROI（成本为 0）
        assert_eq!(best_model, "local");
    }

    #[test]
    fn test_budget_alert() {
        let budget = BudgetConfig {
            daily_budget: 1.0,
            monthly_budget: 30.0,
            alert_threshold: 0.8,
        };
        let mut cortex = EconomicCortex::new(budget);

        // 未超过阈值
        cortex.record_cost(create_test_record("gpt-4", 0.5, 1000, Some(0.9)));
        assert!(cortex.check_budget_alert().is_empty());

        // 超过阈值
        cortex.record_cost(create_test_record("gpt-4", 0.4, 1000, Some(0.9)));
        let alerts = cortex.check_budget_alert();
        assert!(!alerts.is_empty());
        assert!(alerts[0].contains("每日预算"));
    }

    #[test]
    fn test_multiple_models() {
        let mut cortex = EconomicCortex::default();

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        cortex.record_cost(create_test_record("claude", 0.03, 800, Some(0.85)));
        cortex.record_cost(create_test_record("local", 0.0, 500, Some(0.7)));

        assert_eq!(cortex.get_all_stats().len(), 3);
    }

    #[test]
    fn test_reset_costs() {
        let mut cortex = EconomicCortex::default();

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        assert_eq!(cortex.get_daily_cost(), 0.05);

        cortex.reset_daily_cost();
        assert_eq!(cortex.get_daily_cost(), 0.0);
        assert_eq!(cortex.get_monthly_cost(), 0.05);

        cortex.reset_monthly_cost();
        assert_eq!(cortex.get_monthly_cost(), 0.0);
    }

    #[test]
    fn test_satisfaction_ema() {
        let mut cortex = EconomicCortex::default();
        cortex.alpha = 0.2;

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.7)));

        let stats = cortex.get_model_stats("gpt-4").unwrap();
        // EMA = 0.2 * 0.7 + 0.8 * 0.9 = 0.14 + 0.72 = 0.86
        assert!((stats.avg_satisfaction_ema - 0.86).abs() < 0.001);
    }

    #[test]
    fn test_clear_history() {
        let mut cortex = EconomicCortex::default();

        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        cortex.record_cost(create_test_record("claude", 0.03, 800, Some(0.85)));

        assert_eq!(cortex.get_cost_history().len(), 2);

        cortex.clear_history();
        assert_eq!(cortex.get_cost_history().len(), 0);
    }

    #[test]
    fn test_dashboard_snapshot() {
        let mut cortex = EconomicCortex::default();
        cortex.record_cost(create_test_record("gpt-4", 0.05, 1000, Some(0.9)));
        cortex.record_cost(create_test_record("claude", 0.03, 800, Some(0.85)));

        let dash = cortex.dashboard();
        assert_eq!(dash.total_calls, 2);
        assert!((dash.total_cost - 0.08).abs() < 0.001);
        assert_eq!(dash.model_count, 2);
        assert!(dash.avg_satisfaction > 0.8);
        assert!(dash.best_roi_model.is_some());
        assert!(dash.most_cost_effective_model.is_some());
        assert!(dash.daily_utilization > 0.0);
    }

    #[test]
    fn test_dashboard_empty() {
        let cortex = EconomicCortex::default();
        let dash = cortex.dashboard();
        assert_eq!(dash.total_calls, 0);
        assert_eq!(dash.total_cost, 0.0);
        assert_eq!(dash.model_count, 0);
    }

    #[test]
    fn test_agent_recommendations_over_budget() {
        let budget = BudgetConfig {
            daily_budget: 0.05,
            monthly_budget: 1.5,
            alert_threshold: 0.8,
        };
        let mut cortex = EconomicCortex::new(budget);
        cortex.record_cost(create_test_record("gpt-4", 0.06, 1000, Some(0.9)));

        let recs = cortex.agent_recommendations();
        assert!(!recs.is_empty());
        assert_eq!(recs[0].priority, RecommendationPriority::Critical);
        assert!(recs[0].message.contains("超支"));
    }

    #[test]
    fn test_agent_recommendations_low_utilization() {
        let budget = BudgetConfig {
            daily_budget: 100.0,
            monthly_budget: 3000.0,
            alert_threshold: 0.8,
        };
        let mut cortex = EconomicCortex::new(budget);
        cortex.record_cost(create_test_record("gpt-4", 0.01, 500, Some(0.9)));

        let recs = cortex.agent_recommendations();
        assert!(recs
            .iter()
            .any(|r| r.priority == RecommendationPriority::Low));
    }

    #[test]
    fn test_agent_recommendations_roi_optimization() {
        let mut cortex = EconomicCortex::default();
        // 高 ROI 模型
        cortex.record_cost(create_test_record("local", 0.0, 200, Some(0.8)));
        // 低 ROI 模型
        cortex.record_cost(create_test_record("expensive", 0.5, 2000, Some(0.3)));

        let recs = cortex.agent_recommendations();
        let has_switch = recs.iter().any(|r| r.category == "optimization");
        assert!(has_switch);
    }

    #[test]
    fn test_agent_recommendations_healthy() {
        let budget = BudgetConfig {
            daily_budget: 1.0,
            monthly_budget: 30.0,
            alert_threshold: 0.8,
        };
        let mut cortex = EconomicCortex::new(budget);
        cortex.record_cost(create_test_record("gpt-4", 0.5, 1000, Some(0.9)));

        let recs = cortex.agent_recommendations();
        // 正常使用不应有 Critical 告警
        assert!(!recs
            .iter()
            .any(|r| r.priority == RecommendationPriority::Critical));
    }
}
