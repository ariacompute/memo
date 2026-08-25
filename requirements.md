# requirements.md — memo 端侧长期记忆存储（M1 + M2 评测）

> 功能边界 / API / 表结构 / 异常 / 验收标准 / 业界评测。须经人工逐项审核后，方可据其生成 task.md 实施。

## 1. 功能边界

### 1.1 范围内（M1）
- 三层记忆模型：Working / ShortTerm / LongTerm（episodic | semantic | entity | graph）。
- 记忆条目 CRUD：`add` / `get` / `update` / `forget`。
- 本地嵌入：ngram + 哈希/TF-IDF 向量表示，余弦相似度；可注入自定义 `Embedder`。
- 持久化：嵌入式 SQLite（rusqlite bundled），自动建表/迁移/索引、批量写入。
- 检索：语义（向量余弦）+ 关键词（LIKE）混合打分，支持 top-k 与阈值。
- 记忆管理：`consolidate`（巩固：提升重要性/合并）、`dedup`（去重：相似度阈值合并）。
- 生命周期：分层老化、重要性衰减、遗忘（默认硬删除，预留软删除标记）。
- 统一错误 `MemoError`；可选 CLI（add/get/search/list/forget）。
- 全部新增功能同步单测，核心逻辑覆盖正常 + 异常路径。

### 1.2 范围外（M1，列为后续里程碑）
- 分布式/复制后端（Raft，rqlite 灵感）—— 仅 `StorageBackend` trait 抽象预留。
- libSQL/turso 同步/复制后端（feature 占位）。
- 云端协同 / 多端同步。
- GPU/NEON 向量加速、ANN 索引（HNSW 等）。
- 真实 LLM 提取/摘要（仅预留接口，M1 用规则 + 本地 embedder）。
- 业界端到端 Judge 分数的持续对标流水线（见 §6 Track B；编排在 `benches/`）。

### 1.2b Track B 基准范围（M2 增补）
Track B 收敛为四个业界基准，移除早期骨架中的 `beam` 与旧 `locomo`：
- **locomo_refined**：LoCoMo-Refined（CC BY-NC 4.0，github `mem-eval-suite/LoCoMo_refined`），单轮/多轮混合问答；离线出 token-F1 / BLEU，可选 LLM judge 准确率。
- **halumem**：HaluMem（HF `IAAR-Shanghai/HaluMem`，Medium/Long），三任务（记忆提取 / 记忆更新 / 记忆 QA）；提取/更新为操作级，需 `list`/`update` 能力。
- **longmemeval**：LongMemEval（S/M/Oracle 三变体），长上下文时间推理问答。
- **personamem**：PersonaMem（github `bowen-upenn/PersonaMem`，32k/128k/1M），个性化多选问答；完全离线可出多选准确率。

**judge 可选红线**：所有依赖 LLM 判定的指标仅在配置了 OpenAI 兼容凭据（`BENCH_LLM_API_KEY` 等）时计算，否则该指标 `skipped` 并写 `reason`；不伪造分数。离线指标（F1/BLEU/多选/Recall@k）在无数据集/无 LLM 时仍可经 fixture 跑通。

## 2. API

### 2.1 数据模型（memo-core）
```rust
pub type MemoId = String;

pub enum MemoType { Working, ShortTerm, LongTerm { kind: LongTermKind } }
pub enum LongTermKind { Episodic, Semantic, Entity, Graph }

pub struct Memo {
    pub id: MemoId,
    pub memo_type: MemoType,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: std::collections::HashMap<String, String>,
    pub importance: f32,        // 取值 [0.0, 1.0]
    pub version: u64,
    pub created_at: i64,        // unix 秒
    pub updated_at: i64,
}

pub struct SearchQuery {
    pub text: String,
    pub top_k: usize,
    pub semantic_weight: f32,   // [0,1]，与 keyword_weight 可不全为 0，和不必为 1
    pub keyword_weight: f32,    // [0,1]
    pub score_threshold: f32,
    pub memo_type: Option<MemoType>,
}

pub struct ScoredMemo { pub memo: Memo, pub score: f32 }
```

### 2.2 trait（memo-core）
- `MemoStore`：`add` / `get` / `update` / `forget` / `search`。
- `Embedder`：`fn embed(&self, text: &str) -> Result<Vec<f32>, MemoError>`。
- `StorageBackend`：`open` / `migrate` / 原始 CRUD（抽象，供复制后端扩展）。

### 2.3 MemoManager 公共 API（memo crate）
- `new(embedder: Arc<dyn Embedder>, store: Arc<dyn MemoStore>) -> Self`
- `add(content, memo_type, metadata, importance) -> Result<MemoId>`（自动 embed + 持久化；生成 id、version、时间戳）
- `get(id) -> Result<Option<Memo>>`
- `update(id, patch: MemoPatch) -> Result<()>`（content 变更时重算 embedding、version+1）
- `forget(id) -> Result<bool>`（返回是否删除成功；默认硬删除）
- `search(query) -> Result<Vec<ScoredMemo>>`（语义+关键词混合，过滤 deleted）
- `consolidate(id, delta_importance)` / `dedup(threshold)` -> 见 §1.1

### 2.4 CLI（aria-memo，二进制名 `aria-memo` / 命令显示名 `memo`）
- `memo add --type working --content "..." --importance 0.8`
- `memo get --id <id>`
- `memo search --text "..." --top-k 5`
- `memo list [--type ...] [--json]`：新增 `--json` 开关，输出机器可读 JSON 数组（每项含 `id`/`memo_type`/`content`/`importance`/`version`/`metadata`），默认人类可读输出不变（向后兼容）。
- `memo search --text "..." --top-k 5 [--json]`：新增 `--json` 开关，输出 JSON 数组（每项含 `score`/`id`/`content`/`memo_type`），默认 `score\tcontent` 逐行输出不变。
- `memo update --id <id> [--content "..."] [--type ...] [--importance 0.8]`：按 id 更新记忆（内容变更自动重算 embedding、version+1），至少一项非空；缺失 id / 空 patch / 非法类型或 importance 走 `MemoError`。供 HaluMem 操作级评测。
- `memo forget --id <id>`
- `memo bench --size N --top-k K --warmup W --json`（M2：进程内微基准 JSON）

## 3. 表结构（SQLite）

`memories` 表：
| 列 | 类型 | 约束 |
|----|------|------|
| id | TEXT | PRIMARY KEY |
| memo_type | TEXT | NOT NULL（如 `working` / `short_term` / `long_term:episodic`） |
| content | TEXT | NOT NULL |
| embedding | BLOB | 长度前缀序列化的 f32 向量（可为空） |
| metadata | TEXT | JSON 字符串 |
| importance | REAL | NOT NULL，[0,1] |
| version | INTEGER | NOT NULL |
| created_at | INTEGER | NOT NULL |
| updated_at | INTEGER | NOT NULL |
| deleted | INTEGER | NOT NULL DEFAULT 0（软删除标记） |

索引：`idx_memories_type`、`idx_memories_updated_at`、`idx_memories_deleted`。

## 4. 异常（MemoError，thiserror）

- `Io(#[from] std::io::Error)`：IO 失败。
- `Db(String)`：SQLite 操作失败。
- `NotFound(MemoId)`：`get`/`update`/`forget` 目标不存在。
- `DuplicateId(MemoId)`：`add` 重复 id。
- `EmptyContent`：内容为空。
- `EmptyEmbedding`：嵌入为空/零长。
- `InvalidParam(String)`：参数非法（importance 越界、top_k=0、query 文本空）。
- `Serialization(String)`：JSON/向量序列化失败。
- `Embedding(String)`：嵌入计算失败。
- `Other(String)`：兜底。

所有路径禁止静默失败；每条异常路径须有单测覆盖。

## 5. 验收标准（M1）

- `cargo test` 全绿（正常 + 异常用例）。
- `cargo clippy --all-targets` 无告警。
- 交叉编译：`cargo build --target aarch64-linux-android` 通过（或 `wasm32-unknown-unknown -p memo-core -p memo-embed`）。
- 黄金路径单测：`add → search → get` 端到端跑通。
- 异常单测：重复 id、缺失、空内容、空嵌入、非法参数、损坏 DB 打开失败。
- 覆盖率：核心逻辑（manager / search / consolidate / dedup / lifecycle）均有正常 + 异常用例。

## 6. 业界对比与评测（M2）

> 对比系统固定为：mem0 / MemOS / MemPalace / Zep / Letta。
> 工程约定：评测编排与适配器一律放在 `benches/`（Python），**不**新增 `crates/bench`。

### 6.1 评测分层

| 层 | 名称 | 目标 | 依赖 |
|----|------|------|------|
| **A** | 存储/检索层 | add/search 延迟与吞吐、包体/RSS、离线能力、合成集 Recall@k / MRR | aria 可零网络；他系统按 adapter 可用性跳过或标 N/A |
| **B** | 端到端记忆质量 | locomo_refined / halumem / longmemeval / personamem（四基准注册表） | 离线指标零网络可跑；judge 类指标需 OpenAI 兼容 LLM 凭据，缺则 skip 并写原因 |

定位声明：aria-memo 是 local-first 存储/检索层；B 层分数与依赖 LLM 抽取的托管产品**不可直接宣称同质碾压**，报告须分列「离线检索」与「LLM 管线」条件。

### 6.2 功能对比矩阵

- 文档：`docs/compare.md`。
- 维度至少含：三层记忆、类型（episodic/semantic/entity/graph）、CRUD、混合检索、巩固/去重/遗忘、写路径是否依赖 LLM、嵌入是否可离线、持久化、多端同步、图记忆、多模态、语言/运行时、边缘/移动就绪。
- 每格：`✅` / `⚠️` / `❌` + 一句依据；aria 与五系统均须填满。

### 6.3 Track A — 微基准与检索质量

**A1 微基准（延迟/资源）**

- 规模：库规模 `1k` / `10k`（可选 `100k`）；`search` top-k ∈ {5, 10}。
- 指标：`add` / `search` 的 p50 / p99（ms）、吞吐（ops/s）、DB 文件大小、进程 RSS、冷启动（open+migrate）、离线可跑（断网）。
- aria 侧：CLI 提供进程内 `bench` 子命令输出 JSON，避免把进程启动计入热路径；Python 解析并汇总。
- 他系统：经 `benches/adapters/*` 调用；缺依赖/密钥时 skip 并写入报告原因，不得静默失败。

**A2 合成检索质量**

- 固定 seed 的合成集（`benches/data/synthetic_retrieval.json`）：同义改写、关键词命中、干扰项。
- 指标：Recall@k、MRR（可选 nDCG）；CI 可跑小规模回归。

### 6.4 Track B — 端到端质量（四基准注册表）

- 基准：`locomo_refined` / `halumem` / `longmemeval` / `personamem`（移除早期 `beam` 与旧 `locomo`）。
- 管线：各基准 `load(path) -> Dataset` → `backend.reset()` → 流式 ingest → 逐问题 `retrieve` → 离线指标计算 + 可选 `judge` → 聚合为 `Score` 列表。
- 指标分层：
  - 离线指标（无条件计算）：LoCoMo-Refined token-F1 / BLEU；PersonaMem 多选准确率；HaluMem / LongMemEval 检索层 Recall@k。
  - judge 指标（需 OpenAI 兼容 LLM，凭据缺失则 skip）：LoCoMo-Refined 严格 judge 准确率；HaluMem 提取/更新/QA 的语义判定（Recall/Accuracy/FMR/幻觉率/遗漏率）；LongMemEval QA 准确率。
- Adapter 契约扩展：统一 `add` / `search`（及可选 `reset`）；新增**可选能力** `list_memories` / `update`，默认抛 `UnsupportedCapability` 并降级；aria adapter 实现之（依赖 CLI `update` + `list --json`）供 HaluMem 操作级评测。
- 数据集获取：`benches/datasets.py` 提供 `resolve_dataset(bench)`（真实目录优先、仓库内置 fixture 回退并标 `dataset_source: fixture`）与 `download(bench)`（urllib 拉取 HF/GitHub，失败打印手动指引）；`run.py` 增 `--download` / `--limit` / `--judge-model`。
- 产物：JSON + Markdown 报告；离线指标与 judge 指标分列，judge 标注模型名；skip 项带 `reason`。

### 6.5 工程布局（`benches/`）

```
benches/
  README.md           # 运行说明、环境变量、对比系统依赖
  requirements.txt
  run.py              # 入口：--track a|b|all
  common/             # 计时、分位数、报告写出
  track_a/            # 微基准 + 合成检索
  track_b/            # locomo_refined / halumem / longmemeval / personamem 注册表 + 子包
  metrics/            # f1 / bleu / 多选 / recall@k 等离线指标
  judge.py            # OpenAI 兼容 judge 客户端（可选）
  datasets.py         # 数据集定位与下载
  adapters/           # aria / mem0 / memos / mempalace / zep / letta
  data/               # 合成集；fixtures/ 各基准极小合成样例；外部数据集下载说明
  tests/              # Python 单测（离线、零网络、零真实 LLM）
  results/            # 生成结果（样例可入库，大体量 gitignore）
```

### 6.6 CLI 增补（供 Track A）

- `memo bench --ops add,search --size N --top-k K --warmup W --json`：进程内跑测，stdout 打印 JSON 指标。
- 不引入 criterion / `crates/bench`。

### 6.7 M2 验收标准

- `docs/compare.md` 覆盖五系统 + aria，维度齐全。
- `python benches/run.py --track a` 在默认小规模下可复现；结果写入 `benches/results/`。
- Track A 合成检索对 aria 产出 Recall@k / MRR 数值。
- `python benches/run.py --track b --dry-run` 能走通四基准加载与能力探测；真实打分在 fixture 下离线可跑（F1/BLEU/多选/Recall@k 出数值），judge 指标在缺密钥时 skip 并写原因。
- `cargo test` / clippy 仍全绿；新增 CLI `update` 与 `list --json` / `search --json` 有单测，默认输出形态不变（既有断言不破坏）。
- 五系统 adapter 均存在且实现同一基类接口；不可用时报告 N/A + 原因。
- 下载脚本在缺网络时抛错并打印手动指引，不静默失败。
