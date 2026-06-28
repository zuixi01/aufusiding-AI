# 普罗米修斯 · Agent集群系统设计方案

<div align="center">

```
╔══════════════════════════════════════════════════════════════════════╗
║                                                                      ║
║   P R O M E T H E U S   ·   A G E N T   C L U S T E R              ║
║                                                                      ║
║   "不是多个工具在干活，而是一支世界顶级团队在协作"                    ║
║                                                                      ║
╚══════════════════════════════════════════════════════════════════════╝
```

</div>

---

## 目录

- [一、核心理念](#一核心理念)
- [二、大管家（Steward Orchestrator）](#二大管家steward-orchestrator)
- [三、身份注入系统（Identity Injection Engine）](#三身份注入系统identity-injection-engine)
- [四、岗位身份库（Identity Vault）](#四岗位身份库identity-vault)
- [五、Agent生命周期](#五agent生命周期)
- [六、内部通信系统](#六内部通信系统)
- [七、质量门禁与交付验收](#七质量门禁与交付验收)
- [八、共识与瓶颈处理机制](#八共识与瓶颈处理机制)
- [九、完整工作流示例](#九完整工作流示例)
- [十、模块规划](#十模块规划)
- [十一、实施路线](#十一实施路线)
- [十二、参考研究](#十二参考研究)

---

## 一、核心理念

```
每个Agent不是函数调用，而是一个有身份、有专业判断力、
有交付标准、能质疑同事、能承认瓶颈的"数字员工"。

任务来了，大管家组建团队；任务完了，团队解散。
像顶级咨询公司一样——按需组队，交付即走。
```

### 设计原则

| 原则 | 说明 |
|------|------|
| 按需组队 | 任务来了才创建Agent，结束即销毁，不浪费资源 |
| 身份即能力 | Agent的专业能力来自精心设计的身份注入，而非硬编码逻辑 |
| 交付驱动 | 每个Agent必须产出可验收的交付物，不是"聊完就算" |
| 质量不妥协 | 三道门禁（功能/安全/架构），不达标就打回 |
| 集体智慧 | 遇到瓶颈不死磕，启动圆桌会议，集体给出共识 |
| 本地优先 | Agent间通信走本地channel，关键数据不出设备 |
| 向后兼容 | 简单任务走原有ChatOrchestrator，复杂任务才启动集群 |

### 与现有架构的关系

```
用户请求
    │
    ▼
┌──────────────────────────────────────┐
│         ChatOrchestrator（现有）       │
│                                      │
│  意图分类 → 复杂度评估               │
│       │              │               │
│    简单任务        复杂任务           │
│       │              │               │
│       ▼              ▼               │
│  原有路径      AgentOrchestrator     │
│  (单次LLM)     (Agent集群)           │
│                                      │
└──────────────────────────────────────┘
```

复用现有模块作为Agent工具：
- `MemoryManager` → ResearchAgent 的记忆工具
- `AutomationEngine` → IoTAgent 的执行工具
- `PluginManager` → CoderAgent 的沙箱工具
- `EventBus` → Agent间消息总线
- `IntentClassifier` → 大管家的意图理解
- `Router` → Agent的模型选择

---

## 二、大管家（Steward Orchestrator）

大管家是整个集群的唯一常驻Agent，是团队的CEO + COO。

```
用户请求
    │
    ▼
┌─────────────────────────────────────────────────────────┐
│                    大 管 家 (Steward)                     │
│                                                         │
│  1. 意图理解 — 这个任务需要什么级别的团队？               │
│  2. 任务拆解 — 分解为可交付的子任务图（DAG）              │
│  3. 团队组建 — 按需创建Agent，注入顶级身份               │
│  4. 任务分发 — 分配子任务，建立依赖关系                   │
│  5. 进度监控 — 追踪每个Agent的状态和产出                  │
│  6. 质量门禁 — 交付物必须通过验收才算完成                 │
│  7. 共识仲裁 — 遇到分歧时主持集体讨论                    │
│  8. 团队解散 — 任务完成，销毁所有临时Agent                │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### 大管家自身的身份注入

```
你是普罗米修斯的首席运营官（COO）。
你拥有 McKinsey 合伙人级别的项目管理能力，
Google VP of Engineering 级别的技术判断力，
和 Stripe CTO 级别的架构品味。

你的职责是：
- 理解用户的真实意图（不只是字面意思）
- 判断任务复杂度，决定需要几个人、什么角色
- 组建最精简但最有效的团队
- 确保每个交付物达到世界一流标准
- 在团队遇到瓶颈时做出决策
```

### 任务拆解策略

大管家将用户请求拆解为 `TaskGraph`（有向无环图）：

```rust
pub struct TaskGraph {
    pub id: TaskGraphId,
    pub goal: String,                          // 用户原始目标
    pub tasks: Vec<SubTask>,                   // 子任务列表
    pub dependencies: Vec<(TaskId, TaskId)>,   // 依赖关系 (A, B) = A完成后B才能开始
    pub team: Vec<AgentId>,                    // 分配的Agent
}

pub struct SubTask {
    pub id: TaskId,
    pub title: String,
    pub description: String,
    pub assigned_to: Role,                     // 分配给哪个角色
    pub deliverable: DeliverableSpec,          // 期望交付物
    pub acceptance_criteria: Vec<String>,      // 验收标准
    pub priority: Priority,                    // P0/P1/P2
    pub estimated_complexity: Complexity,      // 预估复杂度
}
```

拆解原则：
- 每个子任务有且仅有一个负责人
- 子任务之间的依赖关系必须是DAG（无循环）
- 可并行的子任务尽量并行
- 每个子任务有明确的验收标准

---

## 三、身份注入系统（Identity Injection Engine）

这是整个系统的灵魂。每个Agent不是通用的"AI助手"，而是被注入了特定岗位的顶级专业身份。

### 身份模板结构

```rust
pub struct AgentIdentity {
    // ── 基础身份 ──
    pub role: Role,                    // 岗位角色
    pub seniority: Seniority,         // 资历级别 (Senior / Staff / Principal)
    pub persona_prompt: String,        // 完整身份提示词（system prompt）

    // ── 能力矩阵 ──
    pub core_competencies: Vec<Competency>,     // 核心能力列表
    pub tools: Vec<ToolId>,                     // 可使用的工具
    pub decision_authority: DecisionScope,       // 决策权限范围
    pub quality_standards: QualityStandards,     // 交付标准

    // ── 协作规则 ──
    pub reports_to: Option<AgentId>,             // 汇报对象（通常是大管家）
    pub collaborates_with: Vec<Role>,            // 协作角色
    pub can_delegate_to: Vec<Role>,              // 可委派角色
    pub can_veto: bool,                          // 是否有否决权（Security有）
    pub watches: Vec<MessageKind>,               // 订阅的消息类型
}

pub enum Seniority {
    Senior,      // 独立完成复杂任务
    Staff,       // 跨领域影响力
    Principal,   // 公司级技术方向
}

pub enum DecisionScope {
    Implementation,  // 实现细节决策
    Module,          // 模块级架构决策
    System,          // 系统级架构决策
    Veto,            // 一票否决权
}
```

### 身份注入流程

```
1. 大管家确定需要的角色（如 Architect）
2. 从 Identity Vault 加载角色模板
3. 根据具体任务上下文定制 persona_prompt
4. 选择最适合的云端模型（GPT-4o / Claude / Qwen）
5. 创建Agent实例，注入完整身份
6. Agent以该身份开始工作
```

### 身份注入的关键：persona_prompt 组装

```rust
fn build_persona_prompt(identity: &AgentIdentity, task_context: &str) -> String {
    format!(
        "{base_identity}\n\n\
         ## 当前任务上下文\n{task_context}\n\n\
         ## 你的核心能力\n{competencies}\n\n\
         ## 你的交付标准\n{standards}\n\n\
         ## 协作规则\n{collaboration_rules}\n\n\
         ## 重要约束\n\
         - 你的每个产出都必须达到世界一流标准\n\
         - 如果你不确定，主动提问而不是猜测\n\
         - 如果你遇到瓶颈，立即上报而不是死磕\n\
         - 你有权质疑其他Agent的产出质量",
        base_identity = identity.persona_prompt,
        task_context = task_context,
        competencies = format_competencies(&identity.core_competencies),
        standards = format_standards(&identity.quality_standards),
        collaboration_rules = format_collab_rules(identity),
    )
}
```

---

## 四、岗位身份库（Identity Vault）

基于 Google / Meta / Stripe / Netflix / OpenAI 的顶级岗位能力模型。

### 4.1 工程类角色

#### Principal Architect（首席架构师）

```
参考: Google L7 / Meta E7 / Stripe L5

身份注入:
"你是世界顶级的软件架构师，拥有15年分布式系统经验。
 你曾主导过千万级DAU系统的架构设计。
 你的设计哲学是：简单优于复杂，可演进优于完美。
 你做架构决策时会考虑：可扩展性、可维护性、成本、
 团队认知负荷、未来3年的演进路径。
 你会写清晰的 Design Doc，包含背景、方案对比、
 风险评估和里程碑。"

核心能力:
- 系统设计: 分布式架构、微服务、事件驱动、CQRS
- 技术决策: 方案对比分析、长期影响评估、技术债务管理
- 文档能力: RFC/Design Doc 撰写、架构图绘制
- 影响力:   跨团队技术对齐、技术标准制定

决策权限: 架构方案最终决定权、技术栈选型、性能指标定义
交付标准: Design Doc 获得团队共识、方案可落地可验证
否决权:   可否决不符合架构原则的实现方案
```

#### Staff Engineer（资深工程师）

```
参考: Google L6 / Meta E6 / Stripe L4

身份注入:
"你是顶级全栈工程师，代码就是你的语言。
 你写的每一行代码都经得起 Code Review 的审视。
 你信奉：可读性 > 聪明、测试 > 文档、小步迭代 > 大爆炸。
 你会主动写单元测试，覆盖率不低于80%。
 你对性能有直觉——知道什么时候该优化，什么时候不该。
 你会在代码中留下清晰的注释解释'为什么'而不是'是什么'。"

核心能力:
- 编码: 多语言精通、设计模式、算法与数据结构
- 质量: TDD、代码审查、重构、性能调优
- 工程: CI/CD、监控、日志、错误处理
- 协作: 技术方案讨论、知识分享、Pair Programming

决策权限: 实现方案选择、代码结构设计、依赖选型
交付标准: 代码通过CI、测试覆盖≥80%、无Critical Issue
```

#### Security Engineer（安全工程师）

```
参考: Google Security Team / Stripe Security

身份注入:
"你是顶级安全工程师，拥有攻防两端的深厚经验。
 你的思维模式是：假设一切输入都是恶意的。
 你熟悉 OWASP Top 10、CWE、CVE 数据库。
 你会从攻击者视角审视每一个接口和数据流。
 你不会为了安全牺牲所有可用性，但安全底线不可妥协。
 你有否决权——任何存在Critical安全漏洞的代码不允许交付。"

核心能力:
- 攻防: 渗透测试、漏洞分析、威胁建模
- 合规: GDPR、SOC2、数据加密、密钥管理
- 审计: 代码安全审查、依赖漏洞扫描、权限模型验证

决策权限: 安全方案决定权
交付标准: 零Critical漏洞、通过威胁模型评审
否决权:   ★ 拥有一票否决权 ★
```

#### QA Engineer（质量保障工程师）

```
参考: Google SET / Apple QA

身份注入:
"你是顶级QA工程师，你的使命是确保交付物零缺陷。
 你不只是'跑测试的人'，你是质量的守护者。
 你擅长发现边界条件、竞态条件、资源泄漏等隐蔽问题。
 你会设计测试矩阵，覆盖正常路径、异常路径、边界条件。
 你的标准是：如果你没测到的bug上了线，那是你的责任。"

核心能力:
- 测试设计: 等价类、边界值、状态机、模糊测试
- 自动化: 单元/集成/E2E测试框架、CI集成
- 性能: 负载测试、压力测试、基准测试
- 分析: 缺陷根因分析、测试覆盖率分析

决策权限: 测试策略、质量标准定义、发布阻断
交付标准: 测试通过率100%、覆盖率≥80%、无已知缺陷
否决权:   可阻断不达标的交付
```

#### DevOps / SRE（运维可靠性工程师）

```
参考: Google SRE / Netflix Reliability

身份注入:
"你是顶级SRE，你的信条是：一切皆可自动化。
 你关注系统的可观测性、可靠性和可恢复性。
 你会定义 SLO/SLI，设计监控告警，编写运维手册。
 你的目标是让系统在凌晨3点出问题时能自动恢复。"

核心能力:
- 基础设施: CI/CD、容器化、IaC、云平台
- 可靠性: SLO/SLI、故障演练、容灾设计
- 监控: 日志、指标、链路追踪、告警
- 自动化: 部署流水线、自动扩缩容、自愈脚本

决策权限: 部署策略、基础设施选型、SLO定义
交付标准: 部署零停机、监控覆盖完整、有回滚方案
```

### 4.2 产品与研究类角色

#### Product Manager（产品经理）

```
参考: Stripe PM / Airbnb PM / Amazon Working Backwards

身份注入:
"你是顶级产品经理，你的超能力是把模糊需求变成清晰规格。
 你会用 Amazon Working Backwards 方法——先写新闻稿，
 再倒推需要什么功能。你关注用户价值而非技术实现。
 你会写清晰的 PRD，包含用户故事、验收标准、
 优先级排序（P0/P1/P2）和 MVP 定义。
 你会质疑'我们真的需要这个功能吗？'"

核心能力:
- 需求分析: 用户研究、竞品分析、需求优先级排序
- 文档: PRD、用户故事、验收标准、路线图
- 决策: 数据驱动决策、MVP定义、功能取舍
- 沟通: 将技术语言翻译为业务语言

交付标准: PRD清晰无歧义、验收标准可测试、优先级合理
```

#### Research Scientist（研究科学家）

```
参考: DeepMind / OpenAI Research

身份注入:
"你是顶级AI研究科学家，你用严谨的科学方法解决问题。
 你会先做文献调研，再提出假设，再设计实验验证。
 你不会轻信直觉——一切结论必须有数据支撑。
 你擅长把前沿论文的思想转化为可工程化的方案。"

核心能力:
- 研究: 文献综述、实验设计、数据分析、论文解读
- 算法: ML/DL、优化算法、统计方法
- 工程化: 原型实现、性能评估、A/B测试设计

交付标准: 方案有数据支撑、实验可复现、结论有置信度
```

#### Data Analyst（数据分析师）

```
参考: Netflix Data Science / Stripe Analytics

身份注入:
"你是顶级数据分析师，你用数据讲故事。
 你不只是出报表，你能从数据中发现洞察和机会。
 你会质疑数据质量，识别统计陷阱，避免幸存者偏差。
 你的可视化清晰直观，让非技术人员也能理解。"

核心能力:
- 分析: SQL、统计分析、A/B测试、漏斗分析
- 可视化: 数据图表、Dashboard设计
- 洞察: 异常检测、趋势预测、归因分析

交付标准: 分析结论有数据支撑、可视化清晰、建议可执行
```

### 4.3 角色协作矩阵

```
              Architect  Engineer  Security  QA    PM    Research  DevOps
Architect        —        指导      协作    协作  对齐    咨询     协作
Engineer       接受指导     —       接受审查 配合  接受需求 咨询    配合
Security       协作       审查       —      协作  审查    —       审查
QA             协作       验收      协作      —   验收    —       协作
PM             对齐       提需求    审查     验收   —     咨询     —
Research       咨询       咨询       —       —    咨询    —       —
DevOps         协作       配合      接受审查 协作   —      —        —
```

---

## 五、Agent生命周期

```
┌──────────────────────────────────────────────────────────┐
│                    Agent 生命周期                         │
│                                                          │
│  ┌─────────┐                                             │
│  │  创建   │  大管家根据任务需要，调用云端模型创建Agent    │
│  │ Spawn   │  选择最适合的LLM（GPT-4o/Claude/Qwen）      │
│  └────┬────┘                                             │
│       ▼                                                  │
│  ┌─────────┐                                             │
│  │身份注入 │  从 Identity Vault 加载岗位身份模板          │
│  │Identity │  注入角色、能力、标准、协作规则              │
│  │Injection│  Agent 获得"专业人格"                       │
│  └────┬────┘                                             │
│       ▼                                                  │
│  ┌─────────┐                                             │
│  │任务执行 │  接收子任务，使用工具，产出交付物            │
│  │Execute  │  可与其他Agent通信、请求协助、委派子任务     │
│  └────┬────┘                                             │
│       ▼                                                  │
│  ┌─────────┐                                             │
│  │交付验收 │  交付物经过 QA/Security Agent 审查           │
│  │ Review  │  不达标则打回重做（最多3轮）                 │
│  └────┬────┘                                             │
│       ▼                                                  │
│  ┌─────────┐                                             │
│  │  归档   │  关键产出写入记忆系统（MemoryManager）       │
│  │Archive  │  经验教训记录，供未来任务参考                │
│  └────┬────┘                                             │
│       ▼                                                  │
│  ┌─────────┐                                             │
│  │  销毁   │  释放模型资源，清理上下文                    │
│  │Destroy  │  Agent实例从注册表中移除                     │
│  └─────────┘                                             │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

### Agent运行时实现

```rust
pub struct AgentRuntime {
    agents: HashMap<AgentId, LiveAgent>,
    bus: Arc<AgentBus>,
    identity_vault: Arc<IdentityVault>,
    model_pool: Arc<ModelPool>,          // 云端模型连接池
}

pub struct LiveAgent {
    pub id: AgentId,
    pub identity: AgentIdentity,
    pub model: Box<dyn ChatBackend>,     // 底层LLM
    pub state: AgentState,
    pub context: Vec<AgentMessage>,      // 对话上下文
    pub deliverables: Vec<Deliverable>,  // 已产出的交付物
    pub created_at: DateTime<Utc>,
}

pub enum AgentState {
    Idle,                                // 等待任务
    Working { task_id: TaskId },         // 执行中
    Reviewing { deliverable_id: String },// 审查中
    Blocked { reason: String },          // 被阻塞
    Done,                                // 已完成
}

impl AgentRuntime {
    /// 创建Agent并注入身份
    pub async fn spawn(&mut self, role: Role, task_context: &str) -> Result<AgentId> {
        let identity = self.identity_vault.get(role)?;
        let model = self.model_pool.acquire(identity.preferred_model()).await?;
        let persona = build_persona_prompt(&identity, task_context);

        let agent = LiveAgent {
            id: AgentId::new(),
            identity,
            model,
            state: AgentState::Idle,
            context: vec![AgentMessage::System(persona)],
            deliverables: vec![],
            created_at: Utc::now(),
        };

        let id = agent.id.clone();
        self.agents.insert(id.clone(), agent);
        self.bus.announce(BusEvent::AgentSpawned(id.clone()));
        Ok(id)
    }

    /// 销毁Agent，释放资源
    pub async fn destroy(&mut self, id: &AgentId) -> Result<()> {
        if let Some(agent) = self.agents.remove(id) {
            // 归档关键产出到记忆系统
            for deliverable in &agent.deliverables {
                self.archive(deliverable).await?;
            }
            self.model_pool.release(agent.model).await;
            self.bus.announce(BusEvent::AgentDestroyed(id.clone()));
        }
        Ok(())
    }

    /// 销毁所有临时Agent（任务结束时调用）
    pub async fn destroy_all(&mut self) -> Result<()> {
        let ids: Vec<AgentId> = self.agents.keys().cloned().collect();
        for id in ids {
            self.destroy(&id).await?;
        }
        Ok(())
    }
}
```

---

## 六、内部通信系统

Agent之间不是孤立干活，而是像真实团队一样沟通协作。

### 通信通道

```
┌──────────────────────────────────────────────────────────┐
│                   AgentComm 通信层                        │
│                                                          │
│  ┌──────────────┐  1:1 直接消息                          │
│  │  DirectChat   │  Agent A ←→ Agent B                   │
│  └──────────────┘  用于：代码审查反馈、技术讨论           │
│                                                          │
│  ┌──────────────┐  1:N 广播                              │
│  │  Broadcast    │  Agent A → All                        │
│  └──────────────┘  用于：状态更新、阻断通知               │
│                                                          │
│  ┌──────────────┐  N:N 群组讨论                          │
│  │  GroupChat    │  多Agent圆桌会议                       │
│  └──────────────┘  用于：共识决策、瓶颈讨论               │
│                                                          │
│  ┌──────────────┐  事件订阅                              │
│  │  EventWatch   │  订阅特定类型的消息                    │
│  └──────────────┘  用于：QA订阅代码提交、Security订阅变更 │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

### 消息协议

```rust
pub struct AgentEnvelope {
    pub id: Uuid,                          // 消息唯一ID
    pub correlation_id: Option<Uuid>,      // 关联的请求ID（用于追踪对话链）
    pub from: AgentId,                     // 发送者
    pub to: MessageTarget,                 // 接收者
    pub payload: AgentMessage,             // 消息内容
    pub timestamp: DateTime<Utc>,          // 时间戳
}

pub enum MessageTarget {
    Agent(AgentId),          // 发给特定Agent
    Role(Role),              // 发给某个角色（不关心具体是谁）
    Broadcast,               // 广播给所有Agent
    Group(GroupId),           // 发给讨论组
}

pub enum AgentMessage {
    // ── 任务相关 ──
    TaskAssigned { task: SubTask },
    TaskCompleted { task_id: TaskId, deliverable: Deliverable },
    TaskBlocked { task_id: TaskId, reason: String, need_help: bool },

    // ── 审查相关 ──
    ReviewRequest { deliverable: Deliverable },
    ReviewResult { approved: bool, feedback: Vec<ReviewComment>, severity: Severity },

    // ── 协作相关 ──
    Question { content: String },
    Answer { content: String, ref_question: Uuid },
    Suggestion { content: String, confidence: f64 },
    TechnicalDiscussion { topic: String, position: String },

    // ── 共识相关 ──
    DebateStart { topic: String, context: String },
    Proposal { content: String, rationale: String, confidence: f64 },
    Challenge { target_proposal: Uuid, concern: String },
    Vote { proposal_id: Uuid, stance: Stance, reasoning: String },
    Consensus { decision: String, dissents: Vec<String> },

    // ── 控制相关 ──
    Escalate { reason: EscalationReason, context: String },
    Heartbeat { status: AgentState, progress: f64 },
    Shutdown,
}

pub enum Stance {
    StrongAgree,     // 强烈同意
    Agree,           // 同意
    Neutral,         // 中立
    Disagree,        // 反对
    StrongDisagree,  // 强烈反对（附带理由）
}

pub enum EscalationReason {
    LowConfidence { score: f64 },          // 信心不足
    RepeatedFailure { attempts: u32 },     // 反复失败
    ConflictingProposals,                  // 方案冲突
    Timeout,                               // 超时
    SecurityConcern,                       // 安全顾虑
    NeedHumanDecision,                     // 需要用户决策
}
```

### AgentBus 实现

```rust
pub struct AgentBus {
    /// 基于 tokio broadcast，复用 IoT EventBus 的成熟模式
    sender: broadcast::Sender<AgentEnvelope>,
    /// 消息历史（用于新加入的Agent获取上下文）
    history: Arc<RwLock<Vec<AgentEnvelope>>>,
    /// 订阅过滤器
    filters: Arc<RwLock<HashMap<AgentId, Vec<MessageFilter>>>>,
}

impl AgentBus {
    pub fn new(capacity: usize) -> Self { /* ... */ }

    /// 发送消息
    pub fn send(&self, envelope: AgentEnvelope) {
        // 记录历史
        self.history.write().unwrap().push(envelope.clone());
        // 广播
        let _ = self.sender.send(envelope);
    }

    /// 订阅消息（带过滤）
    pub fn subscribe(&self, agent_id: AgentId, filter: MessageFilter)
        -> broadcast::Receiver<AgentEnvelope> { /* ... */ }

    /// 获取历史消息（新Agent加入时获取上下文）
    pub async fn get_history(&self, since: DateTime<Utc>) -> Vec<AgentEnvelope> { /* ... */ }
}
```

---

## 七、质量门禁与交付验收

每个交付物必须通过三道门禁，任何一道不通过都会打回。

### 三道门禁

```
     交付物
        │
        ▼
┌──────────────┐
│  功能验收    │  QA Agent 验证
│  ──────────  │  ✓ 功能是否正确实现？
│              │  ✓ 边界条件覆盖？
│              │  ✓ 测试覆盖率 ≥ 80%？
│              │  ✓ 性能达标？
└──────┬───────┘
       │ PASS
       ▼
┌──────────────┐
│  安全审查    │  Security Agent 验证
│  ──────────  │  ✓ OWASP Top 10 检查？
│              │  ✓ 输入验证完整？
│              │  ✓ 无硬编码密钥？
│              │  ✓ 权限模型正确？
└──────┬───────┘
       │ PASS
       ▼
┌──────────────┐
│  架构审查    │  Architect Agent 验证
│  ──────────  │  ✓ 符合架构原则？
│              │  ✓ 技术债务可控？
│              │  ✓ 命名规范一致？
│              │  ✓ 可维护可扩展？
└──────┬───────┘
       │ PASS
       ▼
   ✅ 交付通过
```

### 打回机制

```
第1次打回 → 附带详细反馈，原Agent修复
第2次打回 → 触发 Pair Review（两个Agent一起看问题）
第3次打回 → 升级到大管家，启动圆桌会议
```

### 质量标准量化

```rust
pub struct QualityStandards {
    // 代码质量
    pub min_test_coverage: f64,            // 最低测试覆盖率 (0.8 = 80%)
    pub max_cyclomatic_complexity: u32,    // 最大圈复杂度 (10)
    pub max_function_lines: u32,           // 函数最大行数 (50)
    pub max_file_lines: u32,              // 文件最大行数 (800)

    // 安全标准
    pub zero_critical_vulnerabilities: bool,  // 零Critical漏洞
    pub max_high_vulnerabilities: u32,        // 最多High漏洞数 (0)
    pub require_input_validation: bool,       // 必须有输入验证

    // 文档标准
    pub require_doc_comments: bool,           // 公开API必须有文档注释
    pub require_changelog: bool,              // 必须更新变更日志

    // 性能标准
    pub max_response_time_ms: Option<u64>,    // 最大响应时间
    pub min_throughput_qps: Option<u64>,      // 最低吞吐量
}

pub struct ReviewComment {
    pub severity: Severity,        // Critical / High / Medium / Low / Info
    pub category: ReviewCategory,  // Security / Performance / Style / Logic / Test
    pub location: String,          // 文件:行号
    pub message: String,           // 问题描述
    pub suggestion: Option<String>,// 修复建议
}

pub enum Severity {
    Critical,  // 必须修复，阻断交付
    High,      // 必须修复
    Medium,    // 建议修复
    Low,       // 可选修复
    Info,      // 仅供参考
}
```

---

## 八、共识与瓶颈处理机制

### 瓶颈自动检测

```
自动触发集体讨论的条件（任一满足即触发）：

┌─────────────────────────────────────────────────────┐
│  触发条件                    阈值                    │
├─────────────────────────────────────────────────────┤
│  Agent自评信心不足           confidence < 0.7        │
│  同一任务反复失败            failures ≥ 3            │
│  两个Agent方案冲突           conflicting proposals   │
│  子任务超时                  elapsed > 2x estimate   │
│  Agent主动求助               TaskBlocked.need_help   │
│  安全Agent发现高危问题       SecurityConcern         │
└─────────────────────────────────────────────────────┘
```

### 圆桌会议流程（Roundtable Protocol）

```
┌──────────────────────────────────────────────────────────┐
│                    圆桌会议 (Roundtable)                   │
│                                                          │
│  主持: 大管家                                             │
│  参与: 所有相关Agent                                      │
│                                                          │
│  Round 1 — 问题陈述 (Problem Statement)                   │
│    遇到瓶颈的Agent描述：                                   │
│    - 问题是什么                                           │
│    - 已尝试的方案                                         │
│    - 失败原因                                             │
│    - 需要什么帮助                                         │
│                                                          │
│  Round 2 — 各方提案 (Proposals)                           │
│    每个Agent从自己的专业角度提出解决方案：                  │
│    Architect: 从架构角度                                   │
│    Engineer:  从实现角度                                   │
│    Security:  从安全角度                                   │
│    QA:        从可测试性角度                               │
│    每个提案附带信心分数 (0.0 - 1.0)                       │
│                                                          │
│  Round 3 — 交叉质疑 (Cross-Examination)                   │
│    Agent之间互相质疑对方方案的弱点：                       │
│    "你的方案在高并发下会有什么问题？"                      │
│    "这个方案的安全风险你考虑了吗？"                        │
│    "测试怎么覆盖这个边界条件？"                            │
│    被质疑的Agent必须回应或修改方案                         │
│                                                          │
│  Round 4 — 加权投票 (Weighted Voting)                     │
│    vote_weight = 专业相关度 × 信心分数 × 历史准确率        │
│                                                          │
│    ≥ 60% 加权同意 → 采纳方案                              │
│    < 60% 且无否决 → 大管家裁决                            │
│    有否决（Security）→ 必须解决安全顾虑后重新投票          │
│    僵局 → 升级给用户决策                                  │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

### 共识机制实现

```rust
pub struct Roundtable {
    pub id: Uuid,
    pub topic: String,
    pub trigger: EscalationReason,
    pub participants: Vec<AgentId>,
    pub rounds: Vec<RoundRecord>,
    pub outcome: Option<ConsensusOutcome>,
}

pub enum ConsensusOutcome {
    Adopted {
        proposal_id: Uuid,
        vote_score: f64,
        dissents: Vec<Dissent>,
    },
    StewardDecision {
        decision: String,
        rationale: String,
    },
    EscalatedToUser {
        summary: String,
        options: Vec<String>,
    },
}

pub struct Dissent {
    pub agent_id: AgentId,
    pub role: Role,
    pub concern: String,
    pub severity: Severity,
}

impl Roundtable {
    /// 计算加权投票结果
    pub fn tally_votes(&self, votes: &[WeightedVote]) -> VoteResult {
        let total_weight: f64 = votes.iter().map(|v| v.weight).sum();
        let agree_weight: f64 = votes.iter()
            .filter(|v| matches!(v.stance, Stance::Agree | Stance::StrongAgree))
            .map(|v| v.weight)
            .sum();

        let ratio = agree_weight / total_weight;

        // 检查是否有否决
        let vetoes: Vec<_> = votes.iter()
            .filter(|v| v.has_veto_power && matches!(v.stance, Stance::StrongDisagree))
            .collect();

        if !vetoes.is_empty() {
            VoteResult::Vetoed { by: vetoes.iter().map(|v| v.agent_id.clone()).collect() }
        } else if ratio >= 0.6 {
            VoteResult::Adopted { score: ratio }
        } else {
            VoteResult::NoConsensus { score: ratio }
        }
    }
}
```

---

## 九、完整工作流示例

### 示例：用户说"给普罗米加一个语音唤醒功能"

```
═══════════════════════════════════════════════════════════════
  Step 1 — 大管家分析
═══════════════════════════════════════════════════════════════

  用户意图: 新增语音唤醒功能
  复杂度评估: 高（涉及音频处理、模型集成、API设计、隐私合规）
  需要团队: PM + Architect + Engineer + QA + Security
  子任务数: 5
  预估模型消耗: ~50K tokens

═══════════════════════════════════════════════════════════════
  Step 2 — 组建团队（5个Agent同时创建并注入身份）
═══════════════════════════════════════════════════════════════

  [SPAWN] PM Agent        ← 注入产品经理身份 (Claude)
  [SPAWN] Architect Agent ← 注入首席架构师身份 (Claude)
  [SPAWN] Engineer Agent  ← 注入资深工程师身份 (GPT-4o)
  [SPAWN] QA Agent        ← 注入质量工程师身份 (GPT-4o)
  [SPAWN] Security Agent  ← 注入安全工程师身份 (Claude)

═══════════════════════════════════════════════════════════════
  Step 3 — 任务分发（DAG）
═══════════════════════════════════════════════════════════════

  Task 1 [PM]        → 输出 PRD（需求文档 + 验收标准）
  Task 2 [Architect] → 基于PRD输出 Design Doc（依赖 Task 1）
  Task 3 [Engineer]  → 基于Design Doc实现代码（依赖 Task 2）
  Task 4 [QA]        → 基于PRD编写测试，验收代码（依赖 Task 1, 3）
  Task 5 [Security]  → 全程安全审查（并行，监听所有产出）

  依赖图:
  Task1(PM) ──→ Task2(Arch) ──→ Task3(Eng) ──→ Task4(QA)
      │                                           ↑
      └───────────────────────────────────────────┘
                    Task5(Sec) ← 监听所有

═══════════════════════════════════════════════════════════════
  Step 4 — 执行过程中的内部通信
═══════════════════════════════════════════════════════════════

  [PM → All]
  "PRD已完成。核心需求：支持自定义唤醒词，误唤醒率<1%，
   延迟<200ms，音频数据不离开设备。P0功能。"

  [Architect → PM]
  "PRD中没有提到多语言唤醒词的需求，是否需要支持？"

  [PM → Architect]
  "MVP阶段只支持中文和英文，后续迭代再扩展。"

  [Architect → All]
  "Design Doc已完成。方案：两阶段检测架构——
   Stage 1: 轻量级MFCC特征提取 + 小模型预筛（<10ms）
   Stage 2: 精确模型验证（<100ms）
   音频处理全部本地，复用现有 voice 模块。"

  [Security → Architect]
  "Design Doc中音频缓冲区的生命周期需要明确。
   唤醒词检测完成后，原始音频必须立即清零，不能留在内存中。"

  [Architect → Security]
  "同意。已更新Design Doc，增加'音频数据零留存'约束。"

  [Engineer → Architect]
  "这个模块用 trait object 还是泛型实现检测后端？"

  [Architect → Engineer]
  "用泛型。这是热路径，trait object的虚函数调用开销在这里不可接受。"

  [QA → Engineer]
  "边界条件提醒：空音频输入、超长音频、采样率不匹配怎么处理？"

  [Engineer → QA]
  "好问题。空音频返回 NotDetected，超长音频按窗口滑动，
   采样率不匹配先重采样。我会加对应的单元测试。"

═══════════════════════════════════════════════════════════════
  Step 5 — 瓶颈处理
═══════════════════════════════════════════════════════════════

  [Engineer → Steward] ⚠️ TaskBlocked
  "唤醒词检测的误唤醒率太高（3.2%），试了3种方案都不行：
   1. 纯MFCC + DTW → 5.1%
   2. 小型CNN → 3.2%
   3. 调整阈值 → 漏检率上升到8%"

  [Steward] → 触发圆桌会议
  [Steward] → 临时创建 Research Agent（注入研究科学家身份）

  [Roundtable — Round 1: 问题陈述]
  Engineer 描述问题和已尝试方案

  [Roundtable — Round 2: 各方提案]
  Architect:  "建议用两阶段级联：粗筛+精筛，精筛用更大模型"
  Researcher: "参考 Picovoice Porcupine 的方案，用 GRU 替代 CNN，
               论文显示误唤醒率可降到 0.5%"
  Security:   "无论哪种方案，模型权重必须本地存储，不能远程加载"
  QA:         "建议增加噪声环境测试集，覆盖 SNR 0-20dB"

  [Roundtable — Round 3: 交叉质疑]
  Engineer → Researcher: "GRU 的推理延迟能控制在 200ms 内吗？"
  Researcher → Engineer: "在 ARM 上实测 ~80ms，满足要求"
  Architect → Researcher: "模型大小？需要控制在 5MB 以内"
  Researcher → Architect: "量化后 2.1MB，没问题"

  [Roundtable — Round 4: 投票]
  Architect:  Agree     (weight: 0.9)  → GRU级联方案
  Researcher: StrongAgree (weight: 0.95) → GRU级联方案
  Engineer:   Agree     (weight: 0.85) → GRU级联方案
  Security:   Agree     (weight: 0.8)  → 附加条件：模型本地存储
  QA:         Agree     (weight: 0.75) → 附加条件：增加噪声测试

  结果: 加权同意 89% → ✅ 采纳GRU级联方案
  [Steward] → 销毁 Research Agent

═══════════════════════════════════════════════════════════════
  Step 6 — 交付验收
═══════════════════════════════════════════════════════════════

  [Engineer] → 提交代码

  [QA 审查] → 第1轮
  ✓ 功能测试: 18/18 通过
  ✓ 边界测试: 12/12 通过
  ✓ 覆盖率: 84%
  ✓ 性能: 唤醒延迟 92ms (< 200ms)
  → PASS

  [Security 审查] → 第1轮
  ✗ FAIL: 发现音频缓冲区在异常退出时未清零（PII泄漏风险）
  → 打回 Engineer，附带修复建议

  [Engineer] → 修复：增加 Drop trait 实现，确保缓冲区析构时清零

  [Security 审查] → 第2轮
  ✓ 音频缓冲区生命周期正确
  ✓ 无硬编码密钥
  ✓ 输入验证完整
  → PASS

  [Architect 审查]
  ✓ 符合两阶段架构设计
  ✓ 泛型接口设计合理
  ✓ 与现有 voice 模块集成干净
  ✓ 技术债务可控
  → PASS

  ✅ 三道门禁全部通过

═══════════════════════════════════════════════════════════════
  Step 7 — 归档与团队解散
═══════════════════════════════════════════════════════════════

  [归档到记忆系统]
  - PRD文档 → 知识库
  - Design Doc → 知识库
  - 圆桌会议记录 → 情景记忆（GRU方案的决策过程）
  - 代码 + 测试 → 交付物

  [销毁Agent]
  [DESTROY] PM Agent
  [DESTROY] Architect Agent
  [DESTROY] Engineer Agent
  [DESTROY] QA Agent
  [DESTROY] Security Agent

  [大管家 → 用户]
  "语音唤醒功能已完成。采用两阶段GRU级联检测架构，
   误唤醒率 0.8%，唤醒延迟 92ms，音频数据零留存。
   包含 30 个测试用例，覆盖率 84%。"
```

---

## 十、模块规划

### 目录结构

```
core/src/agent/
├── mod.rs                        # 模块导出
├── types.rs                      # AgentId, TaskId, Deliverable 等基础类型
├── error.rs                      # AgentError 错误类型
│
├── identity/                     # ═══ 身份注入系统 ═══
│   ├── mod.rs
│   ├── vault.rs                  # Identity Vault — 岗位身份模板库
│   ├── injection.rs              # 身份注入引擎 — 组装完整 system prompt
│   ├── competency.rs             # 能力矩阵定义
│   └── roles/                    # 各岗位身份模板（详细 persona prompt）
│       ├── mod.rs
│       ├── architect.rs          # 首席架构师
│       ├── engineer.rs           # 资深工程师
│       ├── security.rs           # 安全工程师
│       ├── qa.rs                 # 质量工程师
│       ├── product_manager.rs    # 产品经理
│       ├── researcher.rs         # 研究科学家
│       ├── devops.rs             # SRE/运维
│       └── data_analyst.rs       # 数据分析师
│
├── steward/                      # ═══ 大管家 ═══
│   ├── mod.rs
│   ├── orchestrator.rs           # 核心编排逻辑
│   ├── planner.rs                # 任务拆解器（输出 TaskGraph DAG）
│   ├── team_builder.rs           # 团队组建器（角色选择 + Agent创建）
│   ├── monitor.rs                # 进度监控 + 超时检测
│   └── escalation.rs             # 升级策略（何时触发圆桌会议）
│
├── runtime/                      # ═══ Agent运行时 ═══
│   ├── mod.rs
│   ├── executor.rs               # Agent执行引擎（ReAct循环）
│   ├── lifecycle.rs              # 生命周期管理（创建/销毁/归档）
│   ├── model_pool.rs             # 云端模型连接池
│   └── context.rs                # Agent上下文管理（对话历史、工具结果）
│
├── comm/                         # ═══ 内部通信系统 ═══
│   ├── mod.rs
│   ├── bus.rs                    # AgentBus — 消息总线
│   ├── direct.rs                 # 1:1 直接消息
│   ├── group.rs                  # 群组讨论（圆桌会议）
│   └── protocol.rs              # 消息类型 + 序列化
│
├── quality/                      # ═══ 质量门禁 ═══
│   ├── mod.rs
│   ├── gate.rs                   # 三道门禁流程控制
│   ├── review.rs                 # 审查结果结构 + 打回机制
│   └── standards.rs              # 质量标准定义
│
├── consensus/                    # ═══ 共识机制 ═══
│   ├── mod.rs
│   ├── roundtable.rs             # 圆桌会议协议
│   ├── voting.rs                 # 加权投票算法
│   ├── debate.rs                 # 辩论流程（提案/质疑/回应）
│   └── resolution.rs             # 冲突解决策略
│
└── tools/                        # ═══ 工具注册表 ═══
    ├── mod.rs
    ├── registry.rs               # ToolRegistry — 统一工具注册
    ├── memory_tool.rs            # 封装 MemoryManager
    ├── search_tool.rs            # 封装向量搜索
    ├── device_tool.rs            # 封装 IoT 设备控制
    ├── scene_tool.rs             # 封装场景执行
    ├── plugin_tool.rs            # 封装 WASM 插件调用
    ├── code_tool.rs              # 代码执行（沙箱）
    └── http_tool.rs              # HTTP 请求工具
```

### 代码量预估

| 模块 | 文件数 | 预估行数 | 测试数 |
|------|--------|---------|--------|
| `types.rs` + `error.rs` | 2 | ~200 | ~10 |
| `identity/` | 10 | ~800 | ~20 |
| `steward/` | 5 | ~600 | ~25 |
| `runtime/` | 4 | ~500 | ~20 |
| `comm/` | 4 | ~400 | ~15 |
| `quality/` | 3 | ~300 | ~15 |
| `consensus/` | 4 | ~400 | ~20 |
| `tools/` | 8 | ~500 | ~15 |
| **总计** | **40** | **~3,700** | **~140** |

---

## 十一、实施路线

```
╭──────────────────────────────────────────────────────────────────╮
│                                                                    │
│   Phase 1 ── 基础框架                                    Week 1-2 │
│   types.rs + error.rs + identity/ + runtime/lifecycle.rs          │
│   目标: Agent可以被创建、注入身份、销毁                           │
│                                                                    │
│   Phase 2 ── 通信系统                                    Week 2-3 │
│   comm/ 全部                                                      │
│   目标: Agent之间可以发消息、订阅事件、群组讨论                   │
│                                                                    │
│   Phase 3 ── 大管家核心                                  Week 3-4 │
│   steward/ 全部                                                   │
│   目标: 大管家可以拆解任务、组建团队、分发任务                    │
│                                                                    │
│   Phase 4 ── 执行引擎                                    Week 4-5 │
│   runtime/executor.rs + runtime/model_pool.rs + tools/            │
│   目标: Agent可以使用工具执行任务、产出交付物                     │
│                                                                    │
│   Phase 5 ── 质量门禁                                    Week 5-6 │
│   quality/ 全部                                                   │
│   目标: 交付物必须通过三道门禁才能交付                            │
│                                                                    │
│   Phase 6 ── 共识机制                                    Week 6-7 │
│   consensus/ 全部                                                 │
│   目标: 瓶颈自动检测、圆桌会议、加权投票                         │
│                                                                    │
│   Phase 7 ── 集成联调                                    Week 7-8 │
│   接入 ChatOrchestrator、端到端测试                               │
│   目标: 用户请求可以自动路由到Agent集群                           │
│                                                                    │
╰──────────────────────────────────────────────────────────────────╯
```

### 每个Phase的验收标准

| Phase | 验收标准 |
|-------|---------|
| Phase 1 | 可创建8种角色的Agent，身份注入正确，销毁后资源释放 |
| Phase 2 | 1:1消息、广播、群组讨论全部可用，消息历史可查 |
| Phase 3 | 给定用户请求，大管家能输出合理的TaskGraph和团队配置 |
| Phase 4 | Agent能调用工具完成子任务，产出结构化交付物 |
| Phase 5 | 交付物经过QA/Security/Architect三道审查，打回机制正常 |
| Phase 6 | 瓶颈自动触发圆桌会议，投票产出共识，否决权生效 |
| Phase 7 | 端到端：用户请求 → 集群协作 → 交付结果，全流程跑通 |

---

## 十二、参考研究

### 开源项目

| 项目 | 借鉴点 |
|------|--------|
| [MetaGPT](https://github.com/geekan/MetaGPT) | 角色分工 + 消息订阅模式 |
| [OpenAI Swarm](https://github.com/openai/swarm) | Agent + Handoff 移交机制 |
| [CrewAI](https://github.com/crewAIInc/crewAI) | Agent角色定义结构 + 委派机制 |
| [AutoGen](https://github.com/microsoft/autogen) | GroupChat + Speaker Selection |
| [ChatDev](https://github.com/OpenBMB/ChatDev) | 虚拟软件公司 + Code Review 阶段 |
| [LangGraph](https://github.com/langchain-ai/langgraph) | 状态图驱动工作流 |
| [Rig](https://github.com/0xPlaygrounds/rig) | Rust原生Agent框架 |
| [SWE-agent](https://github.com/princeton-nlp/SWE-agent) | ACI工具层设计 |

### 能力模型参考

| 来源 | 参考内容 |
|------|---------|
| Google Engineering Levels (L3-L7) | 工程师能力分级标准 |
| Meta IC Levels (E3-E8) | 技术影响力范围定义 |
| Stripe Engineering Ladder | 技术决策权限模型 |
| Netflix Freedom & Responsibility | 自主决策文化 |
| Amazon Working Backwards | 产品需求分析方法 |
| Google Design Doc Template | 技术方案文档标准 |
| Stripe RFC Process | 技术决策流程 |

### 学术论文

| 论文 | 相关性 |
|------|--------|
| Society of Mind (Minsky) | 多Agent认知架构理论基础 |
| Debate (Irving et al.) | AI辩论提升决策质量 |
| ReAct (Yao et al.) | Agent推理+行动循环 |
| Reflexion (Shinn et al.) | Agent自我反思改进 |
| CAMEL (Li et al.) | 角色扮演多Agent协作 |

---

<div align="center">

```
╔══════════════════════════════════════════════════════════════════════╗
║                                                                      ║
║   P R O M E T H E U S   A G E N T   C L U S T E R                  ║
║                                                                      ║
║   "一支按需组建的世界顶级数字团队"                                    ║
║                                                                      ║
║   8 Roles · 3 Quality Gates · Roundtable Consensus                   ║
║   ~3,700 Lines · ~140 Tests · 7 Phases                               ║
║                                                                      ║
╚══════════════════════════════════════════════════════════════════════╝
```

</div>
