# memo release notes

Local-first long-term memory store for LLM Agents, written in pure Rust.

## Highlights

- **Storage and retrieval**: SQLite-backed three-tier memories (working/short-term/long-term), FTS5 lexical candidate pushdown, semantic cosine recall, batch search/recall, consolidation, deduplication, and forgetting — all offline at runtime.
- **Multi-relational plane**: four directed views (semantic/temporal/causal/entity), write→connect scoring, retrieve→assess→expand traversal with an inspectable `RetrieveTrace`, and true best-score `graph_bfs` with bounded budget/max_hops.
- **Benchmark harness**: Track A microbenchmarks and retrieval quality plus Track B four-benchmark quality runs; A2 reuses real Track B corpora when available and falls back to bundled fixtures with `dataset_source` tagging.
- **Default evaluation path**: `--sizes` now sweeps `1000,10000,100000` and `--systems` uses the full local control-group set; docs use the correct `ARIA_MEMO_BIN` variable.
- **Comparison adapters**: MemOS/MemPalace stubs removed; a local `agentmem` HTTP adapter is included behind `AGENTMEM_BIN`, with an isolated temp env, projection sync before queries, and honest skip+reason when unavailable.
- **Build-time dictionary cache**: CI/release cache Lindera dictionaries via `LINDERA_BUILD_DICTIONARY_CACHE_DIR`; `scripts/prefetch-lindera.sh` supports offline rebuilds. Runtime remains zero-network; docs no longer claim a network-free first build.
- **Test gates**: Rust fmt/clippy/tests and Python unittest discovery run in CI; pytest is available for local discovery. Binary-level e2e tests drive the compiled CLI against a temporary SQLite DB across the full memory/relation/graph lifecycle.
- **Coverage**: Lines and Functions are 100%; targeted error-path tests raise Regions to 97.80%. No `#[coverage(off)]` was added.
- **Docs**: feature matrix, both READMEs, requirements, task list, and coverage figures are synced; English and Chinese READMEs are structurally aligned.

## Breaking changes

- `aria-memo add --type` now returns `MemoError::InvalidParam` for unknown types instead of silently falling back to `working`.
- `run.py --sizes` and `--systems` defaults changed to the spec values; examples relying on the old single-size default must pass `--sizes` explicitly.
- `memos` and `mempalace` adapters were removed; use `agentmem` as the local control group.

## Known limitations

- The first Rust build still downloads Japanese/Korean dictionaries from Lindera.dev; warm the cache once with `scripts/prefetch-lindera.sh` for offline rebuilds.
- Regions coverage is 97.80%; the remaining branches are mostly `?` error-propagation paths that need fault or dependency injection.
- `agentmem` requires a local Go binary via `AGENTMEM_BIN`; cloud adapters (mem0/Zep/Letta) require credentials and skip with a reason when unavailable.
- Bundled fixtures are synthetic smoke data tagged `dataset_source: fixture` and must not be compared directly with official benchmark scores.
- Track B LLM-judge metrics remain conditional on an OpenAI-compatible endpoint; offline metrics are always emitted.
