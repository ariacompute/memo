# chromem-wrapper

A minimal Go CLI wrapper around
[chromem-go](https://github.com/philippgille/chromem-go) used as a **local embedded
control group** for the memo `Track A` benchmark (`benches/run.py --track a
--systems aria,sqlite_vec,chromem`).

chromem-go has no mature Python binding, so `benches/adapters/chromem.py` drives a
`chromem` binary via subprocess. This wrapper provides exactly the CLI surface the
adapter expects:

```bash
chromem <db> add        --content=<text>           # single-item add
chromem <db> add-batch  --file=<path>             # batched add (JSONL of strings; '-' = stdin)
chromem <db> query      --text=<text> --top-k=<k>  # stdout: score\tcontent per line
```

`add-batch` reads one JSON-encoded string per line (raw-text fallback) and inserts the
whole corpus in a **single process**. The adapter's `add_batch()` writes the corpus to a
temp JSONL and calls this once, so large corpora (10k/100k) no longer spawn one
subprocess per item (which previously took tens of minutes).

`<db>` is treated as a **directory** for chromem's persistent store, so memories
survive across the per-call subprocess invocations the harness spawns.

Embeddings are produced **locally** (hashing n-gram bag-of-words + L2 norm) — no
model download, fully offline, matching the harness's `offline: true` claim. The
wrapper always passes its own `embedFn` to `GetOrCreateCollection`; passing `nil`
would make chromem fall back to the OpenAI embedder (network), so don't.

## Build

Requires Go 1.21+.

```bash
cd benches/chromem-wrapper
go get github.com/philippgille/chromem-go@latest   # resolve the dependency (pinned v0.6.0 in go.mod)
go mod tidy
go build -o chromem .                             # produces ./chromem
```

> The module path is `github.com/philippgille/chromem-go` (the older
> `ottercookie/chromem-go` path no longer exists).

## Wire it into the harness

Point `CHROMEM_BIN` at the built binary (absolute path), then run Track A:

```bash
export CHROMEM_BIN="$PWD/benches/chromem-wrapper/chromem"
export CHROMEM_TIMEOUT=120        # optional; per-call subprocess timeout (default 120s)
python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem
```

Or put `chromem` on `PATH` — the adapter falls back to `shutil.which("chromem")`
when `CHROMEM_BIN` is unset.

When the binary is missing/unbuilt, the harness reports `chromem` as
`skipped` with a `reason` and **never fabricates numbers**.

## Notes / caveats

- **API used (chromem-go):** `chromem.NewPersistentDB(path, compress)`,
  `db.GetOrCreateCollection(name, nil, embedFn)`,
  `coll.Add(ctx, []string{id}, nil, nil, []string{content})` (embeddings `nil` →
  computed via `embedFn`), `coll.Query(ctx, text, nResults, nil, nil)` returning
  `Result` with `Similarity float32` (printed as the `score`). If your chromem-go
  version drifts, adjust `main.go`; if the flag shape changes, also update `_run`
  in `benches/adapters/chromem.py`.
- **Performance.** The harness invokes `add-batch` for the whole corpus (one process),
  so large corpora (10k/100k) insert quickly. The single-item `add` is retained for the
  small A2 retrieval dataset and backward compatibility; prefer `add-batch` for bulk loads.
- **Stronger embeddings.** Replace `embedFn` in `main.go` with a real offline
  embedder if you want the control group to carry a more meaningful retrieval signal.
