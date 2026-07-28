# task.md — memory 端侧长期记忆存储 M1 实施清单

> 由 `requirements.md`（已人工审核通过）生成的分步清单。M1 已落地脚手架、AGENTS.md、requirements.md 审核。
> 统一验收基线：`cargo test` 全绿、`cargo clippy --all-targets` 无告警、交叉编译（wasm32）通过。

## 分步实施

1. [x] 初始化 cargo workspace 与 5 crate 脚手架（core/storage/embed/memory/cli）+ 基线，`cargo build` 通过
2. [x] 产出 `AGENTS.md`（≤100 行）与 `requirements.md` 供人工审核
3. [x] 人工逐项审核通过 `requirements.md`，生成本 `task.md`
4. [x] 实现 `memory-core`：`MemoryError`、数据模型（`Memory`/`MemoryType`/`SearchQuery`/`ScoredMemory`/`MemoryPatch`）与校验、`MemoryStore`/`Embedder`/`StorageBackend` trait、`generate_id`/`now_secs`/`keyword_score` 工具 + 单测（正常+异常）
5. [x] 实现 `memory-embed`：`LocalEmbedder`（ngram+哈希/TF-IDF 向量）、`cosine` + 单测（正常+异常：空文本/零向量/相似度）
6. [x] 实现 `memory-storage`：`SqliteStore`（建表/迁移/索引/CRUD/批量/混合检索）、`StorageBackend` 实现与 `ReplicatedBackend` 占位 + 单测（重复 id/缺失/空内容/损坏 DB/损坏 BLOB）
7. [x] 实现 `memory`：`MemoryManager`（add/get/update/forget/search/consolidate/dedup）、`lifecycle`（decay/prune）+ 单测（正常+异常，端到端黄金路径）
8. [x] 实现 `aria-memory`：add/get/search/list/forget 命令与默认 `LocalEmbedder` + `SqliteStore` + 单测
9. [x] 验收：`cargo test` 全绿（38 tests）、`cargo clippy --all-targets` 无告警、`cargo build --target wasm32-unknown-unknown -p memory-core -p memory-embed` 通过

## 验证

- `cargo test`：覆盖正常 + 异常路径（各 crate，共 38 个测试全绿）。
- `cargo clippy --all-targets`：无告警。
- 黄金路径：`add → search → get` 端到端单测。
- 异常单测：重复 id、缺失、空内容、空嵌入、非法参数、损坏 DB 打开失败、损坏 BLOB 读取。
- 交叉编译：wasm32-unknown-unknown 下 `memory-core` / `memory-embed` 编译通过（纯 Rust 层，边缘/Web 就绪）。
