# memo 发布说明

🧠 面向 LLM Agent、纯 Rust 实现的本地优先长期记忆存储。

## ✨ 新特性

- 💾 **存储与检索**：SQLite 三层记忆（working / short_term / long_term）、FTS5 词法候选下推、语义余弦召回、批量检索、巩固、去重与遗忘，运行期全部离线
- 🕸️ **多关系记忆平面**：semantic / temporal / causal / entity 四视图有向边，write→connect 推断、retrieve→assess→expand 有界遍历与可检视 `RetrieveTrace`；`graph_bfs` 改为真实 best-score 语义并受 budget / max_hops 约束
- 📊 **评测编排**：Track A 微基准与检索质量 + Track B 四基准质量评测；A2 优先复用真实 Track B 语料，缺失时回退内置 fixture 并标注 `dataset_source`
- ⚙️ **默认评测路径**：`--sizes` 默认扫描 `1000,10000,100000`，`--systems` 使用完整本地控制组集合；文档统一使用正确的 `ARIA_MEMO_BIN` 变量名
- 🔌 **对比适配器**：移除 MemOS / MemPalace stub；通过 `AGENTMEM_BIN` 接入本地 `agentmem` HTTP 适配器，使用独立临时环境、查询前同步投影，缺依赖时按 `reason` 跳过
- 📦 **构建期词典缓存**：CI / release 通过 `LINDERA_BUILD_DICTIONARY_CACHE_DIR` 缓存 Lindera 词典，`scripts/prefetch-lindera.sh` 支持离线重建；运行期保持零网络，文档不再宣称首次构建免网络
- 🧪 **测试门禁**：CI 执行 Rust fmt / clippy / test 与 Python unittest discover，pytest 可用于本地发现；二进制级 e2e 用真实 CLI + 临时 SQLite 库跑完整记忆 / 关系 / 图检索生命周期
- 📈 **覆盖率**：Lines 与 Functions 均为 100%；新增异常路径用例将 Regions 提升至 97.80%，未引入 `#[coverage(off)]`
- 📚 **文档**：功能矩阵、中英文 README、需求、任务清单与覆盖率数字已同步；中英文 README 结构严格对齐

## ⚠️ 不兼容变更

- 🔴 `aria-memo add --type` 遇到未知类型时返回 `MemoError::InvalidParam`，不再静默回退到 `working`
- 🔴 `run.py --sizes` 与 `--systems` 默认值改为规格值；依赖旧单尺寸默认值的示例必须显式传 `--sizes`
- 🔴 移除 `memos` 与 `mempalace` adapter；本地控制组改用 `agentmem`

## ⚠️ 已知限制

- 🌐 首次 Rust 构建仍需从 Lindera.dev 下载日 / 韩词典；离线重建前先用 `scripts/prefetch-lindera.sh` 预热缓存
- 🧩 Regions 覆盖率为 97.80%；剩余分支主要是需要故障注入或依赖注入才能触达的 `?` 错误传播路径
- 🔑 `agentmem` 需要通过 `AGENTMEM_BIN` 提供本地 Go 二进制；云服务适配器（mem0 / Zep / Letta）需要凭据，缺失时按 `reason` 跳过
- 🧪 内置 fixture 是标注 `dataset_source: fixture` 的合成冒烟数据，不可与正式基准分数直接对比
- 🤖 Track B 的 LLM judge 指标仍依赖 OpenAI 兼容端点；离线指标始终输出
