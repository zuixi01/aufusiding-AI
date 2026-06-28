//! 成本计算模型
//!
//! 计算使用不同模型的成本，包括 API 费用和延迟成本。

use super::capability::ModelId;
use super::classifier::IntentType;

/// 成本计算器
pub struct CostCalculator {
    /// API 价格表（USD per 1M tokens）
    pricing: PricingTable,
    /// 用户时间价值（USD per hour）
    user_time_value: f64,
}

/// API 价格表
struct PricingTable {
    /// 输入 token 价格（USD per 1M tokens）
    input_prices: Vec<(ModelId, f64)>,
    /// 输出 token 价格（USD per 1M tokens）
    output_prices: Vec<(ModelId, f64)>,
}

impl PricingTable {
    fn new() -> Self {
        Self {
            input_prices: vec![
                (ModelId::Qwen3B, 0.0),
                (ModelId::Qwen7B, 0.0),
                (ModelId::Qwen14B, 0.0),
                (ModelId::QwenTurbo, 0.30),
                (ModelId::QwenMax, 2.00),
                (ModelId::DeepSeekChat, 0.14),
                (ModelId::GPT4oMini, 0.15),
                (ModelId::GPT4o, 2.50),
                (ModelId::Claude35Sonnet, 3.00),
                (ModelId::ClaudeOpus, 15.00),
                (ModelId::GeminiFlash, 0.075),
                (ModelId::GeminiPro, 1.25),
            ],
            output_prices: vec![
                (ModelId::Qwen3B, 0.0),
                (ModelId::Qwen7B, 0.0),
                (ModelId::Qwen14B, 0.0),
                (ModelId::QwenTurbo, 0.60),
                (ModelId::QwenMax, 6.00),
                (ModelId::DeepSeekChat, 0.28),
                (ModelId::GPT4oMini, 0.60),
                (ModelId::GPT4o, 10.00),
                (ModelId::Claude35Sonnet, 15.00),
                (ModelId::ClaudeOpus, 75.00),
                (ModelId::GeminiFlash, 0.30),
                (ModelId::GeminiPro, 5.00),
            ],
        }
    }

    fn get_input_price(&self, model: ModelId) -> f64 {
        self.input_prices
            .iter()
            .find(|(m, _)| *m == model)
            .map(|(_, price)| *price)
            .unwrap_or(0.0)
    }

    fn get_output_price(&self, model: ModelId) -> f64 {
        self.output_prices
            .iter()
            .find(|(m, _)| *m == model)
            .map(|(_, price)| *price)
            .unwrap_or(0.0)
    }
}

impl CostCalculator {
    /// 创建新的成本计算器
    pub fn new() -> Self {
        Self {
            pricing: PricingTable::new(),
            user_time_value: 20.0, // 默认 $20/hour
        }
    }

    /// 设置用户时间价值
    pub fn with_user_time_value(mut self, value: f64) -> Self {
        self.user_time_value = value;
        self
    }

    /// 计算总成本：API 费用 + 延迟成本
    pub fn calculate_cost(
        &self,
        model: ModelId,
        input_tokens: u64,
        output_tokens: u64,
        latency_ms: u64,
    ) -> f64 {
        let api_cost = self.calculate_api_cost(model, input_tokens, output_tokens);
        let latency_cost = self.calculate_latency_cost(latency_ms);

        api_cost + latency_cost
    }

    /// 计算 API 费用
    pub fn calculate_api_cost(&self, model: ModelId, input_tokens: u64, output_tokens: u64) -> f64 {
        let input_price = self.pricing.get_input_price(model);
        let output_price = self.pricing.get_output_price(model);

        let input_cost = (input_tokens as f64 * input_price) / 1_000_000.0;
        let output_cost = (output_tokens as f64 * output_price) / 1_000_000.0;

        input_cost + output_cost
    }

    /// 计算延迟成本（用户等待时间的价值）
    pub fn calculate_latency_cost(&self, latency_ms: u64) -> f64 {
        let latency_hours = latency_ms as f64 / (1000.0 * 3600.0);
        latency_hours * self.user_time_value
    }

    /// 根据意图估算输出 token 数量
    pub fn estimate_output_tokens(&self, input_tokens: u64, intent: &IntentType) -> u64 {
        let multiplier = match intent {
            IntentType::Casual => 0.5,    // 简单回复
            IntentType::Knowledge => 1.0, // 中等长度
            IntentType::ToolUse => 0.3,   // 工具调用结果
            IntentType::CodeGen => 2.0,   // 代码生成较长
            IntentType::Creative => 2.0,  // 创意写作较长
            IntentType::Reasoning => 1.5, // 推理过程较长
            IntentType::Unknown => 1.0,
        };

        (input_tokens as f64 * multiplier) as u64
    }

    /// 估算模型延迟（毫秒）
    pub fn estimate_latency(&self, model: ModelId, total_tokens: u64) -> u64 {
        let base_latency = match model {
            ModelId::Qwen3B => 50,          // 本地 3B 模型很快
            ModelId::Qwen7B => 100,         // 本地 7B 模型
            ModelId::Qwen14B => 200,        // 本地 14B 模型
            ModelId::QwenTurbo => 300,      // 阿里云低价
            ModelId::QwenMax => 500,        // 阿里云中端
            ModelId::DeepSeekChat => 400,   // DeepSeek
            ModelId::GPT4oMini => 500,      // 云端 API
            ModelId::GPT4o => 800,          // 云端大模型
            ModelId::Claude35Sonnet => 700, // 云端大模型
            ModelId::ClaudeOpus => 1000,    // 顶尖模型
            ModelId::GeminiFlash => 300,    // 快速模型
            ModelId::GeminiPro => 600,      // 中高端
        };

        // 根据 token 数量调整延迟
        let token_factor = (total_tokens as f64 / 100.0).sqrt();
        (base_latency as f64 * token_factor) as u64
    }

    /// 计算完整的预估成本（包含估算的输出和延迟）
    pub fn estimate_total_cost(
        &self,
        model: ModelId,
        input_tokens: u64,
        intent: &IntentType,
    ) -> f64 {
        let output_tokens = self.estimate_output_tokens(input_tokens, intent);
        let total_tokens = input_tokens + output_tokens;
        let latency = self.estimate_latency(model, total_tokens);

        self.calculate_cost(model, input_tokens, output_tokens, latency)
    }
}

impl Default for CostCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_model_zero_api_cost() {
        let calculator = CostCalculator::new();

        // 本地模型 API 成本为 0
        let cost = calculator.calculate_api_cost(ModelId::Qwen3B, 1000, 500);
        assert_eq!(cost, 0.0);

        let cost = calculator.calculate_api_cost(ModelId::Qwen7B, 1000, 500);
        assert_eq!(cost, 0.0);

        let cost = calculator.calculate_api_cost(ModelId::Qwen14B, 1000, 500);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_gpt4o_mini_api_cost() {
        let calculator = CostCalculator::new();

        // GPT-4o-mini: $0.15/1M input, $0.60/1M output
        let cost = calculator.calculate_api_cost(ModelId::GPT4oMini, 1_000_000, 1_000_000);
        let expected = 0.15 + 0.60;
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn test_gpt4o_api_cost() {
        let calculator = CostCalculator::new();

        // GPT-4o: $2.50/1M input, $10.00/1M output
        let cost = calculator.calculate_api_cost(ModelId::GPT4o, 1_000_000, 1_000_000);
        let expected = 2.50 + 10.00;
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn test_claude_api_cost() {
        let calculator = CostCalculator::new();

        // Claude-3.5-Sonnet: $3.00/1M input, $15.00/1M output
        let cost = calculator.calculate_api_cost(ModelId::Claude35Sonnet, 1_000_000, 1_000_000);
        let expected = 3.00 + 15.00;
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn test_small_token_cost() {
        let calculator = CostCalculator::new();

        // 测试小量 token 的成本计算
        // 100 input + 50 output tokens for GPT-4o-mini
        let cost = calculator.calculate_api_cost(ModelId::GPT4oMini, 100, 50);
        let expected = (100.0 * 0.15 + 50.0 * 0.60) / 1_000_000.0;
        assert!((cost - expected).abs() < 0.000001);
    }

    #[test]
    fn test_latency_cost() {
        let calculator = CostCalculator::new();

        // 1 秒延迟 = 1000ms
        // 用户时间价值 $20/hour = $20/3600s ≈ $0.00556/s
        let cost = calculator.calculate_latency_cost(1000);
        let expected = 20.0 / 3600.0;
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn test_custom_user_time_value() {
        let calculator = CostCalculator::new().with_user_time_value(50.0);

        let cost = calculator.calculate_latency_cost(1000);
        let expected = 50.0 / 3600.0;
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn test_estimate_output_tokens() {
        let calculator = CostCalculator::new();

        // 简单对话：0.5x
        assert_eq!(
            calculator.estimate_output_tokens(100, &IntentType::Casual),
            50
        );

        // 知识问答：1.0x
        assert_eq!(
            calculator.estimate_output_tokens(100, &IntentType::Knowledge),
            100
        );

        // 代码生成：2.0x
        assert_eq!(
            calculator.estimate_output_tokens(100, &IntentType::CodeGen),
            200
        );

        // 创意写作：2.0x
        assert_eq!(
            calculator.estimate_output_tokens(100, &IntentType::Creative),
            200
        );

        // 推理：1.5x
        assert_eq!(
            calculator.estimate_output_tokens(100, &IntentType::Reasoning),
            150
        );
    }

    #[test]
    fn test_estimate_latency() {
        let calculator = CostCalculator::new();

        // 本地模型延迟较低
        let latency = calculator.estimate_latency(ModelId::Qwen3B, 100);
        assert!(latency < 200);

        // 云端模型延迟较高
        let latency = calculator.estimate_latency(ModelId::GPT4o, 100);
        assert!(latency > 300);
    }

    #[test]
    fn test_total_cost_calculation() {
        let calculator = CostCalculator::new();

        // 本地模型：只有延迟成本
        let cost = calculator.calculate_cost(ModelId::Qwen3B, 100, 50, 100);
        assert!(cost > 0.0);
        assert!(cost < 0.01); // 延迟成本很小

        // 云端模型：API 成本 + 延迟成本
        let cost = calculator.calculate_cost(ModelId::GPT4o, 100, 50, 1000);
        assert!(cost > 0.0);
    }

    #[test]
    fn test_estimate_total_cost() {
        let calculator = CostCalculator::new();

        // 测试完整的成本估算流程
        let cost = calculator.estimate_total_cost(ModelId::GPT4oMini, 1000, &IntentType::Knowledge);

        // 应该包含 API 成本和延迟成本
        assert!(cost > 0.0);
    }

    #[test]
    fn test_local_vs_cloud_cost() {
        let calculator = CostCalculator::new();

        let local_cost =
            calculator.estimate_total_cost(ModelId::Qwen7B, 1000, &IntentType::Knowledge);

        let cloud_cost =
            calculator.estimate_total_cost(ModelId::GPT4oMini, 1000, &IntentType::Knowledge);

        println!("Local cost: ${:.6}", local_cost);
        println!("Cloud cost: ${:.6}", cloud_cost);
        println!("Ratio: {:.2}x", cloud_cost / local_cost);

        // 云端成本应该高于本地（主要是 API 费用）
        assert!(cloud_cost > local_cost);
    }
}
