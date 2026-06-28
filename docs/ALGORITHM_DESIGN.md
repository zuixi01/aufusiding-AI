# Prometheus AI 核心算法设计

> **文档版本**: v1.0
> **编制日期**: 2026-03-04
> **作者**: Prometheus AI Team

本文档详细描述 Prometheus AI 四大核心模块的数学模型、算法公式和 Rust 实现。

---

## 目录

1. [智能路由算法](#1-智能路由算法)
2. [记忆系统算法](#2-记忆系统算法)
3. [Daemon Mind 认知算法](#3-daemon-mind-认知算法)
4. [经济皮层算法](#4-经济皮层算法)

---

## 1. 智能路由算法

### 1.1 问题定义

给定用户输入 `q`，从模型集合 `M = {m₁, m₂, ..., mₙ}` 中选择最优模型 `m*`，使得：

```
m* = argmin_{m ∈ M} Cost(m, q)
     subject to Quality(m, q) ≥ Q_threshold
```

其中：
- `Cost(m, q)`: 使用模型 m 处理查询 q 的成本
- `Quality(m, q)`: 模型 m 对查询 q 的预期质量
- `Q_threshold`: 最低质量阈值

---

### 1.2 意图分类算法

#### 1.2.1 特征提取

对输入 `q` 提取多维特征向量：

```
F(q) = [f_length, f_complexity, f_code, f_creative, f_reasoning, f_context]
```

**特征定义**:

1. **长度特征** `f_length`:
   ```
   f_length = log₁₀(1 + token_count(q))
   ```

2. **复杂度特征** `f_complexity`:
   ```
   f_complexity = α₁·keyword_density + α₂·sentence_depth + α₃·entity_count

   其中:
   keyword_density = |complex_keywords| / |total_words|
   sentence_depth = avg(parse_tree_depth)
   entity_count = |named_entities|

   α₁ = 0.4, α₂ = 0.3, α₃ = 0.3
   ```

3. **代码特征** `f_code`:
   ```
   f_code = sigmoid(code_pattern_score)

   code_pattern_score = Σ w_i · match(pattern_i, q)

   patterns = {
     "```": 3.0,
     "function|class|def": 2.0,
     "import|require": 1.5,
     "=>|->|::": 1.0
   }
   ```

4. **创意特征** `f_creative`:
   ```
   f_creative = creative_verb_ratio + metaphor_score

   creative_verbs = {"想象", "创作", "设计", "构思", ...}
   ```

5. **推理特征** `f_reasoning`:
   ```
   f_reasoning = reasoning_keyword_count / sentence_count

   reasoning_keywords = {"为什么", "如何", "分析", "推导", "证明", ...}
   ```

6. **上下文特征** `f_context`:
   ```
   f_context = min(1.0, history_length / 10)
   ```

#### 1.2.2 意图分类模型

使用轻量级决策树 + 规则混合：

```
Intent(q) = {
  if f_code > 0.6: CODE_GENERATION
  elif f_creative > 0.7: CREATIVE_WRITING
  elif f_reasoning > 0.5 and f_complexity > 0.6: COMPLEX_REASONING
  elif f_length > 2.0: LONG_CONTEXT
  elif contains_tool_keywords(q): TOOL_CALLING
  elif f_complexity < 0.3: SIMPLE_QA
  else: GENERAL_CHAT
}
```

**Rust 实现**:

```rust
#[derive(Debug, Clone)]
pub struct IntentFeatures {
    pub length: f64,
    pub complexity: f64,
    pub code_score: f64,
    pub creative_score: f64,
    pub reasoning_score: f64,
    pub context_score: f64,
}

impl IntentClassifier {
    pub fn extract_features(&self, query: &str, history_len: usize) -> IntentFeatures {
        let tokens = self.tokenize(query);
        let token_count = tokens.len();

        // 长度特征
        let length = (1.0 + token_count as f64).log10();

        // 复杂度特征
        let keyword_density = self.complex_keyword_density(&tokens);
        let sentence_depth = self.avg_parse_depth(query);
        let entity_count = self.count_entities(query);
        let complexity = 0.4 * keyword_density + 0.3 * sentence_depth + 0.3 * entity_count;

        // 代码特征
        let code_score = self.sigmoid(self.code_pattern_score(query));

        // 创意特征
        let creative_score = self.creative_verb_ratio(&tokens) + self.metaphor_score(query);

        // 推理特征
        let reasoning_score = self.reasoning_keyword_count(&tokens) / self.sentence_count(query).max(1) as f64;

        // 上下文特征
        let context_score = (history_len as f64 / 10.0).min(1.0);

        IntentFeatures {
            length,
            complexity,
            code_score,
            creative_score,
            reasoning_score,
            context_score,
        }
    }

    pub fn classify(&self, features: &IntentFeatures) -> Intent {
        if features.code_score > 0.6 {
            Intent::CodeGeneration
        } else if features.creative_score > 0.7 {
            Intent::CreativeWriting
        } else if features.reasoning_score > 0.5 && features.complexity > 0.6 {
            Intent::ComplexReasoning
        } else if features.length > 2.0 {
            Intent::LongContext
        } else if features.complexity < 0.3 {
            Intent::SimpleQA
        } else {
            Intent::GeneralChat
        }
    }

    fn sigmoid(&self, x: f64) -> f64 {
        1.0 / (1.0 + (-x).exp())
    }
}
```

---

### 1.3 模型选择算法

#### 1.3.1 质量预测模型

预测模型 m 对意图 i 的质量得分：

```
Quality(m, i) = β₁·Capability(m, i) + β₂·Historical_Performance(m, i)

其中:
Capability(m, i) = 预定义的模型能力矩阵
Historical_Performance(m, i) = 历史成功率的指数移动平均

EMA_t = α·success_t + (1-α)·EMA_{t-1}
α = 0.2 (平滑系数)
```

**能力矩阵** (0-1 归一化):

| 模型 | 简单问答 | 通用对话 | 代码生成 | 创意写作 | 复杂推理 | 长上下文 |
|------|---------|---------|---------|---------|---------|---------|
| Qwen2.5-3B | 0.85 | 0.70 | 0.60 | 0.50 | 0.40 | 0.30 |
| Qwen2.5-7B | 0.90 | 0.85 | 0.80 | 0.75 | 0.65 | 0.60 |
| Qwen2.5-14B | 0.95 | 0.90 | 0.90 | 0.85 | 0.80 | 0.80 |
| GPT-4o-mini | 0.92 | 0.88 | 0.85 | 0.82 | 0.78 | 0.85 |
| GPT-4o | 0.98 | 0.95 | 0.95 | 0.95 | 0.95 | 0.98 |
| Claude-3.5-Sonnet | 0.97 | 0.96 | 0.96 | 0.98 | 0.97 | 0.99 |

#### 1.3.2 成本计算模型

```
Cost(m, q) = C_api(m) · tokens(q) + C_latency(m) · latency_weight

其中:
C_api(m) = {
  0,                    if m is local
  price_per_1k_tokens,  if m is cloud
}

C_latency(m) = 用户等待成本 (时间价值)

tokens(q) = input_tokens + estimated_output_tokens

estimated_output_tokens = {
  input_tokens * 0.5,  for QA
  input_tokens * 1.0,  for chat
  input_tokens * 2.0,  for code/creative
}
```

**价格表** (USD per 1M tokens):

```rust
const PRICING: &[(ModelId, f64)] = &[
    (ModelId::Qwen3B, 0.0),
    (ModelId::Qwen7B, 0.0),
    (ModelId::Qwen14B, 0.0),
    (ModelId::GPT4oMini, 0.15),
    (ModelId::GPT4o, 2.50),
    (ModelId::Claude35Sonnet, 3.00),
];
```

#### 1.3.3 路由决策算法

**多目标优化**:

```
Score(m, q, i) = w_quality · Quality(m, i)
                 - w_cost · normalize(Cost(m, q))
                 + w_speed · Speed(m)

其中:
w_quality = 0.6  (质量权重)
w_cost = 0.3     (成本权重)
w_speed = 0.1    (速度权重)

normalize(x) = (x - min(x)) / (max(x) - min(x))

Speed(m) = {
  1.0,   if local
  0.5,   if cloud with low latency
  0.3,   if cloud with high latency
}
```

**路由策略**:

```rust
pub enum RoutingStrategy {
    LocalFirst,      // 优先本地，失败才云端
    CostOptimized,   // 成本优先
    QualityFirst,    // 质量优先
    Balanced,        // 平衡模式
    OfflineOnly,     // 仅本地
}

impl Router {
    pub fn select_model(
        &self,
        query: &str,
        intent: Intent,
        strategy: RoutingStrategy,
    ) -> Result<ModelId> {
        let features = self.classifier.extract_features(query, 0);
        let estimated_tokens = self.estimate_tokens(query, &intent);

        let mut candidates: Vec<(ModelId, f64)> = self.models
            .iter()
            .map(|model| {
                let quality = self.predict_quality(model, &intent);
                let cost = self.calculate_cost(model, estimated_tokens);
                let speed = self.get_speed_score(model);

                let score = match strategy {
                    RoutingStrategy::LocalFirst => {
                        if model.is_local() {
                            quality + 10.0  // 本地模型加分
                        } else {
                            quality - cost * 0.5
                        }
                    }
                    RoutingStrategy::CostOptimized => {
                        quality * 0.3 - cost * 0.7
                    }
                    RoutingStrategy::QualityFirst => {
                        quality * 0.9 - cost * 0.1
                    }
                    RoutingStrategy::Balanced => {
                        quality * 0.6 - cost * 0.3 + speed * 0.1
                    }
                    RoutingStrategy::OfflineOnly => {
                        if model.is_local() { quality } else { -1000.0 }
                    }
                };

                (*model, score)
            })
            .collect();

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        Ok(candidates[0].0)
    }

    fn predict_quality(&self, model: &ModelId, intent: &Intent) -> f64 {
        let capability = self.capability_matrix.get(model, intent);
        let historical = self.performance_tracker.get_ema(model, intent);

        0.7 * capability + 0.3 * historical
    }
}
```

---

### 1.4 故障转移与熔断

#### 1.4.1 熔断器算法

使用滑动窗口统计失败率：

```
failure_rate = failures_in_window / total_requests_in_window

State Transition:
CLOSED → OPEN:     if failure_rate > threshold (0.5)
OPEN → HALF_OPEN:  after cooldown_period (30s)
HALF_OPEN → CLOSED: if success_count > min_success (3)
HALF_OPEN → OPEN:   if any failure
```

**Rust 实现**:

```rust
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: Arc<Mutex<CircuitState>>,
    failure_threshold: f64,
    cooldown_duration: Duration,
    window_size: usize,
}

#[derive(Debug)]
struct CircuitState {
    status: BreakerStatus,
    failures: VecDeque<Instant>,
    successes: VecDeque<Instant>,
    last_failure_time: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BreakerStatus {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    pub async fn call<F, T, E>(&self, f: F) -> Result<T, E>
    where
        F: Future<Output = Result<T, E>>,
    {
        // 检查熔断器状态
        {
            let mut state = self.state.lock().await;
            match state.status {
                BreakerStatus::Open => {
                    if let Some(last_failure) = state.last_failure_time {
                        if last_failure.elapsed() > self.cooldown_duration {
                            state.status = BreakerStatus::HalfOpen;
                        } else {
                            return Err(/* CircuitOpen error */);
                        }
                    }
                }
                _ => {}
            }
        }

        // 执行调用
        let result = f.await;

        // 更新状态
        let mut state = self.state.lock().await;
        let now = Instant::now();

        match result {
            Ok(_) => {
                state.successes.push_back(now);
                self.trim_window(&mut state.successes);

                if state.status == BreakerStatus::HalfOpen {
                    if state.successes.len() >= 3 {
                        state.status = BreakerStatus::Closed;
                        state.failures.clear();
                    }
                }
            }
            Err(_) => {
                state.failures.push_back(now);
                state.last_failure_time = Some(now);
                self.trim_window(&mut state.failures);

                let failure_rate = self.calculate_failure_rate(&state);
                if failure_rate > self.failure_threshold {
                    state.status = BreakerStatus::Open;
                }
            }
        }

        result
    }

    fn calculate_failure_rate(&self, state: &CircuitState) -> f64 {
        let total = state.failures.len() + state.successes.len();
        if total == 0 {
            return 0.0;
        }
        state.failures.len() as f64 / total as f64
    }

    fn trim_window(&self, queue: &mut VecDeque<Instant>) {
        while queue.len() > self.window_size {
            queue.pop_front();
        }
    }
}
```

#### 1.4.2 Fallback 策略

```
Fallback Chain: Primary → Secondary → Tertiary → Cache → Error

Example:
GPT-4o (失败) → Claude-3.5 (失败) → Qwen-14B (本地) → 缓存响应 → 错误提示
```

**Rust 实现**:

```rust
pub struct FallbackChain {
    models: Vec<ModelId>,
    circuit_breakers: HashMap<ModelId, CircuitBreaker>,
    cache: Arc<ResponseCache>,
}

impl FallbackChain {
    pub async fn execute(&self, query: &str) -> Result<String> {
        // 尝试主模型链
        for model in &self.models {
            let breaker = self.circuit_breakers.get(model).unwrap();

            match breaker.call(self.call_model(model, query)).await {
                Ok(response) => {
                    self.cache.set(query, &response).await;
                    return Ok(response);
                }
                Err(e) => {
                    tracing::warn!("Model {} failed: {}", model, e);
                    continue;
                }
            }
        }

        // 尝试缓存
        if let Some(cached) = self.cache.get(query).await {
            tracing::info!("Using cached response");
            return Ok(cached);
        }

        // 所有方案失败
        Err(anyhow!("All models failed and no cache available"))
    }
}
```

---

## 2. 记忆系统算法

### 2.1 五层记忆架构

```
L0: 用户画像 (Profile)      - 长期稳定
L1: 工作记忆 (Working)       - 短期活跃
L2: 情景记忆 (Episodic)      - 对话片段
L3: 知识记忆 (Semantic)      - 事实知识
L4: 主动回忆 (Proactive)     - 触发式检索
```

---

### 2.2 工作记忆算法

#### 2.2.1 记忆衰减模型

基于 Ebbinghaus 遗忘曲线：

```
Retention(t) = e^(-t/τ)

其中:
t = 时间间隔 (秒)
τ = 衰减时间常数 (默认 3600s = 1小时)

权重计算:
Weight(memory, t) = Importance(memory) · Retention(t)

Importance(memory) = α·user_explicit + β·interaction_count + γ·recency

α = 0.5 (用户显式标记权重)
β = 0.3 (交互次数权重)
γ = 0.2 (新近度权重)
```

**Rust 实现**:

```rust
#[derive(Debug, Clone)]
pub struct WorkingMemory {
    items: Vec<MemoryItem>,
    capacity: usize,
    decay_constant: f64,  // τ in seconds
}

#[derive(Debug, Clone)]
struct MemoryItem {
    content: String,
    timestamp: Instant,
    importance: f64,
    access_count: usize,
    user_marked: bool,
}

impl WorkingMemory {
    pub fn add(&mut self, content: String, importance: f64) {
        let item = MemoryItem {
            content,
            timestamp: Instant::now(),
            importance,
            access_count: 0,
            user_marked: false,
        };

        self.items.push(item);
        self.evict_if_needed();
    }

    pub fn get_active_context(&self, max_items: usize) -> Vec<String> {
        let mut scored: Vec<(usize, f64)> = self.items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let weight = self.calculate_weight(item);
                (idx, weight)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        scored.iter()
            .take(max_items)
            .map(|(idx, _)| self.items[*idx].content.clone())
            .collect()
    }

    fn calculate_weight(&self, item: &MemoryItem) -> f64 {
        let elapsed = item.timestamp.elapsed().as_secs_f64();
        let retention = (-elapsed / self.decay_constant).exp();

        let importance = 0.5 * if item.user_marked { 1.0 } else { 0.0 }
                       + 0.3 * (item.access_count as f64).ln().max(0.0)
                       + 0.2 * retention;

        importance * retention
    }

    fn evict_if_needed(&mut self) {
        if self.items.len() <= self.capacity {
            return;
        }

        // 移除权重最低的项
        let mut scored: Vec<(usize, f64)> = self.items
            .iter()
            .enumerate()
            .map(|(idx, item)| (idx, self.calculate_weight(item)))
            .collect();

        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let to_remove = self.items.len() - self.capacity;
        for i in 0..to_remove {
            self.items.remove(scored[i].0 - i);
        }
    }
}
```

---

### 2.3 情景记忆算法

#### 2.3.1 对话片段向量化

```
Embedding(episode) = Encoder(summary + context + entities)

其中:
summary = LLM生成的对话摘要
context = 时间、地点、情绪等元信息
entities = 提取的关键实体

向量维度: 768 (使用 bge-small-zh-v1.5)
```

#### 2.3.2 相似度检索

使用余弦相似度 + 时间衰减：

```
Relevance(episode, query) = Similarity(episode, query) · Time_Decay(episode)

Similarity(e, q) = cos(embed(e), embed(q)) = (e·q) / (||e|| ||q||)

Time_Decay(e) = 1 / (1 + log(1 + days_ago))

最终得分:
Score(e, q) = w_sim · Similarity(e, q) + w_time · Time_Decay(e)

w_sim = 0.8
w_time = 0.2
```

**Rust 实现**:


```rust
pub struct IntendModule {
    belief_state: Arc<Mutex<BeliefState>>,
    action_space: Vec<ProactiveAction>,
}

#[derive(Debug, Clone)]
pub enum ProactiveAction {
    SendReminder { content: String, urgency: f64 },
    SuggestTask { task: String, benefit: f64 },
    AskQuestion { question: String },
    UpdateKnowledge { topic: String },
    OptimizeRoutine { area: String },
}

impl IntendModule {
    pub async fn generate_intent(&self) -> Option<ProactiveAction> {
        let belief = self.belief_state.lock().await;
        
        let mut candidates: Vec<(ProactiveAction, f64)> = self.action_space
            .iter()
            .map(|action| {
                let utility = self.calculate_utility(action, &belief);
                (action.clone(), utility)
            })
            .collect();

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // 只有效用 > 阈值才执行
        if candidates[0].1 > 0.5 {
            Some(candidates[0].0.clone())
        } else {
            None
        }
    }

    fn calculate_utility(&self, action: &ProactiveAction, belief: &BeliefState) -> f64 {
        let expected_value = self.estimate_value(action, belief);
        let cost = self.estimate_cost(action);
        let urgency = self.calculate_urgency(action, belief);

        expected_value - cost + urgency * 0.3
    }

    fn estimate_value(&self, action: &ProactiveAction, belief: &BeliefState) -> f64 {
        match action {
            ProactiveAction::SendReminder { urgency, .. } => {
                let p_success = 0.8;  // 提醒成功率
                let benefit = urgency * 2.0;
                p_success * benefit
            }
            ProactiveAction::SuggestTask { benefit, .. } => {
                let p_accept = 0.6;  // 用户接受率
                p_accept * benefit
            }
            _ => 0.5
        }
    }

    fn estimate_cost(&self, action: &ProactiveAction) -> f64 {
        match action {
            ProactiveAction::SendReminder { .. } => 0.1,  // 低成本
            ProactiveAction::UpdateKnowledge { .. } => 0.5,  // 中等成本
            _ => 0.3
        }
    }
}
```

---

### 3.6 好奇模块 (Curious)

#### 3.6.1 信息增益计算

```
Curiosity Score:
C(question) = Information_Gain(question) · Relevance(question)

Information_Gain = H(belief_before) - H(belief_after | answer)

H(belief) = -Σ p_i · log(p_i)  (Shannon Entropy)

Relevance = alignment_with_goals + user_interest
```

**Rust 实现**:

```rust
pub struct CuriousModule {
    knowledge_gaps: Vec<KnowledgeGap>,
    question_history: VecDeque<String>,
}

#[derive(Debug, Clone)]
struct KnowledgeGap {
    topic: String,
    uncertainty: f64,  // 0-1, 越高越不确定
    importance: f64,   // 0-1, 越高越重要
}

impl CuriousModule {
    pub fn generate_question(&self, belief: &BeliefState) -> Option<String> {
        let mut scored_gaps: Vec<(KnowledgeGap, f64)> = self.knowledge_gaps
            .iter()
            .map(|gap| {
                let info_gain = self.estimate_information_gain(gap);
                let relevance = self.calculate_relevance(gap, belief);
                let curiosity = info_gain * relevance;
                (gap.clone(), curiosity)
            })
            .collect();

        scored_gaps.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        if scored_gaps.is_empty() || scored_gaps[0].1 < 0.3 {
            return None;
        }

        Some(self.formulate_question(&scored_gaps[0].0))
    }

    fn estimate_information_gain(&self, gap: &KnowledgeGap) -> f64 {
        // 不确定性越高，信息增益越大
        gap.uncertainty * gap.importance
    }

    fn calculate_relevance(&self, gap: &KnowledgeGap, belief: &BeliefState) -> f64 {
        // 检查是否与用户目标相关
        let goal_alignment = belief.goals.iter()
            .map(|goal| self.topic_similarity(&gap.topic, &goal.description))
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        goal_alignment
    }

    fn formulate_question(&self, gap: &KnowledgeGap) -> String {
        format!("我注意到关于 {} 的信息还不完整，能告诉我更多吗？", gap.topic)
    }
}
```

---

## 4. 经济皮层算法

### 4.1 成本追踪模型

#### 4.1.1 多维成本计算

```
Total_Cost = C_api + C_compute + C_storage + C_attention + C_energy

C_api = Σ (tokens_i · price_per_token_i)  // API 调用成本

C_compute = Σ (flops_i · cost_per_flop)   // 本地算力成本

C_storage = data_size · price_per_gb · days  // 存储成本

C_attention = user_time · value_of_time   // 用户注意力成本

C_energy = kwh · electricity_price        // 电力成本
```

**成本参数**:

```rust
pub struct CostParameters {
    // API 价格 (USD per 1M tokens)
    pub gpt4o_input: f64,      // 2.50
    pub gpt4o_output: f64,     // 10.00
    pub claude_input: f64,     // 3.00
    pub claude_output: f64,    // 15.00
    
    // 本地算力成本 (USD per TFLOP)
    pub local_compute: f64,    // 0.0001
    
    // 存储成本 (USD per GB per month)
    pub storage: f64,          // 0.02
    
    // 用户时间价值 (USD per hour)
    pub user_time: f64,        // 20.0
    
    // 电力成本 (USD per kWh)
    pub electricity: f64,      // 0.12
}
```

#### 4.1.2 实时成本追踪

**Rust 实现**:

```rust
#[derive(Debug, Clone)]
pub struct EconomicsLayer {
    cost_tracker: Arc<Mutex<CostTracker>>,
    parameters: CostParameters,
    budget: Budget,
}

#[derive(Debug, Default)]
struct CostTracker {
    api_costs: HashMap<ModelId, f64>,
    compute_costs: f64,
    storage_costs: f64,
    total_tokens: u64,
    total_requests: u64,
}

#[derive(Debug, Clone)]
struct Budget {
    daily_limit: f64,
    monthly_limit: f64,
    current_daily: f64,
    current_monthly: f64,
}

impl EconomicsLayer {
    pub async fn track_api_call(
        &self,
        model: ModelId,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<f64> {
        let cost = self.calculate_api_cost(model, input_tokens, output_tokens);
        
        let mut tracker = self.cost_tracker.lock().await;
        *tracker.api_costs.entry(model).or_insert(0.0) += cost;
        tracker.total_tokens += input_tokens + output_tokens;
        tracker.total_requests += 1;

        // 检查预算
        self.check_budget(cost).await?;

        Ok(cost)
    }

    fn calculate_api_cost(
        &self,
        model: ModelId,
        input_tokens: u64,
        output_tokens: u64,
    ) -> f64 {
        let (input_price, output_price) = match model {
            ModelId::GPT4o => (self.parameters.gpt4o_input, self.parameters.gpt4o_output),
            ModelId::Claude35Sonnet => (self.parameters.claude_input, self.parameters.claude_output),
            _ => (0.0, 0.0),  // 本地模型免费
        };

        (input_tokens as f64 * input_price + output_tokens as f64 * output_price) / 1_000_000.0
    }

    async fn check_budget(&self, cost: f64) -> Result<()> {
        let mut budget = self.budget.clone();
        budget.current_daily += cost;
        budget.current_monthly += cost;

        if budget.current_daily > budget.daily_limit {
            return Err(anyhow!("Daily budget exceeded: ${:.2}", budget.current_daily));
        }

        if budget.current_monthly > budget.monthly_limit {
            return Err(anyhow!("Monthly budget exceeded: ${:.2}", budget.current_monthly));
        }

        Ok(())
    }

    pub async fn get_cost_report(&self) -> CostReport {
        let tracker = self.cost_tracker.lock().await;
        
        let total_api_cost: f64 = tracker.api_costs.values().sum();
        let total_cost = total_api_cost + tracker.compute_costs + tracker.storage_costs;

        CostReport {
            total_cost,
            api_costs: tracker.api_costs.clone(),
            compute_costs: tracker.compute_costs,
            storage_costs: tracker.storage_costs,
            total_tokens: tracker.total_tokens,
            total_requests: tracker.total_requests,
            cost_per_request: total_cost / tracker.total_requests.max(1) as f64,
        }
    }
}
```

---

### 4.2 价值量化模型

#### 4.2.1 ROI 计算

```
ROI = (Value_Generated - Cost_Incurred) / Cost_Incurred

Value_Generated = Σ value_i

value_i = {
  time_saved · hourly_rate,           // 节省时间
  quality_improvement · task_value,   // 质量提升
  knowledge_gained · learning_value,  // 知识获取
  automation_benefit · frequency,     // 自动化收益
}
```

**Rust 实现**:

```rust
pub struct ValueTracker {
    interactions: Vec<InteractionValue>,
    user_hourly_rate: f64,
}

#[derive(Debug, Clone)]
struct InteractionValue {
    timestamp: DateTime<Utc>,
    time_saved: Duration,
    quality_score: f64,
    knowledge_gained: f64,
    cost: f64,
}

impl ValueTracker {
    pub fn calculate_roi(&self, period: Duration) -> f64 {
        let cutoff = Utc::now() - chrono::Duration::from_std(period).unwrap();
        
        let recent: Vec<&InteractionValue> = self.interactions
            .iter()
            .filter(|i| i.timestamp > cutoff)
            .collect();

        if recent.is_empty() {
            return 0.0;
        }

        let total_value: f64 = recent.iter()
            .map(|i| self.calculate_value(i))
            .sum();

        let total_cost: f64 = recent.iter()
            .map(|i| i.cost)
            .sum();

        if total_cost < 0.01 {
            return f64::INFINITY;
        }

        (total_value - total_cost) / total_cost
    }

    fn calculate_value(&self, interaction: &InteractionValue) -> f64 {
        // 时间价值
        let time_value = interaction.time_saved.as_secs_f64() / 3600.0 * self.user_hourly_rate;

        // 质量价值
        let quality_value = interaction.quality_score * 10.0;

        // 知识价值
        let knowledge_value = interaction.knowledge_gained * 5.0;

        time_value + quality_value + knowledge_value
    }

    pub fn get_value_dashboard(&self) -> ValueDashboard {
        let roi_7d = self.calculate_roi(Duration::from_secs(7 * 86400));
        let roi_30d = self.calculate_roi(Duration::from_secs(30 * 86400));

        ValueDashboard {
            roi_7d,
            roi_30d,
            total_time_saved: self.total_time_saved(),
            total_cost: self.total_cost(),
            value_per_dollar: roi_30d + 1.0,
        }
    }
}
```

---

### 4.3 成本优化算法

#### 4.3.1 动态路由优化

```
Optimization Goal:
minimize: E[Cost]
subject to: E[Quality] ≥ Q_min

使用强化学习优化路由策略:

Q(state, action) = immediate_reward + γ · max_a' Q(state', a')

reward = quality_achieved - cost_incurred

state = (intent, history, budget_remaining)
action = model_selection
```

**Rust 实现**:

```rust
pub struct CostOptimizer {
    q_table: HashMap<(Intent, ModelId), f64>,
    learning_rate: f64,
    discount_factor: f64,
    exploration_rate: f64,
}

impl CostOptimizer {
    pub fn select_optimal_model(
        &mut self,
        intent: Intent,
        budget_remaining: f64,
    ) -> ModelId {
        // ε-greedy 策略
        if rand::random::<f64>() < self.exploration_rate {
            // 探索：随机选择
            self.random_model()
        } else {
            // 利用：选择最优
            self.best_model_for_intent(&intent, budget_remaining)
        }
    }

    pub fn update_q_value(
        &mut self,
        state: Intent,
        action: ModelId,
        reward: f64,
        next_state: Intent,
    ) {
        let current_q = self.q_table.get(&(state.clone(), action)).copied().unwrap_or(0.0);
        
        let max_next_q = self.models()
            .iter()
            .map(|m| self.q_table.get(&(next_state.clone(), *m)).copied().unwrap_or(0.0))
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let new_q = current_q + self.learning_rate * (
            reward + self.discount_factor * max_next_q - current_q
        );

        self.q_table.insert((state, action), new_q);
    }

    fn calculate_reward(&self, quality: f64, cost: f64) -> f64 {
        // 归一化质量和成本
        let normalized_quality = quality;  // 已经是 0-1
        let normalized_cost = (cost / 0.01).min(1.0);  // 假设 $0.01 为参考成本

        // 奖励函数：质量高、成本低为正奖励
        normalized_quality - normalized_cost
    }
}
```

---

### 4.4 预算管理算法

#### 4.4.1 自适应预算分配

```
Budget Allocation:
B_daily = B_monthly / days_in_month

动态调整:
B_adjusted = B_daily · (1 + α · (value_yesterday - cost_yesterday) / cost_yesterday)

α = 0.1 (调整系数)

限制:
B_min ≤ B_adjusted ≤ B_max
```

**Rust 实现**:

```rust
pub struct BudgetManager {
    monthly_budget: f64,
    daily_budgets: VecDeque<f64>,
    value_history: VecDeque<f64>,
    cost_history: VecDeque<f64>,
}

impl BudgetManager {
    pub fn calculate_daily_budget(&mut self) -> f64 {
        let days_in_month = 30;
        let base_daily = self.monthly_budget / days_in_month as f64;

        if self.cost_history.len() < 2 {
            return base_daily;
        }

        // 计算昨日 ROI
        let yesterday_value = self.value_history.back().copied().unwrap_or(0.0);
        let yesterday_cost = self.cost_history.back().copied().unwrap_or(0.01);
        let roi = (yesterday_value - yesterday_cost) / yesterday_cost;

        // 动态调整
        let alpha = 0.1;
        let adjusted = base_daily * (1.0 + alpha * roi);

        // 限制范围
        let min_budget = base_daily * 0.5;
        let max_budget = base_daily * 2.0;

        adjusted.clamp(min_budget, max_budget)
    }

    pub fn should_throttle(&self, current_cost: f64, daily_budget: f64) -> bool {
        let hour = chrono::Local::now().hour();
        let expected_cost_by_now = daily_budget * (hour as f64 / 24.0);

        current_cost > expected_cost_by_now * 1.2  // 超出 20% 则限流
    }
}
```

---

## 5. 算法集成与协同

### 5.1 模块协同工作流

```
User Input
    ↓
[Intent Classification] → features
    ↓
[Router] → model selection (考虑成本)
    ↓
[Model Execution]
    ↓
[Memory Update] → 存储对话
    ↓
[Economics Tracking] → 记录成本
    ↓
[Daemon Mind] → 后台分析
    ↓
[Proactive Action] → 主动建议
```

### 5.2 完整系统实现

```rust
pub struct PrometheusCore {
    router: Router,
    memory: MemorySystem,
    daemon_mind: DaemonMind,
    economics: EconomicsLayer,
}

impl PrometheusCore {
    pub async fn process_message(&mut self, user_input: &str) -> Result<String> {
        // 1. 意图分类
        let intent = self.router.classify(user_input).await?;

        // 2. 检索相关记忆
        let context = self.memory.retrieve_context(user_input, 5).await?;

        // 3. 选择模型（考虑成本）
        let model = self.router.select_model(&intent, &context).await?;

        // 4. 检查预算
        self.economics.check_budget_before_call(&model, user_input).await?;

        // 5. 执行推理
        let response = self.execute_inference(&model, user_input, &context).await?;

        // 6. 追踪成本
        let cost = self.economics.track_call(&model, user_input, &response).await?;

        // 7. 更新记忆
        self.memory.add_interaction(user_input, &response).await?;

        // 8. Daemon Mind 分析
        self.daemon_mind.analyze_interaction(user_input, &response, cost).await?;

        // 9. 生成主动意图（异步）
        tokio::spawn({
            let daemon = self.daemon_mind.clone();
            async move {
                if let Some(intent) = daemon.generate_proactive_intent().await {
                    daemon.execute_intent(intent).await.ok();
                }
            }
        });

        Ok(response)
    }
}
```

---

## 6. 性能指标与优化

### 6.1 关键性能指标

| 指标 | 目标值 | 测量方法 |
|------|--------|----------|
| 意图分类延迟 | < 1ms | 平均响应时间 |
| 路由决策延迟 | < 5ms | 平均决策时间 |
| 向量检索延迟 | < 50ms | P95 延迟 |
| 本地推理首 token | < 100ms | P95 延迟 |
| 记忆更新延迟 | < 10ms | 平均写入时间 |
| 月均成本 | < $3 | 实际花费统计 |
| 本地处理占比 | > 80% | 请求数统计 |

### 6.2 算法复杂度分析

```
意图分类: O(n)  n = 输入长度
路由决策: O(m)  m = 模型数量
向量检索: O(log N + k)  N = 向量总数, k = top-k
记忆衰减: O(C)  C = 工作记忆容量
PBRIC 循环: O(A)  A = 动作空间大小
```

---

## 7. 总结

本文档详细设计了 Prometheus AI 四大核心模块的算法：

1. **智能路由**: 多维特征分类 + 多目标优化 + 熔断降级
2. **记忆系统**: 五层架构 + 向量检索 + 时间衰减
3. **Daemon Mind**: PBRIC 认知循环 + 贝叶斯信念更新 + 主动意图生成
4. **经济皮层**: 多维成本追踪 + ROI 量化 + 强化学习优化

所有算法均提供：
- 数学公式和理论基础
- 完整的 Rust 实现代码
- 性能指标和优化建议

这些算法将支撑 Prometheus 实现"本地优先、成本可控、主动智能"的核心目标。

---

**文档维护**: 随着实现和测试，算法参数将持续调优。

**下一步**: 将这些算法集成到实际代码中，并进行 A/B 测试验证效果。
