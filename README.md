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

- **Lexical pushdown (FTS5).** `memories.content` is indexed with a SQLite FTS5 virtual table (`memories_fts`). On a hybrid query the store runs `MATCH` + `bm25()` *inside SQLite* to prune the candidate set to the top lexical hits first, then computes cosine only on that small set — eliminating the old full-table scan. Pure-semantic queries (or when FTS5 finds no lexical hit) fall back to a full scan so recall is preserved.
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
`SearchQuery.keyword_weight` (default `0.3`) to shift the balance.

## Benchmarks & Comparison

Compare against: mem0 / MemOS / MemPalace / Zep / Letta.

- Feature matrix: [docs/compare.md](./docs/compare.md)
- Results guide: [docs/bench_results.md](./docs/bench_results.md)
- Python harness (Track A storage/retrieval + Track B end-to-end quality): [benches/README.md](./benches/README.md)

```bash
pip install -r benches/requirements.txt
python benches/run.py --track a --size 1000
python benches/run.py --track b --dry-run
```

### Track A evaluation report

Run with `python benches/run.py --track a --size 1000`
(results under `benches/results/`, e.g. `20261001T012416Z/track_a.json`).

**A1 — Microbench (aria-memo, size=1000, top_k=5, offline):**

| Operation | ops/sec | p50 (ms) | p99 (ms) |
|-----------|--------:|---------:|---------:|
| `add`     | 298.36  | 2.98     | 9.77     |
| `search`  | 434.66  | 2.19     | 3.93     |

**A2 — Retrieval quality (synthetic_retrieval.json, 8 queries, top_k=5):**

| System | Recall@5 | MRR  | Queries | Offline |
|--------|---------:|-----:|--------:|:-------:|
| aria   | 1.00     | 1.00 | 8       | true    |

> A1 is an in-process (offline) microbenchmark; A2 measures hybrid (semantic + keyword) retrieval on a synthetic dataset. See [docs/compare.md](./docs/compare.md) for the full comparison matrix.

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
python benches/run.py --track b --judge-model hy3
```

> Each benchmark falls back to the bundled `benches/data/fixtures/` (synthetic, offline smoke) when real data is absent; `dataset_source` is recorded in the report. Use `--benchmarks`, `--limit`, or `--ingest-only` to scope a run.

> **Timeouts & progress (no silent hangs).** Every aria-memo CLI call has a 120s timeout (override `ARIA_MEMO_TIMEOUT`); the LLM judge has a 30s per-call timeout (override `BENCH_LLM_TIMEOUT`). Both emit `[bench]`/`[judge]` progress to stderr, so a long run stays observable. `--judge-model` requires `BENCH_LLM_API_KEY` (and `BENCH_LLM_BASE_URL` for self-hosted models like `hy3`); without it, judge metrics are skipped and only offline metrics run. Memory-heavy sub-tasks (e.g. `halumem` extraction) issue one judge call per memory — keep the run bounded with `--limit`.
> **Judge errors are surfaced, not swallowed.** A failing judge call (bad `BENCH_LLM_BASE_URL`/`API_KEY`, unknown model, rate-limit, or an incompatible response shape) prints `[judge] ERROR (first): …` with the HTTP status + body (or exception) **and every URL tried**, then a count every 10 errors. Errored calls are treated as undecidable and skipped — the run still finishes. The judge (a) auto-retries the alternate `/v1` mount on HTTP 404, (b) retries transient failures (timeout / HTTP 429·5xx) with exponential backoff, **escalating the per-call timeout 1×→2×→4×→8×** on retries, and (c) already sends `stream: false` and tolerates chat/completion/streaming responses. Tune with `BENCH_LLM_TIMEOUT` (per-call seconds, default 30) and `BENCH_LLM_RETRIES` (default 2). A persistently high error rate means the endpoint is too slow or misconfigured — raise `BENCH_LLM_TIMEOUT` (e.g. `120`) for long-prompt benchmarks like `halumem`.

**Scoping large real datasets.** Real datasets can be huge — e.g. `halumem` (HaluMem-Medium) holds ~75k `add` calls. `--limit N` bounds how many samples are *ingested and evaluated* per benchmark (for `halumem` each record is one sample set; for `locomo_refined`/`longmemeval`/`personamem` it caps questions/items). When `--limit` is omitted, a default cap of `50` samples/benchmark is applied (printed to stderr) so an unbounded run does not hang. Ingestion prints progress to stderr (`[bench] ingest 500/N ... ingest done`).

```bash
# bounded, fast smoke run
python benches/run.py --track b --limit 2
# full-scale (long but bounded); build a release binary to speed up each add
cargo build -p aria-memo --release
python benches/run.py --track b --benchmarks halumem
```

**Offline results — this run (`20261001T014217Z/track_b.json`, real datasets, judge unavailable):**

All four benchmarks ran on the bundled real datasets; the LLM judge was **not** available (`BENCH_LLM_API_KEY` absent → `judge.available=false`, 0 calls), so every LLM-dependent metric is `skipped`. Only offline metrics are reported:

| Benchmark | Offline metric | Value | Subset / note |
|-----------|---------------|------:|---------------|
| `locomo_refined` | F1 | 0.008 / 0.006 / 0.026 | subsets 1/2/3 (no LLM answer gen) |
| `locomo_refined` | BLEU | 0.004 / 0.003 / 0.014 | subsets 1/2/3 |
| `locomo_refined` | judge_accuracy | skipped | no LLM judge |
| `halumem` | retrieval_recall@5 | 1.00 | extraction subset — retrieval OK |
| `halumem` | retrieval_recall@5 | 0.00 | qa subset |
| `halumem` | memory_recall / memory_accuracy / false_memory_resistance / f1 / qa_accuracy | skipped | no LLM judge |
| `longmemeval` | retrieval_recall@5 | 0.00 | offline |
| `longmemeval` | qa_accuracy | skipped | no LLM judge |
| `personamem` | multiple_choice_accuracy | 0.00 | offline (no LLM answering) |

Takeaway: with **no LLM judge**, Track B cannot score generative QA quality — offline metrics only confirm that retrieval works for `halumem` extraction (recall@5 = 1.00), while other retrieval/multiple-choice signals are near zero. The `halumem` `updating` subset metrics are also skipped because that dataset has no update-type questions. Meaningful quality numbers require the LLM-judge path (set `BENCH_LLM_API_KEY`) or a larger offline signal set.

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
