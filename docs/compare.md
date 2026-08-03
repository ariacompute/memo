# 功能对比矩阵 — aria-memory vs 业界长期记忆系统

> 对比对象：mem0 / MemOS / MemPalace / Zep / Letta。  
> 图例：`✅` 具备 · `⚠️` 部分/需外部依赖 · `❌` 不具备或非设计目标。  
> 依据公开文档与产品定位（2026）；托管云能力与开源 SDK 可能不一致，以开源/可本地部署路径为主。

## 总表

| 维度 | aria-memory | mem0 | MemOS | MemPalace | Zep | Letta |
|------|:-----------:|:----:|:-----:|:---------:|:---:|:-----:|
| 三层/分层记忆 | ✅ Working/ST/LT | ⚠️ 会话+长期语义 | ✅ MemCube/调度分层 | ✅ Wings→Rooms 空间分层 | ⚠️ 时序图/会话 | ⚠️ Agent 状态+归档 |
| Episodic / Semantic / Entity / Graph 类型 | ✅ 模型齐全（graph 存型为主） | ✅ 抽取分类 | ✅ 图结构化记忆 | ⚠️ 空间隐喻组织 | ✅ 知识图谱时序 | ⚠️ 工具/核心记忆 |
| CRUD（add/get/update/delete） | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 混合检索（语义+关键词） | ✅ | ✅ 多信号 | ✅ 混合检索 | ✅ 检索 | ✅ | ⚠️ 依赖嵌入/检索配置 |
| 巩固 / 去重 / 遗忘 | ✅ | ⚠️ 更新/去重偏 LLM | ✅ 反馈修正/演进 | ⚠️ 组织为主 | ⚠️ 策略化 | ⚠️ 驱逐/归档 |
| 写路径零 LLM | ✅ | ❌ 抽取依赖 LLM | ⚠️ 可配，默认偏 LLM | ✅ 原文存储可无 LLM | ⚠️ 常配 LLM | ⚠️ Agent 循环常配 LLM |
| 本地/离线嵌入 | ✅ ngram+哈希/TF-IDF | ⚠️ 可接本地模型 | ⚠️ 可接本地 | ✅ 可本地 | ⚠️ 常云嵌入 | ⚠️ 可自托管 |
| 嵌入式持久化 | ✅ SQLite | ⚠️ 多后端可选 | ⚠️ SQLite/图库等 | ✅ 本地优先 | ⚠️ 服务端为主 | ⚠️ 服务/DB |
| 多端/云同步 | ❌（后续） | ✅ 托管平台 | ✅ 企业能力 | ⚠️ 有限 | ✅ | ⚠️ |
| 图记忆（推理级） | ⚠️ 类型占位 | ⚠️ 实体链接 | ✅ | ⚠️ 空间索引 | ✅ | ❌ 非主路径 |
| 多模态记忆 | ❌ | ⚠️ 扩展中 | ✅ | ❌ 文本为主 | ⚠️ | ⚠️ |
| 语言 / 运行时 | Rust | Python | Python | Python 等 | 服务/SDK | Python |
| 边缘 / 移动就绪 | ✅ 零网络、轻依赖 | ❌ 偏服务 | ⚠️ 偏服务 | ⚠️ 桌面/本地 | ❌ | ❌ |
| 开源可自托管评测 | ✅ | ✅ OSS + 托管 | ✅ | ✅ | ⚠️ 社区/云 | ✅ |

## 定位差异（读矩阵前必读）

| 系统 | 一句话定位 |
|------|------------|
| **aria-memory** | 端侧 local-first **存储+检索**层；不内置 LLM 抽取与 Judge。 |
| **mem0** | 生产级 Agent 记忆；单遍分层抽取 + 多信号检索；LoCoMo/LongMemEval/BEAM 强。 |
| **MemOS** | 记忆操作系统；MemCube、调度、多模态与 OmniMemEval 对比。 |
| **MemPalace** | 空间隐喻组织 + 可无写时 LLM；LongMemEval Recall 突出、零写时成本。 |
| **Zep** | 会话/时序知识图谱记忆服务，偏云与 Agent 上下文装配。 |
| **Letta**（原 MemGPT） | Agent 运行时 + 分层上下文/记忆管理，非纯记忆后端。 |

## 与评测的对应关系

- **Track A**（`benches/track_a`）：延迟、吞吐、体积、离线、合成 Recall —— 最能体现 aria 差异化。
- **Track B**（`benches/track_b`）：LoCoMo / LongMemEval / BEAM —— 需 LLM 答/判；报告必须分列离线条件与模型栈。

生成/更新微基准数字见 [bench_results.md](./bench_results.md) 与 `python benches/run.py`。
