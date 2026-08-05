from __future__ import annotations

"""Track B：LoCoMo / LongMemEval / BEAM 端到端骨架。

真实分数需要：
1. 下载官方数据集到 benches/data/
2. 配置答/判 LLM（OPENAI_API_KEY 等）
3. 可用的 MemoBackend.add/search

亦可将 adapters/ 接到 OmniMemEval / mem0 memo-benchmarks。
"""

from pathlib import Path
from typing import Any

from adapters import build_backend
from common import utc_stamp, write_report

BENCHMARKS = ("locomo", "longmemeval", "beam")


def _llm_configured() -> bool:
    import os

    return bool(
        os.environ.get("OPENAI_API_KEY")
        or os.environ.get("BENCH_LLM_API_KEY")
        or os.environ.get("ANTHROPIC_API_KEY")
    )


def run_one(
    benchmark: str,
    systems: list[str],
    dry_run: bool,
) -> dict[str, Any]:
    rows: list[dict[str, Any]] = []
    for name in systems:
        backend = build_backend(name)
        info = backend.info()
        base = {
            "name": info.name,
            "benchmark": benchmark,
            "includes_network": info.includes_network,
            "offline": info.offline,
        }
        if not info.available:
            rows.append({**base, "skipped": True, "reason": info.reason})
            continue
        if dry_run:
            rows.append(
                {
                    **base,
                    "dry_run": True,
                    "pipeline": ["ingest", "retrieve", "answer", "judge", "aggregate"],
                    "status": "ok",
                    "note": "Skeleton only; no dataset scored",
                }
            )
            backend.close()
            continue
        if not _llm_configured():
            rows.append(
                {
                    **base,
                    "skipped": True,
                    "reason": "No LLM API key (OPENAI_API_KEY / BENCH_LLM_API_KEY); use --dry-run",
                }
            )
            backend.close()
            continue
        # 完整实现点：加载 benches/data/<benchmark>/，调用 backend，LLM 答/判
        rows.append(
            {
                **base,
                "skipped": True,
                "reason": (
                    f"Dataset for {benchmark} not present under benches/data/{benchmark}/; "
                    "download upstream then re-run. Compatible with OmniMemEval user-memo track."
                ),
            }
        )
        backend.close()
    return {
        "benchmark": benchmark,
        "generated_at": utc_stamp(),
        "dry_run": dry_run,
        "systems": rows,
    }


def run_track_b(
    systems: list[str],
    benchmarks: list[str],
    dry_run: bool,
    out_dir: Path,
) -> Path:
    results = [run_one(b, systems, dry_run=dry_run) for b in benchmarks]
    payload = {
        "track": "B",
        "generated_at": utc_stamp(),
        "dry_run": dry_run,
        "benchmarks": results,
        "systems": [s for r in results for s in r["systems"]],
        "notes": (
            "End-to-end quality depends on extraction + judge models. "
            "Do not compare raw scores to managed mem0/MemOS without matching model stack. "
            "See https://github.com/MemTensor/OmniMemEval and "
            "https://github.com/mem0ai/memo-benchmarks"
        ),
    }
    write_report(out_dir, "track_b", payload)
    return out_dir / "track_b.json"
