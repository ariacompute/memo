# requirements.md — memo 端侧长期记忆存储（M1 + M2 评测）

> 功能边界 / API / 表结构 / 异常 / 验收标准 / 业界评测。须经人工逐项审核后，方可据其生成 task.md 实施。

## 1. 功能边界

### 1.1 范围内（M1）
- 三层记忆模型：Working / ShortTerm / LongTerm（episodic | semantic | entity | graph）。
- 记忆条目 CRUD：`add` / `get` / `update` / `forget`。
- 本地嵌入：ngram + 哈希/TF-IDF 向量表示，余弦相似度；可注入自定义 `Embedder`。
- 持久化：嵌入式 SQLite（rusqlite bundled），自动建表/迁移/索引、批量写入。
- 检索：两条入口 —— `search` 为语义（向量余弦）+ 词法（FTS5 `bm25()`）**混合**打分（支持 top-k 与阈值；词法命中经 FTS5 候选下推，仅对候选集做内存精排，否则全表扫描保语义召回）；`recall` 为**纯向量（余弦-only）**召回，按 query 与记忆的余弦相似度排序（keyword 权重为 0）。批量入口 `search_batch`/`recall_batch`：一次持锁对多个 query 评分（见 §2.3）。
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

### 1.3 范围内（M4，多关系记忆平面）

Jev-Mem 启发的结构化多关系记忆层，叠加在扁平 `MemoStore` 之上；零网络依赖、默认离线（可插拔 LLM scorer）。

- 四视图有向关系边：`semantic`（语义关联）/ `temporal`（时间序或共现）/ `causal`（因果链）/ `entity`（同一实体聚合）。每条边带 `score ∈ [0,1]` 与 `provenance`（如 `local` 或 `llm:causal`）。
- 关系存于独立 `relations` 表，**绝不改动 `Memo`**；`Memo` 模型与既有扁平检索（search/recall）保持不变，作为图检索的回退。
- 写路径：`add` 持久化记忆后由 `MemoryController::connect` 推断并持久化四视图边（write→connect 顺序）。
- 读路径：`retrieve` 走「hybrid search 选种子锚点 → 跨 views 有界图扩展 → 打分」；返回 `GraphRetrieveResult`（打分记忆 + 可检视 `RetrieveTrace`），对齐 Jev-Mem 的 Retrieve→Assess→Expand 与透明决策。
- `RelationScorer` 可插拔：默认 `LocalRelationScorer` 纯本地启发式（semantic=cosine / temporal=时间序 / entity=token Jaccard / causal=时间邻+重叠）；可换 LLM 实现而不动控制器。
- 范围外（M4）：LLM 抽取/摘要、跨会话/跨用户全局图、云端协同图（属后续里程碑）。

### 1.4 范围内（M7，多语言词级切分）

参考 cockpit 支持的全部 13 种本地语言（en zh zh-TW es fr de it ru ja ko pt ar hi），对 FTS5 索引做词级预切分，使各语言子词均可命中（bundle 的 rusqlite 不能开启内置 `icu` 分词器，故在 Rust 侧按 Unicode script 路由）：

- 中文（zh / zh-TW）：`jieba-rs`（纯 Rust、内嵌词典，基于词典的中文词切分）。
- 日语（ja）：`lindera` + 内嵌 `ipadic` 词典（形态切分）。
- 韩语（ko）：`lindera` + 内嵌 `ko-dic` 词典（形态切分，召回质量高）。
- 空格型语言（en / es / fr / de / it / ru / pt / ar / hi）：沿用 `memories_fts` 默认 `unicode61` 空白切分（拉丁/西里尔/阿拉伯/天城文按词天然以空格分隔）。

实现：`fts5_index_text` 与 `fts5_match_expr` 共用 `segment(text)`（按 Unicode script 路由），写入侧空格拼接、查询侧逐词加引号 `OR` 连接；`is_index_term` 丢弃纯标点/空白 run。`jieba-rs` / `lindera` 分词器以进程级 `OnceLock` 单例持有，词典仅加载一次。依赖 `lindera` / `lindera-dictionary` / `lindera-ipadic` / `lindera-ko-dic`，词典内嵌（构建期与运行期均零网络下载）；代价是二进制体积更大。已有数据库的 FTS 索引在 `migrate` 时通过 `user_version`（3 → 4）一次性重建以用上新分词 token。

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
    pub keyword_weight: f32,    // [0,1]；默认 0.5（经 `SearchQuery::new` 构造）
    pub score_threshold: f32,
    pub memo_type: Option<MemoType>,
    pub query_embedding: Option<Vec<f32>>, // 预置 query 向量可跳过 embedder
}

pub struct ScoredMemo { pub memo: Memo, pub score: f32 }

/// Pure vector (semantic) recall query — cosine-only scoring (no keyword).
pub struct RecallQuery {
    pub text: String,
    pub top_k: usize,            // 默认 10
    pub score_threshold: f32,
    pub memo_type: Option<MemoType>,
    pub query_embedding: Option<Vec<f32>>, // 预置 query 向量可跳过 embedder
}
// RecallQuery::validate()：text 为空 或 top_k == 0 -> InvalidParam
```

### 2.2 trait（memo-core）
- `MemoStore`：`add` / `get` / `update` / `forget` / `search` / `search_batch`（默认方法：逐条调 `search`，`SqliteStore` 持锁循环 `search_inner` 实现批量）。
- `Embedder`：`fn embed(&self, text: &str) -> Result<Vec<f32>, MemoError>`。
- `StorageBackend`：`open` / `migrate` / 原始 CRUD（抽象，供复制后端扩展）。
- 纯向量召回 `recall` 是 `MemoManager` 方法（复用 `search` 的存储层，令
  `semantic_weight=1` / `keyword_weight=0`），**不在** `MemoStore` trait 上。

### 2.3 MemoManager 公共 API（memo crate）
- `new(embedder: Arc<dyn Embedder>, store: Arc<dyn MemoStore>) -> Self`
- `add(content, memo_type, metadata, importance) -> Result<MemoId>`（自动 embed + 持久化；生成 id、version、时间戳）
- `get(id) -> Result<Option<Memo>>`
- `update(id, patch: MemoPatch) -> Result<()>`（content 变更时重算 embedding、version+1）
- `forget(id) -> Result<bool>`（返回是否删除成功；默认硬删除）
- `search(query) -> Result<Vec<ScoredMemo>>`（语义+关键词混合，过滤 deleted）
- `search_batch(queries: &[SearchQuery]) -> Result<Vec<Vec<ScoredMemo>>>`：批量混合检索，逐条 embed（已预置 `query_embedding` 则跳过）后一次持锁评分，返回与输入等长的结果组。
- `recall_batch(queries: &[RecallQuery]) -> Result<Vec<Vec<ScoredMemo>>>`：批量纯向量召回（内部构造 `semantic_weight=1`/`keyword_weight=0` 的 `SearchQuery` 经 `search_batch`）。
- `recall(query: RecallQuery) -> Result<Vec<ScoredMemo>>`：**纯向量语义召回**，
  嵌入 query 文本后按余弦相似度排序（keyword 权重为 0）；`query.query_embedding`
  已预置时跳过 embedder。`search` 为混合检索，`recall` 为纯向量，二者区分明确。
- `consolidate(id, delta_importance)` / `dedup(threshold)` -> 见 §1.1

### 2.4 CLI（aria-memo，二进制名 `aria-memo` / 命令显示名 `memo`）
- `memo add --type working --content "..." --importance 0.8`
- `memo get --id <id>`
- `memo search --text "..." --top-k 5`
- `memo list [--type ...] [--json]`：新增 `--json` 开关，输出机器可读 JSON 数组（每项含 `id`/`memo_type`/`content`/`importance`/`version`/`metadata`），默认人类可读输出不变（向后兼容）。
- `memo search --text "..." --top-k 5 [--json]`：新增 `--json` 开关，输出 JSON 数组（每项含 `score`/`id`/`content`/`memo_type`），默认 `score\tcontent` 逐行输出不变。
- `memo search-batch --text "q1" --text "q2" [--top-k 5] [--json]`：批量混合检索，重复 `--text` 逐个传入 query；默认逐 query 输出 `score\tcontent`，`--json` 输出二维数组（外层 per-query、内层 `score`/`id`/`content`/`memo_type`）。
- `memo bench --size N --top-k K --warmup W [--batch] [--json]`：新增 `--batch` 开关，在单查询基准之外额外测量「单次锁内批量检索」吞吐（`report["batch"]` 含 `total_ms` / `ops_per_sec`）；其余 JSON 形态不变。
- `memo recall --text "..." --top-k 5`：纯向量召回，输出 `score\tcontent` 逐行；`memo recall --text "..." --top-k 5 --json`：输出 JSON 数组（含 `score`/`id`/`content`/`memo_type`），与 `search` 形态一致。
- `memo update --id <id> [--content "..."] [--type ...] [--importance 0.8]`：按 id 更新记忆（内容变更自动重算 embedding、version+1），至少一项非空；缺失 id / 空 patch / 非法类型或 importance 走 `MemoError`。供 HaluMem 操作级评测。
- `memo forget --id <id>`
- `memo bench --size N --top-k K --warmup W --json`（M2：进程内微基准 JSON）

### 2.5 关系模型（memo-core）

```rust
/// 四视图关系边类型；每条边恰有一个 kind。
pub enum RelationKind { Semantic, Temporal, Causal, Entity }

/// 两记忆间的有向关系边，存于独立表，绝不改动 Memo。
pub struct Relation {
    pub from_id: MemoId,
    pub to_id: MemoId,
    pub kind: RelationKind,
    pub score: f32,         // 边置信度 [0,1]
    pub provenance: String, // 如 "local" / "llm:causal"
    pub created_at: i64,    // unix 秒
}
// Relation::validate()：自环 / score 越界 / 空 provenance -> InvalidParam

/// 有界图检索查询（对齐 Jev-Mem Retrieve->Assess->Expand）。
pub struct GraphRetrieveQuery {
    pub seeds: Vec<MemoId>,
    pub views: Vec<RelationKind>, // 空 = 四视图全开
    pub budget: usize,            // 访问节点上限，默认 60
    pub max_hops: usize,          // 最大遍历深度，默认 3
    pub top_k: usize,             // 返回条数，默认 20
}
// GraphRetrieveQuery::validate()：seeds 空 / budget=0 / max_hops=0 / top_k=0 -> InvalidParam

/// 随图检索返回的可检视决策轨迹（透明决策）。
pub struct RetrieveTrace {
    pub views: Vec<RelationKind>,
    pub budget: usize,
    pub stop_reason: String,
    pub hits: usize,
}
pub struct GraphRetrieveResult { pub items: Vec<ScoredMemo>, pub trace: RetrieveTrace }

/// 共享有界图遍历（BFS + 去环）：neighbor_fn 返回 (to_id, score, kind)；
/// 调用方按 views 过滤、budget 封顶、0.9/hop 衰减；返回 (id, score) 与可读 stop_reason。
pub fn graph_bfs(
    seeds: &[MemoId],
    views: &[RelationKind],
    budget: usize,
    max_hops: usize,
    neighbor_fn: impl FnMut(&MemoId) -> Vec<(MemoId, f32, RelationKind)>,
) -> (Vec<(MemoId, f32)>, String);
```

### 2.6 trait 扩展（memo-core，`MemoStore`）

- `add_relation(&Relation) -> Result<()>`：自环/非法边 → `InvalidParam`；任一端点记忆缺失 → `NotFound`。
- `get_relations(from: Option<&MemoId>, to: Option<&MemoId>, kind: Option<RelationKind>, top_k: usize) -> Result<Vec<Relation>>`：按 `from`/`to`/`kind` 过滤，`top_k` 截断（最旧跳过）。
- `delete_relations(from: Option<&MemoId>, to: Option<&MemoId>, kind: Option<RelationKind>) -> Result<usize>`：返回删除条数；`from`/`to`/`kind` 至少其一，否则 `InvalidParam`。
- `expand(&GraphRetrieveQuery) -> Result<GraphRetrieveResult>`：有界图扩展（各后端复用 §2.5 `graph_bfs`）。

### 2.7 MemoryController 公共 API（memo crate）

- `with_defaults(embedder: Arc<dyn Embedder>, store: Arc<dyn MemoStore>) -> Self`：默认 `LocalRelationScorer` + `RelationConfig::default()`。
- `new(embedder, store, scorer: Arc<dyn RelationScorer>, cfg: RelationConfig) -> Self`：可注入自定义 scorer / config。
- `connect(&Memo) -> Result<usize>`：记忆须已持久化（write→connect 顺序），否则 `NotFound`；推断四视图边并持久化高于阈值的边，返回建边数。对称视图（semantic/entity/causal）双向建边，`temporal` 仅按时间序单向。
- `retrieve(text, views, budget, max_hops, top_k) -> Result<GraphRetrieveResult>`：hybrid search 选种子锚点 → 跨 `views` 有界扩展 → 打分 + 可检视 `RetrieveTrace`。`budget`/`max_hops`/`top_k` 为 0 时 `InvalidParam`。

### 2.8 CLI（关系平面子命令）

- `memo connect --id <id>`：对已存记忆推断四视图边，打印建边数。
- `memo relate --from <id> --to <id> --kind <semantic|temporal|causal|entity> [--score 0.8] [--provenance local]`：手动建边（自环/未知 kind/越界 score 走 `MemoError`）。
- `memo relations [--from <id>] [--to <id>] [--kind <kind>] [--top-k 50] [--json]`：列边（JSON 含 from/to/kind/score/provenance）。
- `memo graph --text "..." [--views semantic,entity] [--budget 60] [--max-hops 3] [--top-k 20] [--json]`：有界多关系遍历；`--json` 输出含 `trace`。
- `memo search --text "..." --top-k 5 --graph`：等价开启图感知检索（`graph` 标志；默认四视图全开、budget 60 / max_hops 3）。

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

`relations` 表（M4，独立边表，不改动 `memories`）：

| 列 | 类型 | 约束 |
|----|------|------|
| from_id | TEXT | NOT NULL（PRIMARY KEY 一部分） |
| to_id | TEXT | NOT NULL（PRIMARY KEY 一部分） |
| kind | TEXT | NOT NULL（`semantic`/`temporal`/`causal`/`entity`，PRIMARY KEY 一部分） |
| score | REAL | NOT NULL，[0,1] |
| provenance | TEXT | NOT NULL |
| created_at | INTEGER | NOT NULL，unix 秒 |

主键：`(from_id, to_id, kind)`（同一方向同种边唯一；对称视图双向各一条）。
索引：`idx_relations_from`（from_id, kind）、`idx_relations_to`（to_id）。

`memories_fts` 表（M5，FTS5 全文索引，用于词法 BM25 候选下推；随 add/update/forget 同步，`migrate` 幂等回填）：
| 列 | 类型 | 约束 |
|----|------|------|
| content | TEXT | 全文索引列（记忆内容） |
| mem_id | TEXT | UNINDEXED，记忆 id（独立表，避免 FTS5 external-content 的 rowid/TEXT-id 映射） |

说明：独立 `memories_fts` 表（非 external-content），`mem_id` 标记为 `UNINDEXED`；`add` 插入后同步写一行，`update` 先删后插，`forget` 删除对应行，`migrate` 以 `id NOT IN (SELECT mem_id FROM memories_fts)` 幂等回填旧数据。写入前内容经 `segment()` 按 Unicode script 预切分（中日韩：jieba-rs/lindera 词级切分；其余语言：unicode61 空格切分），空格拼接存入 `content` 索引列；纯标点/空白 run 丢弃。检索时仅当 `keyword_weight>0` 且 query 非空，才用 FTS5 `MATCH` + `bm25()` 取 `top_k*5 max 50` 命中点作候选集（query 同样经 `segment()` 切词后 OR 连接，与索引 token 对齐；标点/空白 run 丢弃，回退全表扫描）；候选集内再做 Rust 内存余弦精排，词法命中更早剪枝候选。

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
- 关系平面（M4）复用上述变体：`add_relation`/`connect` 遇自环、score 越界（非 [0,1]）、空 provenance → `InvalidParam`；端点记忆缺失 → `NotFound`。`delete_relations`/`GraphRetrieveQuery`/`MemoryController::retrieve` 在缺过滤条件或 `budget`/`max_hops`/`top_k`/`seeds` 为 0 / 空时 → `InvalidParam`。未知 `relation_kind` 字符串 → `InvalidParam`。

所有路径禁止静默失败；每条异常路径须有单测覆盖。

## 5. 验收标准（M1）

- `cargo test` 全绿（正常 + 异常用例）。
- `cargo clippy --all-targets` 无告警。
- 交叉编译：`cargo build --target aarch64-linux-android` 通过（或 `wasm32-unknown-unknown -p memo-core -p memo-embed`）。
- 黄金路径单测：`add → search → get` 端到端跑通。
- 异常单测：重复 id、缺失、空内容、空嵌入、非法参数（importance 越界 / top_k=0 / query 文本空）、损坏/非法 metadata DB、recall/search 拒绝非法 query。
- 覆盖率：核心逻辑（manager / search / recall / consolidate / dedup / lifecycle / storage / cli）均有正常 + 异常用例；纯向量召回 `recall` 与 `RecallQuery` 校验有单测。

### 5.1 M4 验收标准（多关系记忆平面）

- `cargo test` 全绿、`cargo clippy --all-targets` 无告警（含关系平面用例）。
- 黄金路径单测：`add → connect` 推断四视图边 → `get_relations` 列边 → `graph`/`retrieve` 跨视图扩展返回打分记忆 + `RetrieveTrace` 端到端跑通。
- 异常单测（须覆盖正常 + 异常）：
  - `Relation::validate`：自环、score 越界（<0 / >1）、空 provenance。
  - `GraphRetrieveQuery::validate`：seeds 空、budget=0、max_hops=0、top_k=0。
  - `add_relation`：端点缺失 → `NotFound`；自环 → `InvalidParam`。
  - `delete_relations`：from/to/kind 全空 → `InvalidParam`；按过滤正确删数。
  - `expand`/`retrieve`：拒绝非法 query；budget 封顶生效、max_hops 生效、去环（不重复访问）、0.9/hop 衰减；views 空 = 四视图全开。
  - `LocalRelationScorer`：四视图打分在 [0,1]（semantic=cosine / temporal=时间序 / entity=token Jaccard / causal=时间邻+重叠）。
  - CLI：`relate` 拒未知 kind / 自环 / 越界 score；`relations`/`graph` 正常 + `--json` 形态；`connect` 对未存 id → `NotFound`；`search --graph` 等价图感知。
- 回归：既有扁平 `search`/`recall`（纯向量）与 `Memo` 模型行为不变，作为图检索回退。

### 5.2 M7 验收标准（多语言词级切分）

- `cargo test` 全绿、`cargo clippy --all-targets` 无告警（含多语言用例）。
- 中文子词命中：`search --text "香蕉"` 命中含「小明爱吃香蕉和橘子」的记忆（jieba 整词切分，修复 ICU4X 误切）。
- 日语形态切分：`search` 以日语词级 token 命中对应记忆（`japanese_keyword_search` 单测）。
- 韩语形态切分：`search` 以韩语词级 token 命中对应记忆（`korean_keyword_search` 单测）。
- 路由正确性：`multilingual_segment_routing` 单测验证 `segment()` 对中/日/韩走形态分词、对空格型语言（en 等）保留原词、对纯标点/空白返回空（被 `is_index_term` 丢弃）。
- 迁移：既有数据库（user_version < 4）打开时 `migrate` 重建 `memories_fts` 并回填为新的多语言切分 token；中文既有命中不被破坏。
- 文档：README/README_cn、AGENTS.md、本文件 §1.4/§3 与 task.md M7 同步。

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

**A1 微基准（延迟/资源 / 扩展曲线 / 写长尾）**

- 规模：**多尺寸 sweep** `1k` / `10k` / `100k`（CLI `--sizes`，默认 `1000,10000,100000`）；`search` top-k ∈ {5, 10}。
- 扩展曲线：对每个 size 测 `add` / `search` 的 p50 / p99（ms）与吞吐（ops/s），跨尺寸计算 **p99 增长因子**（`p99@10k / p99@1k`、`p99@100k / p99@10k`），报告亚线性判定（增长因子 < 尺寸倍率即亚线性）。
- 写长尾压测：aria CLI `bench` 新增 `--wal` / `--batch-embed` / `--bulk`，分别报告 `add_baseline`（默认 journal + 逐条嵌入+事务）、`add_wal`（WAL 模式）、`add_batch_embed`（`add_batch` 批量嵌入+事务合并）、`add_bulk`（逐条嵌入+`add_batch` 事务合并）的 p50/p99/ops，观察 add p99 是否收敛。
- 本地控制组：**`sqlite_vec`**（pip `sqlite-vec`，本地 SQLite 向量索引 + 离线哈希-n-gram 嵌入）与 **`chromem`**（chromem-go 二进制子进程，离线）纳入默认 `--systems`，缺依赖/二进制时 skip 并写 `reason`，让对比矩阵有真实数值。
- 他系统：经 `benches/adapters/*` 调用；缺依赖/密钥时 skip 并写入报告原因，不得静默失败。

**A2 合成检索质量**

- 查询集 ≥50–100 条、贴近真实分布：`synthetic_retrieval.json`（小，~8 条）+ `synthetic_retrieval_v2.json`（新增，84 条：关键词/同义改写/干扰项混合）；并支持复用 **Track B 真实数据集**作为查询语料（`--a2-dataset track_b:locomo_refined` / `track_b:halumem`，证据/记忆点作相关文档）。
- 指标：**Recall@k、MRR 与各自方差（样本标准差）**；`run_retrieval_quality` 输出 `recall_at_k` / `recall_at_k_std` / `mrr` / `mrr_std` / `n_queries`，给出有统计意义的检索质量。

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
    datasets.py       # A2 查询集加载（synthetic / synthetic_v2 / track_b:*）
  track_b/            # locomo_refined / halumem / longmemeval / personamem 注册表 + 子包
  metrics/            # f1 / bleu / 多选 / recall@k / mean_std 等离线指标
  judge.py            # OpenAI 兼容 judge 客户端（可选）
  datasets.py         # 数据集定位与下载
  adapters/           # aria / sqlite_vec / chromem / mem0 / memos / mempalace / zep / letta
  data/               # synthetic_retrieval.json + synthetic_retrieval_v2.json；fixtures/ 各基准极小合成样例
  tests/              # Python 单测（离线、零网络、零真实 LLM）
  results/            # 生成结果（样例可入库，大体量 gitignore）
```

### 6.6 CLI 增补（供 Track A）

- `memo bench --size N --top-k K --warmup W --json`：进程内跑测，stdout 打印 JSON（含 `add`/`search` p50/p99/ops + 可选 `batch` 搜索吞吐）。
- 写长尾开关：`--wal`（WAL 模式，报告 `add_wal`）、`--batch-embed`（报告 `add_batch_embed`）、`--bulk`（报告 `add_bulk`）；均构造独立分段，便于对照 `add_baseline` 观察 add p99 收敛。
- 不引入 criterion / `crates/bench`。

### 6.7 M2 验收标准

- `docs/compare.md` 覆盖五系统 + aria，维度齐全。
- `python benches/run.py --track a` 在默认小规模下可复现；结果写入 `benches/results/`。
- Track A 合成检索对 aria 产出 Recall@k / MRR 数值。
- `python benches/run.py --track b --dry-run` 能走通四基准加载与能力探测；真实打分在 fixture 下离线可跑（F1/BLEU/多选/Recall@k 出数值），judge 指标在缺密钥时 skip 并写原因。
- `cargo test` / clippy 仍全绿；新增 CLI `update` 与 `list --json` / `search --json` 有单测，默认输出形态不变（既有断言不破坏）。
- 五系统 adapter 均存在且实现同一基类接口；不可用时报告 N/A + 原因。
- 下载脚本在缺网络时抛错并打印手动指引，不静默失败。

### 6.8 M6 验收标准（Track A 扩展：规模/控制组/方差/写长尾）

- `run.py --track a --sizes 1000,10000,100000` 产出 A1 多尺寸报告，含 `scaling`（p99 增长因子 + 亚线性/线性/超线性判定）。
- 默认 `--systems` 含 `sqlite_vec` / `chromem`；缺依赖/二进制时 skip 并写 `reason`（不伪造数值）。
- A2 产出 `recall_at_k` / `recall_at_k_std` / `mrr` / `mrr_std` / `n_queries`；`--a2-dataset synthetic_v2` 查询数 ≥50，`track_b:*` 能复用真实数据集（缺数据则 skip 并写 reason）。
- `aria-memo bench --wal --batch-embed --bulk --json` 报告 `add_baseline` / `add_wal` / `add_batch_embed` / `add_bulk` 四分段 p50/p99/ops。
- `cargo test` / `cargo clippy --all-targets` 全绿；benches `python -m unittest tests.test_metrics tests.test_adapters tests.test_track_a` 全绿。
