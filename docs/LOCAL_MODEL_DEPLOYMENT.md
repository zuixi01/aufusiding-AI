# 本地模型部署方案

## 目标
使用本地模型完成核心模块的完全体实现，摆脱对云端 API 的依赖。

## 模型选型

### 1. 主对话模型（LLM）
**推荐**: Qwen2.5 系列
- **Qwen2.5-3B**: 快速响应，闲聊和简单任务
- **Qwen2.5-7B**: 平衡性能，通用对话
- **Qwen2.5-14B**: 高质量输出，复杂推理

**优势**:
- 中文能力强
- 量化版本（GGUF）性能好
- 社区支持完善
- 可商用

### 2. Embedding 模型
**推荐**: bge-large-zh-v1.5
- 维度: 1024
- 中文语义理解优秀
- 适合 RAG 检索

**备选**: text-embedding-ada-002 (OpenAI) 的开源替代
- all-MiniLM-L6-v2 (轻量)
- gte-large-zh (国产)

### 3. 意图分类模型
**推荐**: 微调的 BERT 模型
- bert-base-chinese
- roberta-base-chinese

**方案**: 基于现有数据微调 6 分类模型
- Casual, Knowledge, ToolUse, CodeGen, Creative, Reasoning

### 4. 行为预测模型
**推荐**: 轻量级 Transformer
- DistilBERT
- TinyBERT

**输入**: 用户历史行为序列
**输出**: 下一步行为概率分布

## 部署架构

```
┌─────────────────────────────────────────────────┐
│              Prometheus Core                     │
├─────────────────────────────────────────────────┤
│  智能路由                                         │
│  ├─ 意图分类: BERT (本地)                        │
│  ├─ 能力矩阵: 动态学习                           │
│  └─ 路由决策: 多目标优化                         │
├─────────────────────────────────────────────────┤
│  记忆系统                                         │
│  ├─ Embedding: bge-large-zh (本地)              │
│  ├─ 向量存储: Qdrant (本地部署)                  │
│  └─ 检索: BM25 + 向量 + RRF                      │
├─────────────────────────────────────────────────┤
│  Daemon Mind                                     │
│  ├─ 预测: Transformer (本地)                     │
│  ├─ 信念: 贝叶斯网络                             │
│  └─ 反思: LLM 驱动                               │
├─────────────────────────────────────────────────┤
│  对话生成                                         │
│  ├─ Qwen2.5-3B: 快速响应                         │
│  ├─ Qwen2.5-7B: 通用对话                         │
│  └─ Qwen2.5-14B: 复杂推理                        │
└─────────────────────────────────────────────────┘
```

## 实施计划

### Phase 1: 基础设施（1-2周）
- [ ] 部署 Ollama 或 llama.cpp 作为推理引擎
- [ ] 下载并量化 Qwen2.5 模型（3B/7B/14B）
- [ ] 部署 Qdrant 向量数据库
- [ ] 集成 sentence-transformers (Python) 或 candle (Rust)

### Phase 2: Embedding 集成（1周）
- [ ] 实现 bge-large-zh 的 Rust 绑定
- [ ] 替换模拟的 embedding 生成
- [ ] 更新向量存储接口
- [ ] 测试检索质量

### Phase 3: 意图分类模型（1-2周）
- [ ] 收集/标注意图分类数据集
- [ ] 微调 BERT 模型（6分类）
- [ ] 导出 ONNX 格式
- [ ] 集成到 Rust（使用 tract 或 ort）
- [ ] 替换现有的关键词匹配

### Phase 4: 行为预测模型（1-2周）
- [ ] 设计行为序列编码方案
- [ ] 训练 Transformer 预测模型
- [ ] 集成到预测模块
- [ ] 在线学习机制

### Phase 5: 智能路由优化（1周）
- [ ] 实现动态能力矩阵学习
- [ ] 添加 A/B 测试框架
- [ ] 实时性能监控
- [ ] 自适应路由策略

### Phase 6: 反思层增强（1周）
- [ ] 使用 LLM 生成反思内容
- [ ] 元认知提示工程
- [ ] 自我改进循环

## 技术栈

### Rust 生态
- **llama.cpp**: LLM 推理（通过 FFI）
- **candle**: Hugging Face 模型推理
- **tract/ort**: ONNX 运行时
- **tokenizers**: 分词器
- **qdrant-client**: 向量数据库客户端

### Python 桥接（可选）
- **PyO3**: Rust 调用 Python
- **sentence-transformers**: Embedding 生成
- **transformers**: 模型加载

### 模型格式
- **GGUF**: LLM 量化格式（llama.cpp）
- **ONNX**: 通用模型格式
- **SafeTensors**: Hugging Face 格式

## 硬件需求

### 最低配置
- CPU: 8核+
- RAM: 16GB
- 存储: 50GB SSD
- GPU: 可选（加速推理）

### 推荐配置
- CPU: 16核+
- RAM: 32GB
- 存储: 100GB NVMe SSD
- GPU: NVIDIA RTX 3060 (12GB VRAM) 或更高

### 模型大小估算
- Qwen2.5-3B (Q4): ~2GB
- Qwen2.5-7B (Q4): ~4GB
- Qwen2.5-14B (Q4): ~8GB
- bge-large-zh: ~1.3GB
- BERT 分类器: ~400MB
- 预测模型: ~200MB

**总计**: 约 16GB 存储

## 性能目标

### 推理延迟
- 意图分类: <50ms
- Embedding 生成: <100ms
- 向量检索: <200ms
- LLM 生成 (3B): <2s (首token)
- LLM 生成 (7B): <3s (首token)
- LLM 生成 (14B): <5s (首token)

### 吞吐量
- 并发请求: 10+ (3B模型)
- 每日对话: 10,000+

## 成本分析

### 一次性成本
- 硬件: $1,000 - $3,000 (取决于GPU)
- 开发时间: 6-8周

### 运营成本
- 电费: ~$50/月 (24/7运行)
- 维护: 最小

### vs 云端 API
- GPT-4: $0.03/1K tokens → $300/月 (1M tokens)
- 本地: 固定成本，无限使用
- **ROI**: 3-6个月回本

## 风险与挑战

### 技术风险
- ⚠️ Rust ML 生态不如 Python 成熟
- ⚠️ 模型量化可能损失精度
- ⚠️ 多模型协同的复杂度

### 缓解方案
- ✅ 使用 PyO3 桥接 Python 生态
- ✅ 对比测试量化前后效果
- ✅ 模块化设计，逐步替换

### 性能风险
- ⚠️ 本地推理速度 < 云端
- ⚠️ 资源竞争（多模型同时运行）

### 缓解方案
- ✅ 模型量化和优化
- ✅ 异步推理和批处理
- ✅ 智能调度和缓存

## 下一步行动

### 立即开始
1. **环境准备**: 安装 Ollama + Qwen2.5 模型
2. **集成测试**: 验证 llama.cpp 集成
3. **Embedding**: 部署 bge-large-zh

### 本周目标
- [ ] Qwen2.5-7B 运行起来
- [ ] 替换模拟 embedding
- [ ] 向量检索质量测试

### 本月目标
- [ ] 完成 Phase 1-3
- [ ] 意图分类模型上线
- [ ] 性能基准测试

## 参考资源

### 模型下载
- Qwen2.5: https://huggingface.co/Qwen
- bge-large-zh: https://huggingface.co/BAAI/bge-large-zh-v1.5
- BERT-Chinese: https://huggingface.co/bert-base-chinese

### 工具
- Ollama: https://ollama.ai/
- llama.cpp: https://github.com/ggerganov/llama.cpp
- Qdrant: https://qdrant.tech/
- Candle: https://github.com/huggingface/candle

### 文档
- Rust ML Book: https://www.arewelearningyet.com/
- ONNX Runtime: https://onnxruntime.ai/
