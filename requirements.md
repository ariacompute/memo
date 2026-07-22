# requirements.md — memory 端侧长期记忆存储（M1）

> 功能边界 / API / 表结构 / 异常 / 验收标准。须经人工逐项审核后，方可据其生成 task.md 实施。

## 1. 功能边界

### 1.1 范围内（M1）
- 三层记忆模型：Working / ShortTerm / LongTerm（episodic | semantic | entity | graph）。
- 记忆条目 CRUD：`add` / `get` / `update` / `forget`。
- 本地嵌入：ngram + 哈希/TF-IDF 向量表示，余弦相似度；可注入自定义 `Embedder`。
- 持久化：嵌入式 SQLite（rusqlite bundled），自动建表/迁移/索引、批量写入。
- 检索：语义（向量余弦）+ 关键词（LIKE）混合打分，支持 top-k 与阈值。
- 记忆管理：`consolidate`（巩固：提升重要性/合并）、`dedup`（去重：相似度阈值合并）。
- 生命周期：分层老化、重要性衰减、遗忘（默认硬删除，预留软删除标记）。
- 统一错误 `MemoryError`；可选 CLI（add/get/search/list/forget）。
- 全部新增功能同步单测，核心逻辑覆盖正常 + 异常路径。

### 1.2 范围外（M1，列为后续里程碑）
- 分布式/复制后端（Raft，rqlite 灵感）—— 仅 `StorageBackend` trait 抽象预留。
- libSQL/turso 同步/复制后端（feature 占位）。
- 云端协同 / 多端同步。
- GPU/NEON 向量加速、ANN 索引（HNSW 等）。
- 真实 LLM 提取/摘要（仅预留接口，M1 用规则 + 本地 embedder）。

## 2. API

### 2.1 数据模型（memory-core）
```rust
pub type MemoryId = String;

pub enum MemoryType { Working, ShortTerm, LongTerm { kind: LongTermKind } }
pub enum LongTermKind { Episodic, Semantic, Entity, Graph }

pub struct Memory {
    pub id: MemoryId,
    pub memory_type: MemoryType,
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
    pub memory_type: Option<MemoryType>,
}

pub struct ScoredMemory { pub memory: Memory, pub score: f32 }
```

### 2.2 trait（memory-core）
- `MemoryStore`：`add` / `get` / `update` / `forget` / `search`。
- `Embedder`：`fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError>`。
- `StorageBackend`：`open` / `migrate` / 原始 CRUD（抽象，供复制后端扩展）。

### 2.3 MemoryManager 公共 API（memory crate）
- `new(embedder: Arc<dyn Embedder>, store: Arc<dyn MemoryStore>) -> Self`
- `add(content, memory_type, metadata, importance) -> Result<MemoryId>`（自动 embed + 持久化；生成 id、version、时间戳）
- `get(id) -> Result<Option<Memory>>`
- `update(id, patch: MemoryPatch) -> Result<()>`（content 变更时重算 embedding、version+1）
- `forget(id) -> Result<bool>`（返回是否删除成功；默认硬删除）
- `search(query) -> Result<Vec<ScoredMemory>>`（语义+关键词混合，过滤 deleted）
- `consolidate(id, delta_importance)` / `dedup(threshold)` -> 见 §1.1

### 2.4 CLI（memory-cli，二进制名 `memory`）
- `memory add --type working --content "..." --importance 0.8`
- `memory get --id <id>`
- `memory search --text "..." --top-k 5`
- `memory list [--type ...]`
- `memory forget --id <id>`

## 3. 表结构（SQLite）

`memories` 表：
| 列 | 类型 | 约束 |
|----|------|------|
| id | TEXT | PRIMARY KEY |
| memory_type | TEXT | NOT NULL（如 `working` / `short_term` / `long_term:episodic`） |
| content | TEXT | NOT NULL |
| embedding | BLOB | 长度前缀序列化的 f32 向量（可为空） |
| metadata | TEXT | JSON 字符串 |
| importance | REAL | NOT NULL，[0,1] |
| version | INTEGER | NOT NULL |
| created_at | INTEGER | NOT NULL |
| updated_at | INTEGER | NOT NULL |
| deleted | INTEGER | NOT NULL DEFAULT 0（软删除标记） |

索引：`idx_memories_type`、`idx_memories_updated_at`、`idx_memories_deleted`。

## 4. 异常（MemoryError，thiserror）

- `Io(#[from] std::io::Error)`：IO 失败。
- `Db(String)`：SQLite 操作失败。
- `NotFound(MemoryId)`：`get`/`update`/`forget` 目标不存在。
- `DuplicateId(MemoryId)`：`add` 重复 id。
- `EmptyContent`：内容为空。
- `EmptyEmbedding`：嵌入为空/零长。
- `InvalidParam(String)`：参数非法（importance 越界、top_k=0、query 文本空）。
- `Serialization(String)`：JSON/向量序列化失败。
- `Embedding(String)`：嵌入计算失败。
- `Other(String)`：兜底。

所有路径禁止静默失败；每条异常路径须有单测覆盖。

## 5. 验收标准

- `cargo test` 全绿（正常 + 异常用例）。
- `cargo clippy --all-targets` 无告警。
- 交叉编译：`cargo build --target aarch64-linux-android` 通过（或 `wasm32-unknown-unknown -p memory-core -p memory-embed`）。
- 黄金路径单测：`add → search → get` 端到端跑通。
- 异常单测：重复 id、缺失、空内容、空嵌入、非法参数、损坏 DB 打开失败。
- 覆盖率：核心逻辑（manager / search / consolidate / dedup / lifecycle）均有正常 + 异常用例。
