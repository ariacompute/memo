# memo

[English](README.md) | [中文](README_cn.md)

Rust 实现的边缘/移动端本地优先（local-first）长期记忆存储，为 LLM Agent 提供记忆的增删改查、语义 + 关键词混合检索、巩固、去重与遗忘。零网络依赖、纯 Rust（不引入重型 ML 框架）。还内置默认离线的多关系记忆平面（semantic/temporal/causal/entity 四视图）。

参考：rqlite / turso（嵌入式持久化）、MemOS / mem0 / MemPalace（记忆管理）。

## 分层架构

分层 cargo workspace（trait 解耦）：

```
cli(aria-memo) → memo(编排) → storage(SQLite) / embed(本地嵌入) → core(模型/错误/trait)
```

### 检索（Search & retrieval）

混合检索融合语义（本地嵌入余弦）与词法（关键词）相关性：
`score = semantic_weight·cosine + keyword_weight·lexical_relevance`。

- **词法下推（FTS5）。** `memories.content` 由 SQLite FTS5 虚拟表 `memories_fts` 索引。混合查询时，存储层先在 *SQLite 内部* 跑 `MATCH` + `bm25()` 把候选集剪枝到词法命中最高的若干条，再仅对这小集合计算余弦——彻底消除了旧有的全表扫描。纯语义查询（或 FTS5 无词法命中）回退到全表扫描，保证召回不丢。**多语言词级切分：** 由于本项目的 bundled SQLite 无法开启内置 `icu` 分词器，写入前会在 Rust 侧做词级预切分、查询侧用同一 `segment` 逻辑切分，使子词命中而非整段当一个不可匹配的 token。路由按 Unicode 文字范围：中文 `zh/zh-TW` 用 `jieba-rs`，日语 `ja` 与韩语 `ko` 用 `lindera`（内嵌 ipadic/ko-dic 词典），其余按空格切分的语言（en/es/fr/de/it/ru/pt/ar/hi）走空格切分——覆盖 cockpit 的全部 13 种语言。FTS 索引在打开时通过 `PRAGMA user_version` 一次性重建（多语言路由已升到 4），使已有数据库也用上切分后的 token。
- **更高的词法权重。** `SearchQuery.keyword_weight` 默认 `0.5`（原 `0.3`），词法命中更早参与剪枝与排序；`semantic_weight` 保持 `0.7`。`MemoryController` 同样使用 `0.7 / 0.5`。
- **批量检索。** `MemoStore::search_batch` / `MemoManager::search_batch` / `recall_batch` 在单次连接锁内（一次 FTS5 准备 + 一次扫描）对多个查询打分，返回 `Vec<Vec<ScoredMemo>>`。CLI 提供 `aria-memo search-batch --text … --text …` 与 `aria-memo bench --batch`（报告 `batch.ops_per_sec`）。

## 快速开始

```bash
cargo build
cargo test
cargo run -p aria-memo -- add --type working --content "用户喜欢 Rust" --importance 0.8
cargo run -p aria-memo -- search --text "Rust" --top-k 5
```

## CLI 与自管理

`aria-memo` 内置两个自管理命令（对齐 `aria-router`）：

- `aria-memo setup` — 将 CLI 配置持久化到 `~/.ariacompute/memo-cli.yml`
  （`upgrade_url`，即 Releases 组织根）。交互运行时会提示选择 **GitHub**
  （`https://github.com/ariacompute`，默认）或 **Gitee**
  （`https://gitee.com/ariacompute`）。非交互运行时遵循 `ARIA_MEMO_SITE=cn`
  （→ Gitee），否则沿用已有配置，最后回落到 GitHub 默认。
  - `aria-memo setup --status` — 打印配置路径与 `upgrade_url`。
  - `aria-memo setup --clear` — 删除配置文件。
- `aria-memo upgrade [version]` — 从 GitHub/Gitee Releases 下载并原地替换当前
  二进制（不传版本升级到最新稳定版，传 `version` 升级到指定 tag）。
  - `aria-memo upgrade --url <URL>` — 本次运行覆盖配置中的 `upgrade_url`。
  - `upgrade_url` 解析优先级：`--url` > `~/.ariacompute/memo-cli.yml`(优先读取)
    > `ARIA_MEMO_UPGRADE_URL` 环境变量 > 内置默认（`https://github.com/ariacompute`）。
  - Release 资产命名约定：`aria-memo_{ver}_{os}.tar.gz`
    （`os` ∈ `linux_x86_64`/`linux_arm64`/`macos`/`windows_x86_64`，Windows 为 `.zip`）。

`aria-memo --version` / `-v` 打印构建版本。

> home 目录 `~/.ariacompute` 可由 `ARIA_COMPUTE_HOME` 覆盖。

## 多关系记忆平面（Jev-Mem 启发）

在扁平 `MemoStore` 之上构建的结构化多关系记忆层——零网络依赖、默认离线（可插拔 LLM `RelationScorer`）。

四种关系视图以有向边连接记忆：

- `semantic` — 语义关联（嵌入余弦）
- `temporal` — 时间序 / 共现
- `causal` — 因果链
- `entity` — 同一实体聚合

边存于独立的 `relations` 表，绝不改动 `Memo`。写入时 `connect` 推断四视图边；读取时 `retrieve` 先做混合检索选种子锚点，再跨视图做有界图扩展，返回打分记忆 + 可检视 `RetrieveTrace`（对齐 Jev-Mem 的 Retrieve→Assess→Expand 与透明决策）。

```bash
# 为已存记忆推断四视图边
cargo run -p aria-memo -- connect --id <id>

# 手动建边
cargo run -p aria-memo -- relate --from <id> --to <id> --kind semantic --score 0.8

# 列边（按 from / to / kind 过滤）
cargo run -p aria-memo -- relations --kind semantic --json

# 有界多关系遍历（views/budget/max-hops/top-k）
cargo run -p aria-memo -- graph --text "Rust 系统编程" --views semantic,entity --top-k 20 --json

# 或在普通 search 上开启图感知
cargo run -p aria-memo -- search --text "Rust" --graph
```

默认 `LocalRelationScorer` 完全本地（semantic = 余弦，temporal = 时间序，entity = token Jaccard，causal = 时间邻 + 重叠）。可换 LLM 实现而不动控制器。

**离线检索质量。** 扁平 `search` 走混合检索：本地语义分（哈希嵌入的余弦）+ 零依赖的词法分
（`lexical_relevance`：按稀有度加权的词项重叠 + 精确短语奖励，并含 CJK 字符 2-gram）。由于内置
嵌入器是轻量 hash/TF-IDF 向量器（不引入重型 ML），语义信号刻意偏弱；`lexical_relevance` 最大化
关键词贡献，使离线召回仍可用。可用 `SearchQuery.keyword_weight`（默认 `0.5`）调节平衡。

## 对比评测

对比系统：mem0 / MemOS / MemPalace / Zep / Letta，外加本地嵌入式**控制组** `sqlite_vec` 与
`chromem`（默认纳入；其依赖/二进制缺失时按 `reason` 跳过并写原因，绝不伪造数值）。

- 功能矩阵：[docs/compare.md](./docs/compare.md)
- 评测说明与结果：[docs/bench_results.md](./docs/bench_results.md)
- Python 编排（Track A 存储/检索 + Track B 端到端质量）：[benches/README.md](./benches/README.md)

```bash
pip install -r benches/requirements.txt

pushd benches/chromem-wrapper
go get github.com/philippgille/chromem-go@latest && go mod tidy && go build -o chromem .
export CHROMEM_BIN="$PWD/chromem"
popd

python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem
python benches/run.py --track b --download
```

### Track A 评测报告

运行：`python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem`
（结果位于 `benches/results/`）。本报告数据：
- aria：`python benches/run.py --track a --sizes 1000,10000,100000 --systems aria`
- sqlite_vec：`python benches/run.py --track a --sizes 1000,10000,100000 --systems sqlite_vec`
- chromem：`python benches/run.py --track a --sizes 1000,10000,100000 --systems chromem`

**A1 — 微基准（top_k=5，离线；aria `add_baseline` = 逐条嵌入 + 逐条事务）：**

| 系统 | 尺寸 | 分段 | ops/秒 | p50 (ms) | p99 (ms) | 说明 |
|------|------|------|-------:|---------:|---------:|------|
| aria | 1000 | `add_baseline` | 157.18 | 5.91 | 13.35 | 朴素逐条路径（基线） |
| aria | 1000 | `search` | 1456.11 | 0.69 | 0.69 | 单 query 均摊 |
| aria | 10000 | `add_wal` | 752.06 | 1.06 | 4.08 | WAL 日志模式 |
| aria | 10000 | `add_batch_embed` | 40039.03 | 0.02 | 0.02 | 批量嵌入 + 事务写入 |
| aria | 10000 | `add_bulk` | 72164.68 | 0.01 | 0.01 | 逐条嵌入，事务合并 |
| aria | 10000 | `search` | 140.71 | 7.11 | 7.11 | 单 query 均摊 |
| aria | 100000 | `add_batch_embed` | 41459.87 | 0.02 | 0.02 | 100k 仅测此优化分段 |
| aria | 100000 | `search` | 13.65 | 73.25 | 73.25 | 单 query 均摊（全语料扫描） |
| sqlite_vec | 1000 | `add` | 23345.52 | 0.04 | 0.04 | 批量插入 |
| sqlite_vec | 1000 | `search` | 592.15 | 1.69 | 1.69 | 单 query 均摊 |
| sqlite_vec | 10000 | `add` | 22223.95 | 0.04 | 0.04 | 批量插入 |
| sqlite_vec | 10000 | `search` | 50.42 | 19.84 | 19.84 | 单 query 均摊 |
| sqlite_vec | 100000 | `add` | 20212.36 | 0.05 | 0.05 | 批量插入 |
| sqlite_vec | 100000 | `search` | 4.58 | 218.25 | 218.25 | 单 query 均摊（全语料扫描） |
| chromem | 1000 | `add` | 22126.47 | 0.05 | 0.05 | 批量插入（`add-batch`） |
| chromem | 1000 | `search` | 208.95 | 4.79 | 4.79 | 单 query 均摊（`query-batch`） |
| chromem | 10000 | `add` | 23672.96 | 0.04 | 0.04 | 批量插入（`add-batch`） |
| chromem | 10000 | `search` | 23.08 | 43.34 | 43.34 | 单 query 均摊（`query-batch`） |
| chromem | 100000 | `add` | 23089.77 | 0.04 | 0.04 | 批量插入（`add-batch`） |
| chromem | 100000 | `search` | 2.33 | 429.42 | 429.42 | 单 query 均摊（`query-batch`） |

**A2 — 检索质量（synthetic_v2，84 条查询，top_k=5）：**

| 系统 | Recall@5 | MRR | 查询数 | 离线 |
|------|---------:|----:|-------:|-----:|
| aria | 1.00 | 1.00 | 84 | true |
| sqlite_vec | 0.9762 | 0.9762 | 84 | true |
| chromem | 1.00 | 1.00 | 84 | true |

### Track B 评测报告

四个基准的端到端记忆质量评测：`locomo_refined`、`halumem`、`longmemeval`、`personamem`。
aria 后端通过 CLI 驱动（先构建：`cargo build -p aria-memo --release`）。

```bash
# 可选：打分前先验证数据集加载与后端能力
python benches/run.py --track b --dry-run

# 真实数据集（自动下载；网络不通时明确报错并给出手动指引）
python benches/run.py --track b --download
```

离线指标（F1/BLEU、多选、检索 Recall@k、抽取 Recall）**无需 LLM** 即可运行。
LLM judge 指标为**可选**：无凭据时静默跳过：

```bash
export BENCH_LLM_API_KEY=sk-...
export BENCH_LLM_BASE_URL=https://tokenhub.tencentmaas.com
python benches/run.py --track b --download --judge-model hy3
```

> 各基准在缺少真实数据时会回退到仓库内置 `benches/data/fixtures/`（合成样本，离线冒烟），报告中以 `dataset_source` 标注。可用 `--benchmarks`、`--limit`、`--ingest-only` 限定评测范围。

**限定大型真实数据集。** 真实数据集可能非常大——例如 `halumem`（HaluMem-Medium）约需 7.5 万次 `add` 调用。`--limit N` 限定每个基准**注入与评测**的样本数（对 `halumem` 每个记录为一个样本集；对 `locomo_refined`/`longmemeval`/`personamem` 则限制问题/条目数）。省略 `--limit` 时，默认对每个基准施加 `50` 样本上限（会打印到 stderr），避免无界运行卡死。注入过程会向 stderr 输出进度（`[bench] ingest 500/N ... ingest done`）。

```bash
# 有界的快速冒烟运行
python benches/run.py --track b --limit 2
# 全量运行（耗时但受上限约束）；构建 release 二进制可加速每次 add
cargo build -p aria-memo --release
python benches/run.py --track b --benchmarks halumem
```



## 目录

- `crates/core` — 数据模型、统一错误 `MemoError`、trait
- `crates/embed` — 本地轻量 embedder（ngram + 哈希/TF-IDF 向量）+ 余弦相似度
- `crates/storage` — rusqlite 嵌入式持久化后端
- `crates/memo` — 记忆管理编排与生命周期（含多关系 `MemoryController` / `RelationScorer`）
- `crates/cli` — 命令行入口
- `benches/` — 业界对比评测
- `docs/` — 功能矩阵与评测结果说明

## 工程规范

本仓库遵循 Harness Engineering 理念：

- [`AGENTS.md`](AGENTS.md)：Agent 工程上下文入口与目录索引
- [`requirements.md`](requirements.md)：需求规格（功能边界/异常/验收标准，人工审核制）
- [`task.md`](task.md)：实施任务清单
