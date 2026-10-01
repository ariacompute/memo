# memo

[English](README.md) | [中文](README_cn.md)

Local-first long-term memory store for LLM Agents, built in Rust for edge/mobile deployment.
Provides CRUD, semantic + keyword hybrid retrieval, consolidation, deduplication, and forgetting —
with zero network dependency and zero heavy ML frameworks. Also ships an offline-first,
multi-relational memory plane (four relation views: semantic/temporal/causal/entity).

Inspired by: rqlite / turso (embedded persistence), MemOS / mem0 / MemPalace (memo management).

## Architecture

Layered cargo workspace (trait-decoupled):

```
cli(aria-memo) → memo(orchestration) → storage(SQLite) / embed(local embedding) → core(models/errors/traits)
```

### Search & retrieval

Hybrid search blends semantic (cosine over local embeddings) and lexical (keyword) relevance:
`score = semantic_weight·cosine + keyword_weight·lexical_relevance`.

- **Lexical pushdown (FTS5).** `memories.content` is indexed with a SQLite FTS5 virtual table (`memories_fts`). On a hybrid query the store runs `MATCH` + `bm25()` *inside SQLite* to prune the candidate set to the top lexical hits first, then computes cosine only on that small set — eliminating the old full-table scan. Pure-semantic queries (or when FTS5 finds no lexical hit) fall back to a full scan so recall is preserved. **Multilingual word segmentation:** because the bundled SQLite build cannot enable the built-in `icu` tokenizer, content is pre-segmented in Rust before FTS5 indexing and the same `segment` logic tokenizes the query, so sub-words match instead of the whole run being one unmatchable token. Routing is by Unicode script: Chinese (zh/zh-TW) via `jieba-rs`, Japanese (ja) and Korean (ko) via `lindera` (embedded ipadic/ko-dic dictionaries), and the remaining space-delimited languages (en/es/fr/de/it/ru/pt/ar/hi) via whitespace — covering all 13 cockpit locales. The FTS index is rebuilt once on open (via `PRAGMA user_version`) so pre-existing databases pick up the segmented tokens (bumped to 4 for the multilingual router).
- **Higher lexical weight.** `SearchQuery.keyword_weight` defaults to `0.5` (was `0.3`), so lexical hits prune and rank earlier; `semantic_weight` stays `0.7`. `MemoryController` uses the same `0.7 / 0.5` blend.
- **Batch retrieval.** `MemoStore::search_batch` / `MemoManager::search_batch` / `recall_batch` score many queries under a single connection lock (one FTS5 prep + one scan), returning `Vec<Vec<ScoredMemo>>`. The CLI exposes `aria-memo search-batch --text … --text …` and `aria-memo bench --batch` (which reports `batch.ops_per_sec`).

## Quick Start

```bash
cargo build
cargo test
cargo run -p aria-memo -- add --type working --content "User likes Rust" --importance 0.8
cargo run -p aria-memo -- search --text "Rust" --top-k 5
```

## CLI & Self-Management

`aria-memo` ships two self-management commands (mirroring `aria-router`):

- `aria-memo setup` — persist the CLI config to `~/.ariacompute/memo-cli.yml`
  (the `upgrade_url` Releases org root). Interactive runs prompt you to pick
  **GitHub** (`https://github.com/ariacompute`, default) or **Gitee**
  (`https://gitee.com/ariacompute`). Non-interactive runs honor
  `ARIA_MEMO_SITE=cn` (→ Gitee), fall back to any existing config, then to GitHub.
  - `aria-memo setup --status` — print the config path and `upgrade_url`.
  - `aria-memo setup --clear` — delete the config file.
- `aria-memo upgrade [version]` — replace the running binary with the latest
  stable release (or a specific `version` tag) from GitHub/Gitee Releases.
  - `aria-memo upgrade --url <URL>` — override `upgrade_url` from config for this run.
  - The resolved `upgrade_url` precedence is:
    `--url` > `~/.ariacompute/memo-cli.yml` > `ARIA_MEMO_UPGRADE_URL` env > built-in
    default (`https://github.com/ariacompute`).
  - Release assets follow `aria-memo_{ver}_{os}.tar.gz`
    (`os` ∈ `linux_x86_64`/`linux_arm64`/`macos`/`windows_x86_64`; `.zip` on Windows).

`aria-memo --version` / `-v` prints the build version.

> The home directory `~/.ariacompute` is overridable via `ARIA_COMPUTE_HOME`.

## Multi-relational Memory Plane (Jev-Mem inspired)

A structured, multi-relational memory layer on top of the flat `MemoStore` — zero
network dependency, offline by default (an LLM-backed `RelationScorer` is pluggable).

Four relation views connect memories with directed edges:

- `semantic` — meaning association (cosine over embeddings)
- `temporal` — ordering / co-occurrence
- `causal` — causal chain
- `entity` — same-entity aggregation

Edges live in a separate `relations` table and never mutate `Memo`. On write,
`connect` infers four-view edges; on read, `retrieve` does hybrid search to seed
anchors, then a bounded graph expansion across views, returning scored memories
plus an inspectable `RetrieveTrace` (Jev-Mem's Retrieve→Assess→Expand, transparent).

```bash
# Infer four-view edges for an already-stored memory
cargo run -p aria-memo -- connect --id <id>

# Manually add an edge
cargo run -p aria-memo -- relate --from <id> --to <id> --kind semantic --score 0.8

# List edges (filter by from / to / kind)
cargo run -p aria-memo -- relations --kind semantic --json

# Bounded multi-relational traversal (views/budget/max-hops/top-k)
cargo run -p aria-memo -- graph --text "Rust systems" --views semantic,entity --top-k 20 --json

# Or turn graph awareness on for plain search
cargo run -p aria-memo -- search --text "Rust" --graph
```

The default `LocalRelationScorer` is fully local (semantic = cosine, temporal = time
order, entity = token Jaccard, causal = time-adjacency + overlap). Swap in an
LLM-backed scorer without touching the controller.

**Offline retrieval quality.** The flat `search` path is a hybrid of a local semantic
score (cosine over the hashing embedder) and a dependency-free lexical score
(`lexical_relevance`: rarity-weighted term overlap plus an exact-phrase bonus, with
CJK character 2-grams). Because the bundled embedder is a lightweight hash/TF-IDF
vectorizer — no heavy ML — the semantic signal is intentionally weak; `lexical_relevance`
maximizes the keyword contribution so offline recall stays useful. Tune
`SearchQuery.keyword_weight` (default `0.5`) to shift the balance.

## Benchmarks & Comparison

Compare against: mem0 / MemOS / MemPalace / Zep / Letta, plus local embedded
**control groups** `sqlite_vec` and `chromem` (included by default; skipped with
a recorded `reason` when their dependency/binary is missing — never faked).

- Feature matrix: [docs/compare.md](./docs/compare.md)
- Results guide: [docs/bench_results.md](./docs/bench_results.md)
- Python harness (Track A storage/retrieval + Track B end-to-end quality): [benches/README.md](./benches/README.md)

```bash
pip install -r benches/requirements.txt

pushd benches/chromem-wrapper
go get github.com/philippgille/chromem-go@latest && go mod tidy && go build -o chromem .
export CHROMEM_BIN="$PWD/chromem"
popd

python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem
python benches/run.py --track b --download
```

### Track A evaluation report

Run with `python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem`
(results under `benches/results/`). This report's figures:
- aria: `python benches/run.py --track a --sizes 1000,10000,100000 --systems aria`
- sqlite_vec: `python benches/run.py --track a --sizes 1000,10000,100000 --systems sqlite_vec`
- chromem: `python benches/run.py --track a --sizes 1000,10000,100000 --systems chromem`

**A1 — Microbench (top_k=5, offline; aria `add_baseline` = per-item embed + per-item txn):**

| System | Size | Segment | ops/sec | p50 (ms) | p99 (ms) | Note |
|--------|------|---------|--------:|---------:|---------:|------|
| aria | 1000 | `add_baseline` | 157.18 | 5.91 | 13.35 | naive per-item path (baseline) |
| aria | 1000 | `search` | 1456.11 | 0.69 | 0.69 | per-query amortized |
| aria | 10000 | `add_wal` | 752.06 | 1.06 | 4.08 | WAL journal mode |
| aria | 10000 | `add_batch_embed` | 40039.03 | 0.02 | 0.02 | batch embed + transactional write |
| aria | 10000 | `add_bulk` | 72164.68 | 0.01 | 0.01 | per-item embed, txn merge |
| aria | 10000 | `search` | 140.71 | 7.11 | 7.11 | per-query amortized |
| aria | 100000 | `add_batch_embed` | 41459.87 | 0.02 | 0.02 | only optimized segment at 100k |
| aria | 100000 | `search` | 13.65 | 73.25 | 73.25 | per-query amortized (full-corpus scan) |
| sqlite_vec | 1000 | `add` | 23345.52 | 0.04 | 0.04 | batched insert |
| sqlite_vec | 1000 | `search` | 592.15 | 1.69 | 1.69 | per-query amortized |
| sqlite_vec | 10000 | `add` | 22223.95 | 0.04 | 0.04 | batched insert |
| sqlite_vec | 10000 | `search` | 50.42 | 19.84 | 19.84 | per-query amortized |
| sqlite_vec | 100000 | `add` | 20212.36 | 0.05 | 0.05 | batched insert |
| sqlite_vec | 100000 | `search` | 4.58 | 218.25 | 218.25 | per-query amortized (full-corpus scan) |
| chromem | 1000 | `add` | 22126.47 | 0.05 | 0.05 | batched insert (`add-batch`) |
| chromem | 1000 | `search` | 208.95 | 4.79 | 4.79 | per-query amortized (`query-batch`) |
| chromem | 10000 | `add` | 23672.96 | 0.04 | 0.04 | batched insert (`add-batch`) |
| chromem | 10000 | `search` | 23.08 | 43.34 | 43.34 | per-query amortized (`query-batch`) |
| chromem | 100000 | `add` | 23089.77 | 0.04 | 0.04 | batched insert (`add-batch`) |
| chromem | 100000 | `search` | 2.33 | 429.42 | 429.42 | per-query amortized (`query-batch`) |

**A2 — Retrieval quality (synthetic_v2, 84 queries, top_k=5):**

| System | Recall@5 | MRR | Queries | Offline |
|--------|---------:|----:|--------:|:-------:|
| aria | 1.00 | 1.00 | 84 | true |
| sqlite_vec | 0.9762 | 0.9762 | 84 | true |
| chromem | 1.00 | 1.00 | 84 | true |

### Track B evaluation report

End-to-end memory quality across four benchmarks: `locomo_refined`, `halumem`, `longmemeval`, `personamem`.
The aria backend is driven via the CLI (build first: `cargo build -p aria-memo --release`).

```bash
# optional: verify dataset loading + backend capabilities before scoring
python benches/run.py --track b --dry-run

# real datasets (auto-download; fails loudly with manual hints if network blocked)
python benches/run.py --track b --download
```

Offline metrics (F1/BLEU, multiple-choice, retrieval Recall@k, extraction recall) run with **no LLM**.
LLM-judge metrics are **optional** — skipped silently when no credentials are present:

```bash
export BENCH_LLM_API_KEY=sk-...
export BENCH_LLM_BASE_URL=https://tokenhub.tencentmaas.com
python benches/run.py --track b --download --judge-model hy3
```

> Each benchmark falls back to the bundled `benches/data/fixtures/` (synthetic, offline smoke) when real data is absent; `dataset_source` is recorded in the report. Use `--benchmarks`, `--limit`, or `--ingest-only` to scope a run.

**Scoping large real datasets.** Real datasets can be huge — e.g. `halumem` (HaluMem-Medium) holds ~75k `add` calls. `--limit N` bounds how many samples are *ingested and evaluated* per benchmark (for `halumem` each record is one sample set; for `locomo_refined`/`longmemeval`/`personamem` it caps questions/items). When `--limit` is omitted, a default cap of `50` samples/benchmark is applied (printed to stderr) so an unbounded run does not hang. Ingestion prints progress to stderr (`[bench] ingest 500/N ... ingest done`).

```bash
# bounded, fast smoke run
python benches/run.py --track b --limit 2
# full-scale (long but bounded); build a release binary to speed up each add
cargo build -p aria-memo --release
python benches/run.py --track b --benchmarks halumem
```

## Directory

- `crates/core` — data models, unified `MemoError`, traits
- `crates/embed` — lightweight local embedder (ngram + hash/TF-IDF vectors) + cosine similarity
- `crates/storage` — rusqlite bundled embedded persistence backend
- `crates/memo` — memory management orchestration & lifecycle; multi-relational `MemoryController` / `RelationScorer`
- `crates/cli` — command-line entry point
- `benches/` — industry comparison harness
- `docs/` — feature matrix and benchmark notes

## Engineering Conventions

This repository follows the Harness Engineering philosophy:

- [`AGENTS.md`](AGENTS.md): Agent engineering context entry and directory index
- [`requirements.md`](requirements.md): Requirements spec (feature boundaries/exceptions/acceptance criteria, human-review-gated)
- [`task.md`](task.md): Implementation task checklist
