from __future__ import annotations

from collections import defaultdict
from typing import Any

from adapters import build_backend
from adapters.aria_memo import AriaMemoBackend
from common import percentile, timed_ms, utc_stamp, write_report
from metrics.retrieval import mean_std
from track_a.datasets import load_dataset


def _measure_generic(backend: Any, size: int, top_k: int, warmup: int) -> dict[str, Any]:
    """Python-side timing for non-aria backends (may include network for managed services)."""
    backend.reset()
    add_ms: list[float] = []
    for i in range(size):
        _, ms = timed_ms(
            lambda i=i: backend.add(
                f"bench item {i}: user prefers rust and local-first memo {i}"
            )
        )
        add_ms.append(ms)
    queries = [
        "rust systems programming",
        "local-first memo",
        "user prefers",
        "bench item",
        "programming",
    ]
    search_ms: list[float] = []
    for i in range(size):
        q = queries[i % len(queries)]
        _, ms = timed_ms(lambda q=q: backend.search(q, top_k=top_k))
        search_ms.append(ms)
    add_sum = sum(add_ms)
    search_sum = sum(search_ms)
    return {
        "size": size,
        "top_k": top_k,
        "add": {
            "p50_ms": percentile(add_ms, 0.5),
            "p99_ms": percentile(add_ms, 0.99),
            "ops_per_sec": (size / (add_sum / 1000.0)) if add_sum else 0.0,
        },
        "search": {
            "p50_ms": percentile(search_ms, 0.5),
            "p99_ms": percentile(search_ms, 0.99),
            "ops_per_sec": (size / (search_sum / 1000.0)) if search_sum else 0.0,
        },
    }


def _microbench_one(backend: Any, size: int, top_k: int, warmup: int) -> dict[str, Any]:
    info = backend.info()
    if isinstance(backend, AriaMemoBackend):
        row = backend.microbench_json(size=size, top_k=top_k, warmup=warmup)
        row["name"] = "aria"
        row["size"] = size
        return row
    return _measure_generic(backend, size, top_k, warmup)


def run_microbench(
    systems: list[str], sizes: list[int], top_k: int, warmup: int
) -> dict[str, Any]:
    """A1: multi-size microbenchmark. Sweeps each `size` and reports per-system add/search p50/p99,
    plus a `scaling` verdict (sublinear if p99 growth factor < size ratio)."""
    rows: list[dict[str, Any]] = []
    for size in sizes:
        for name in systems:
            backend = build_backend(name)
            info = backend.info()
            if not info.available:
                rows.append(
                    {
                        "name": info.name,
                        "size": size,
                        "skipped": True,
                        "reason": info.reason,
                        "includes_network": info.includes_network,
                        "offline": info.offline,
                    }
                )
                continue
            try:
                row = _microbench_one(backend, size, top_k, warmup)
                row["name"] = info.name
                row["includes_network"] = info.includes_network
                row["offline"] = info.offline
                rows.append(row)
            except Exception as e:  # noqa: BLE001
                rows.append(
                    {
                        "name": info.name,
                        "size": size,
                        "skipped": True,
                        "reason": str(e),
                        "includes_network": info.includes_network,
                    }
                )
            finally:
                backend.close()

    # Scaling analysis per system: p99 growth factor between consecutive sizes.
    by_sys: dict[str, dict[int, dict[str, Any]]] = defaultdict(dict)
    for r in rows:
        if not r.get("skipped") and "add" in r:
            by_sys[r["name"]][r["size"]] = r
    scaling: dict[str, Any] = {}
    for name, per in by_sys.items():
        ordered = sorted(per.keys())
        factors: dict[str, float | None] = {}
        verdicts: list[str] = []
        for a, b in zip(ordered, ordered[1:]):
            pa = per[a]["add"]["p99_ms"]
            pb = per[b]["add"]["p99_ms"]
            sa = per[a]["search"]["p99_ms"]
            sb = per[b]["search"]["p99_ms"]
            f_add = pb / pa if pa else None
            f_search = sb / sa if sa else None
            factors[f"add_p99_{a}_to_{b}"] = round(f_add, 3) if f_add is not None else None
            factors[f"search_p99_{a}_to_{b}"] = (
                round(f_search, 3) if f_search is not None else None
            )
            ratio = b / a
            if f_add is not None:
                if f_add < ratio:
                    verdicts.append("sublinear")
                elif abs(f_add - ratio) <= 0.1 * ratio:
                    verdicts.append("linear")
                else:
                    verdicts.append("superlinear")
        scaling[name] = {
            "factors": factors,
            "verdict": max(set(verdicts), key=verdicts.count) if verdicts else "n/a",
        }

    return {
        "track": "A1-microbench",
        "generated_at": utc_stamp(),
        "sizes": sizes,
        "top_k": top_k,
        "systems": rows,
        "scaling": scaling,
    }


def _align_hits(hits: list[Any], corpus: list[dict[str, Any]], relevant: set[str]) -> list[str]:
    """Map retrieved hits back to corpus ids via content alignment (mirrors the original A2 logic)."""
    hit_keys: list[str] = []
    for h in hits:
        for doc in corpus:
            if doc["id"] in relevant and (
                doc["content"][:40] in h.content or h.content in doc["content"]
            ):
                hit_keys.append(doc["id"])
                break
    return hit_keys


def run_retrieval_quality(
    systems: list[str], top_k: int, dataset: str = "synthetic_v2"
) -> dict[str, Any]:
    """A2: retrieval quality (Recall@k, MRR) + variance over a query set.

    `dataset` selects the query corpus (synthetic_v2 by default; can reuse Track B real sets).
    Missing datasets are skipped with a reason — no fabricated numbers.
    """
    ds = load_dataset(dataset)
    if ds is None:
        return {
            "track": "A2-retrieval-quality",
            "generated_at": utc_stamp(),
            "dataset": dataset,
            "skipped": True,
            "reason": f"dataset '{dataset}' unavailable (missing data file or unknown source)",
            "systems": [],
        }
    corpus = ds["corpus"]
    queries = ds["queries"]
    rows: list[dict[str, Any]] = []
    for name in systems:
        backend = build_backend(name)
        info = backend.info()
        if not info.available:
            rows.append({"name": info.name, "skipped": True, "reason": info.reason})
            continue
        try:
            backend.reset()
            for doc in corpus:
                backend.add(doc["content"], metadata={"key": doc["id"]})
            recalls: list[float] = []
            rr: list[float] = []
            for q in queries:
                hits = backend.search(q["text"], top_k=top_k)
                relevant = set(q["relevant_ids"])
                hit_keys = _align_hits(hits, corpus, relevant)
                recall = len(set(hit_keys) & relevant) / max(len(relevant), 1)
                recalls.append(recall)
                rank = None
                for i, hk in enumerate(hit_keys):
                    if hk in relevant:
                        rank = i + 1
                        break
                rr.append(0.0 if rank is None else 1.0 / rank)
            r_mean, r_std = mean_std(recalls)
            m_mean, m_std = mean_std(rr)
            rows.append(
                {
                    "name": info.name,
                    "available": True,
                    "n_queries": len(queries),
                    "top_k": top_k,
                    "recall_at_k": round(r_mean, 4),
                    "recall_at_k_std": round(r_std, 4),
                    "mrr": round(m_mean, 4),
                    "mrr_std": round(m_std, 4),
                    "offline": info.offline,
                }
            )
        except Exception as e:  # noqa: BLE001
            rows.append({"name": info.name, "skipped": True, "reason": str(e)})
        finally:
            backend.close()
    return {
        "track": "A2-retrieval-quality",
        "generated_at": utc_stamp(),
        "dataset": dataset,
        "source": ds.get("source", dataset),
        "systems": rows,
    }


def run_track_a(
    systems: list[str],
    sizes: list[int],
    top_k: int,
    warmup: int,
    a2_dataset: str,
    out_dir: Any,
) -> Any:
    micro = run_microbench(systems, sizes=sizes, top_k=top_k, warmup=warmup)
    quality = run_retrieval_quality(systems, top_k=top_k, dataset=a2_dataset)
    payload = {
        "track": "A",
        "generated_at": utc_stamp(),
        "microbench": micro,
        "retrieval_quality": quality,
        "notes": (
            "See docs/compare.md. A1 scales add/search p99 across sizes and reports a sublinear "
            "verdict; A2 reports Recall@k/MRR with per-query variance. aria microbench uses the "
            "in-process CLI bench JSON; write-tail (WAL/batch-embed/bulk) is exercised via "
            "`aria-memo bench --wal --batch-embed --bulk`."
        ),
    }
    write_report(out_dir, "track_a", payload)
    return out_dir / "track_a.json"
