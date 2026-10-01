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

- **词法下推（FTS5）。** `memories.content` 由 SQLite FTS5 虚拟表 `memories_fts` 索引。混合查询时，存储层先在 *SQLite 内部* 跑 `MATCH` + `bm25()` 把候选集剪枝到词法命中最高的若干条，再仅对这小集合计算余弦——彻底消除了旧有的全表扫描。纯语义查询（或 FTS5 无词法命中）回退到全表扫描，保证召回不丢。
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
python benches/run.py --track a --size 1000
python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem
python benches/run.py --track b --dry-run
```

### Track A 评测报告

运行：`python benches/run.py --track a --size 1000`
（结果位于 `benches/results/`，例如 `20261001T012416Z/track_a.json`）。

**A1 — 微基准（aria-memo，size=1000，top_k=5，离线）：**

| 操作    | ops/秒 | p50 (ms) | p99 (ms) |
|---------|-------:|---------:|---------:|
| `add`   | 298.36 | 2.98     | 9.77     |
| `search`| 434.66 | 2.19     | 3.93     |

**A2 — 检索质量（synthetic_retrieval.json，8 条查询，top_k=5）：**

| 系统  | Recall@5 | MRR  | 查询数 | 离线  |
|-------|---------:|-----:|-------:|:-----:|
| aria  | 1.00     | 1.00 | 8      | true  |

> A1 为进程内（离线）微基准；A2 在合成数据集上衡量混合（语义 + 关键词）检索质量。完整对比矩阵见 [docs/compare.md](./docs/compare.md)。

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
python benches/run.py --track b --judge-model hy3
```

> 各基准在缺少真实数据时会回退到仓库内置 `benches/data/fixtures/`（合成样本，离线冒烟），报告中以 `dataset_source` 标注。可用 `--benchmarks`、`--limit`、`--ingest-only` 限定评测范围。

> **超时与进度（避免静默卡死）。** 每次 aria-memo CLI 调用有 120s 超时（可用 `ARIA_MEMO_TIMEOUT` 覆盖）；LLM judge 每次调用 30s 超时（可用 `BENCH_LLM_TIMEOUT` 覆盖）。两者都会向 stderr 输出 `[bench]`/`[judge]` 进度，长任务可见其进行。使用 `--judge-model` 需设置 `BENCH_LLM_API_KEY`（自托管模型如 `hy3` 还需 `BENCH_LLM_BASE_URL`）；未设置时 judge 指标跳过，仅跑离线指标。记忆密集的子任务（如 `halumem` extraction）会对每条记忆发起一次 judge 调用——请用 `--limit` 控制规模。
> **Judge 错误会暴露而非吞掉。** judge 调用失败（`BENCH_LLM_BASE_URL`/`API_KEY` 错误、模型不存在、限流或响应格式不兼容）会打印 `[judge] ERROR (first): …`，含 HTTP 状态码与响应体（或异常信息）**以及所有尝试过的 URL**，之后每 10 次错误汇总一次。失败调用视为不可判定并跳过——运行仍会结束。judge 会：(a) HTTP 404 时自动重试另一种 `/v1` 挂载；(b) 对瞬时失败（超时 / HTTP 429·5xx）做指数退避重试，**且重试时把单次超时按 1×→2×→4×→8× 放大**；(c) 已发送 `stream: false` 并兼容 chat/completion/streaming 三种响应形态。可用 `BENCH_LLM_TIMEOUT`（单次秒数，默认 30）与 `BENCH_LLM_RETRIES`（默认 2）调参。若仍高错误率，说明端点太慢或配置有误——对有长输入提示的基准（如 `halumem`）请调大 `BENCH_LLM_TIMEOUT`（如 `120`）。

**限定大型真实数据集。** 真实数据集可能非常大——例如 `halumem`（HaluMem-Medium）约需 7.5 万次 `add` 调用。`--limit N` 限定每个基准**注入与评测**的样本数（对 `halumem` 每个记录为一个样本集；对 `locomo_refined`/`longmemeval`/`personamem` 则限制问题/条目数）。省略 `--limit` 时，默认对每个基准施加 `50` 样本上限（会打印到 stderr），避免无界运行卡死。注入过程会向 stderr 输出进度（`[bench] ingest 500/N ... ingest done`）。

```bash
# 有界的快速冒烟运行
python benches/run.py --track b --limit 2
# 全量运行（耗时但受上限约束）；构建 release 二进制可加速每次 add
cargo build -p aria-memo --release
python benches/run.py --track b --benchmarks halumem
```

**离线结果 —— 本次运行（`20261001T014217Z/track_b.json`，真实数据集，judge 不可用）：**

四个基准均在仓库内置真实数据集上运行；LLM judge **未**可用（`BENCH_LLM_API_KEY` 缺失 → `judge.available=false`，0 次调用），因此所有依赖 LLM 的指标均为 `skipped`。仅汇报离线指标：

| 基准 | 离线指标 | 数值 | 子集 / 说明 |
|------|---------|------:|------------|
| `locomo_refined` | F1 | 0.008 / 0.006 / 0.026 | 子集 1/2/3（无 LLM 答案生成） |
| `locomo_refined` | BLEU | 0.004 / 0.003 / 0.014 | 子集 1/2/3 |
| `locomo_refined` | judge_accuracy | skipped | 无 LLM judge |
| `halumem` | retrieval_recall@5 | 1.00 | extraction 子集 —— 检索正常 |
| `halumem` | retrieval_recall@5 | 0.00 | qa 子集 |
| `halumem` | memory_recall / memory_accuracy / false_memory_resistance / f1 / qa_accuracy | skipped | 无 LLM judge |
| `longmemeval` | retrieval_recall@5 | 0.00 | 离线 |
| `longmemeval` | qa_accuracy | skipped | 无 LLM judge |
| `personamem` | multiple_choice_accuracy | 0.00 | 离线（无 LLM 作答） |

小结：在**无 LLM judge** 条件下，Track B 无法对生成式 QA 质量打分——离线指标仅确认 `halumem` extraction 的检索可用（recall@5 = 1.00），其余检索/多选信号接近 0；`halumem` 的 `updating` 子集指标也因该数据集无 update 类问题而 skipped。有意义的质检分数需要开启 LLM-judge 路径（设置 `BENCH_LLM_API_KEY`）或扩充离线信号集。

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
