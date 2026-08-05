from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from adapters import build_backend
from adapters.aria_memo import AriaMemoBackend
from common import timed_ms, utc_stamp, write_report


def _repo_data() -> Path:
    return Path(__file__).resolve().parents[1] / "data" / "synthetic_retrieval.json"


def run_microbench(systems: list[str], size: int, top_k: int, warmup: int) -> dict[str, Any]:
    rows: list[dict[str, Any]] = []
    for name in systems:
        backend = build_backend(name)
        info = backend.info()
        if not info.available:
            rows.append(
                {
                    "name": info.name,
                    "skipped": True,
                    "reason": info.reason,
                    "includes_network": info.includes_network,
                    "offline": info.offline,
                }
            )
            continue
        if isinstance(backend, AriaMemoBackend):
            row = backend.microbench_json(size=size, top_k=top_k, warmup=warmup)
            row["name"] = "aria"
            rows.append(row)
            backend.close()
            continue
        # 通用路径：Python 侧计时（可能含网络）
        try:
            backend.reset()
            add_ms: list[float] = []
            for i in range(size):
                _, ms = timed_ms(
                    lambda i=i: backend.add(
                        f"bench item {i}: user prefers rust and local-first memo {i}"
                    )
                )
                add_ms.append(ms)
            search_ms: list[float] = []
            queries = [
                "rust systems programming",
                "local-first memo",
                "user prefers",
                "bench item",
                "programming",
            ]
            for i in range(size):
                q = queries[i % len(queries)]
                _, ms = timed_ms(lambda q=q: backend.search(q, top_k=top_k))
                search_ms.append(ms)
            from common import percentile

            add_sum = sum(add_ms)
            search_sum = sum(search_ms)
            rows.append(
                {
                    "name": info.name,
                    "includes_network": info.includes_network,
                    "offline": info.offline,
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
            )
        except Exception as e:  # noqa: BLE001
            rows.append(
                {
                    "name": info.name,
                    "skipped": True,
                    "reason": str(e),
                    "includes_network": info.includes_network,
                }
            )
        finally:
            backend.close()
    return {
        "track": "A1-microbench",
        "generated_at": utc_stamp(),
        "size": size,
        "top_k": top_k,
        "systems": rows,
    }


def run_retrieval_quality(systems: list[str], top_k: int) -> dict[str, Any]:
    data = json.loads(_repo_data().read_text(encoding="utf-8"))
    corpus = data["corpus"]
    queries = data["queries"]
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
                # 匹配：命中内容与期望 doc 文本对齐
                relevant = set(q["relevant_ids"])
                hit_keys: list[str] = []
                for h in hits:
                    matched = None
                    for doc in corpus:
                        if doc["id"] in relevant and (
                            doc["content"][:40] in h.content or h.content in doc["content"]
                        ):
                            matched = doc["id"]
                            break
                    if matched:
                        hit_keys.append(matched)
                recall = len(set(hit_keys) & relevant) / max(len(relevant), 1)
                recalls.append(recall)
                rank = None
                for i, hk in enumerate(hit_keys):
                    if hk in relevant:
                        rank = i + 1
                        break
                rr.append(0.0 if rank is None else 1.0 / rank)
            rows.append(
                {
                    "name": info.name,
                    "recall_at_k": sum(recalls) / len(recalls) if recalls else 0.0,
                    "mrr": sum(rr) / len(rr) if rr else 0.0,
                    "n_queries": len(queries),
                    "top_k": top_k,
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
        "systems": rows,
        "dataset": str(_repo_data()),
    }


def run_track_a(
    systems: list[str],
    size: int,
    top_k: int,
    warmup: int,
    out_dir: Path,
) -> Path:
    micro = run_microbench(systems, size=size, top_k=top_k, warmup=warmup)
    quality = run_retrieval_quality(systems, top_k=top_k)
    payload = {
        "track": "A",
        "generated_at": utc_stamp(),
        "microbench": micro,
        "retrieval_quality": quality,
        "systems": micro["systems"] + [{"section": "quality", **r} for r in quality["systems"]],
        "notes": "See docs/compare.md; aria microbench uses in-process CLI bench JSON.",
    }
    write_report(out_dir, "track_a", payload)
    return out_dir / "track_a.json"
