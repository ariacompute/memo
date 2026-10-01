# 功能对比矩阵 — aria-memo vs 业界长期记忆系统

> 对比对象：mem0 / MemOS / MemPalace / Zep / Letta。  
> 图例：`✅` 具备 · `⚠️` 部分/需外部依赖 · `❌` 不具备或非设计目标。  
> 依据公开文档与产品定位（2026）；托管云能力与开源 SDK 可能不一致，以开源/可本地部署路径为主。

## 总表

| 维度 | aria-memo | mem0 | MemOS | MemPalace | Zep | Letta |
|------|:-----------:|:----:|:-----:|:---------:|:---:|:-----:|
| 三层/分层记忆 | ✅ Working/ST/LT | ⚠️ 会话+长期语义 | ✅ MemCube/调度分层 | ✅ Wings→Rooms 空间分层 | ⚠️ 时序图/会话 | ⚠️ Agent 状态+归档 |
| Episodic / Semantic / Entity / Graph 类型 | ✅ 模型齐全（graph 存型为主） | ✅ 抽取分类 | ✅ 图结构化记忆 | ⚠️ 空间隐喻组织 | ✅ 知识图谱时序 | ⚠️ 工具/核心记忆 |
| CRUD（add/get/update/delete） | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 混合检索（语义+关键词） | ✅ | ✅ 多信号 | ✅ 混合检索 | ✅ 检索 | ✅ | ⚠️ 依赖嵌入/检索配置 |
| 巩固 / 去重 / 遗忘 | ✅ | ⚠️ 更新/去重偏 LLM | ✅ 反馈修正/演进 | ⚠️ 组织为主 | ⚠️ 策略化 | ⚠️ 驱逐/归档 |
| 写路径零 LLM | ✅ | ❌ 抽取依赖 LLM | ⚠️ 可配，默认偏 LLM | ✅ 原文存储可无 LLM | ⚠️ 常配 LLM | ⚠️ Agent 循环常配 LLM |
| 本地/离线嵌入 | ✅ ngram+哈希/TF-IDF | ⚠️ 可接本地模型 | ⚠️ 可接本地 | ✅ 可本地 | ⚠️ 常云嵌入 | ⚠️ 可自托管 |
| 检索下推（SQLite 内 FTS5 词法剪枝） | ✅ 免全表扫描、离线可用 | ⚠️ 取决于后端 | ⚠️ 取决于后端 | ⚠️ 取决于后端 | ⚠️ 取决于后端 | ⚠️ 取决于后端 |
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
| **aria-memo** | 端侧 local-first **存储+检索**层；不内置 LLM 抽取与 Judge。 |
| **mem0** | 生产级 Agent 记忆；单遍分层抽取 + 多信号检索；LoCoMo/LongMemEval/BEAM 强。 |
| **MemOS** | 记忆操作系统；MemCube、调度、多模态与 OmniMemEval 对比。 |
| **MemPalace** | 空间隐喻组织 + 可无写时 LLM；LongMemEval Recall 突出、零写时成本。 |
| **Zep** | 会话/时序知识图谱记忆服务，偏云与 Agent 上下文装配。 |
| **Letta**（原 MemGPT） | Agent 运行时 + 分层上下文/记忆管理，非纯记忆后端。 |

## 与评测的对应关系

- **Track A**（`benches/track_a`）：延迟、吞吐、体积、离线、合成 Recall —— 最能体现 aria 差异化。
  - **多尺寸扩展曲线**：`--sizes 1000,10000,100000` sweep `add`/`search` p99，报告 p99 增长因子与**亚线性/线性/超线性**判定。
  - **本地控制组**：`sqlite_vec`（SQLite 向量索引）、`chromem`（chromem-go 子进程）纳入默认 `--systems`，与 aria 同场对比；缺依赖/二进制时 skip 并写 `reason`。
  - **A2 检索质量 + 方差**：`synthetic_v2`（≥50 条真实分布查询）与复用 `track_b:*` 真实语料，输出 Recall@k / MRR 及各自**样本标准差**。
  - **写长尾压测**：`aria-memo bench --wal --batch-embed --bulk` 对照 `add_baseline` / `add_wal` / `add_batch_embed` / `add_bulk` 的 p50/p99/ops，观察 add p99 收敛。
- **Track B**（`benches/track_b`）：四基准 ——
  - **locomo_refined**：混合问答，考察 token-F1/BLEU 与严格 judge 准确率。
  - **halumem**：记忆提取/更新/QA 三任务，考察幻觉/更新/遗忘（最需要操作级能力）。
  - **longmemeval**：长上下文时间推理，考察检索召回与 QA。
  - **personamem**：个性化多选，完全离线可出多选准确率。
  - 离线指标（F1/BLEU/多选/Recall@k）零网络可出；judge 指标需 OpenAI 兼容 LLM，缺则 skip 并写 reason。报告分列「离线条件」与「LLM 管线条件」。

生成/更新微基准数字见 [bench_results.md](./bench_results.md) 与 `python benches/run.py`。

## Track A 本地控制组（microbench）

`sqlite_vec` 与 `chromem` 是**本地嵌入式**控制组，用于和 aria 同场对比存储/向量索引与写路径，而非记忆能力本身（两者均无 LLM 抽取/巩固/遗忘）。运行 `python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem` 后，下表填入真实数值：

| 系统 | 类型 | add p99@1k | add p99@10k | add p99@100k | search p99@10k | Recall@k (synthetic_v2) | MRR |
|------|------|-----------:|------------:|-------------:|---------------:|-------------------------:|----:|
| aria-memo | 本地 SQLite + FTS5 + 本地嵌入 | _run_ | _run_ | _run_ | _run_ | _run_ | _run_ |
| sqlite_vec | 本地 SQLite + vec0 向量索引 | _run_ | _run_ | _run_ | _run_ | _run_ | _run_ |
| chromem | 本地 Go 向量库（子进程） | _run_ | _run_ | _run_ | _run_ | _run_ | _run_ |

> `_run_` = 安装依赖并运行后填充；任一控制组缺失依赖时该列 skip 并写 `reason`，矩阵不伪造数值。写长尾（WAL/批量嵌入/事务合并）对照见 `aria-memo bench --wal --batch-embed --bulk --json` 的 `add_*` 分段。
