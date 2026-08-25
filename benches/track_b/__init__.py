"""Track B — end-to-end memory-quality evaluation registry for four benchmarks.

Benchmarks: locomo_refined / halumem / longmemeval / personamem (early beam and
old locomo were removed). Each benchmark subpackage exports the unified contract:
    NAME: str
    DATASET_FILES: tuple[str, ...]
    def load(path, limit=None) -> Dataset
    def run(backend, dataset, judge, top_k) -> list[Score]
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from adapters import MemoBackend
from common import utc_stamp, write_report
from common.reporting import Score
from datasets import resolve_dataset
from judge import Judge

BENCHMARKS = ("locomo_refined", "halumem", "longmemeval", "personamem")

# Default cap on the number of samples evaluated per benchmark when `--limit` is
# omitted. Real datasets (e.g. HaluMem-Medium) can require tens of thousands of
# `add` calls; without a cap an unbounded run is effectively unresponsive.
DEFAULT_BENCH_LIMIT = 50

_REGISTRY = {
    "locomo_refined": "track_b.locomo_refined",
    "halumem": "track_b.halumem",
    "longmemeval": "track_b.longmemeval",
    "personamem": "track_b.personamem",
}


def _load_module(bench: str):
    import importlib

    return importlib.import_module(_REGISTRY[bench])


def run_track_b(
    backend: MemoBackend,
    judge: Judge | None,
    out_dir: str | None = None,
    benchmarks: list[str] | None = None,
    top_k: int = 5,
    limit: int | None = None,
    do_ingest: bool = True,
) -> dict[str, Any]:
    """Run end-to-end evaluation on the selected benchmarks, aggregate and report."""
    selected = benchmarks or list(BENCHMARKS)
    datasets_source: dict[str, str] = {}
    bench_scores: list[dict[str, Any]] = []

    effective_limit = limit if limit is not None else DEFAULT_BENCH_LIMIT
    if limit is None:
        print(f"[track_b] no --limit given; applying default cap of {DEFAULT_BENCH_LIMIT} samples/benchmark")

    for bench in selected:
        if bench not in BENCHMARKS:
            raise ValueError(f"unknown benchmark: {bench}; valid={BENCHMARKS}")
        mod = _load_module(bench)
        resolved = resolve_dataset(bench)
        datasets_source[bench] = resolved.source
        print(f"[track_b] {bench}: dataset={resolved.path} (source={resolved.source})")
        dataset = mod.load(resolved.path, limit=effective_limit)
        scores = mod.run(backend, dataset, judge, top_k, do_ingest=do_ingest)
        bench_scores.append(
            {
                "benchmark": bench,
                "dataset_source": resolved.source,
                "scores": [s.as_dict() for s in scores],
            }
        )

    summary = {
        "track": "B",
        "model": backend.name(),
        "timestamp": utc_stamp(),
        "benchmarks": selected,
        "datasets_source": datasets_source,
        "judge": {
            "available": judge is not None,
            "model": judge.model if judge else None,
            "calls": judge.stats.calls if judge else 0,
            "errors": judge.stats.errors if judge else 0,
        },
        "systems": _flatten_systems(bench_scores),
    }
    if out_dir is not None:
        write_report(out_dir, "track_b", summary)
    return summary


def _flatten_systems(bench_scores: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Keep the top-level `systems` as a list structure where each element holds a
    benchmark and its scores, for compatibility with existing report rendering."""
    systems: list[dict[str, Any]] = []
    for entry in bench_scores:
        systems.append(
            {
                "system": entry["benchmark"],
                "dataset_source": entry["dataset_source"],
                "scores": entry["scores"],
            }
        )
    return systems


def dry_run(
    backend: MemoBackend,
    out_dir: str | None = None,
    benchmarks: list[str] | None = None,
) -> dict[str, Any]:
    """Only validate dataset loading and backend capability probing; no scoring."""
    selected = benchmarks or list(BENCHMARKS)
    report: dict[str, Any] = {
        "track": "B",
        "mode": "dry-run",
        "model": backend.name(),
        "capabilities": {
            "list_memories": backend.supports("list_memories"),
            "update": backend.supports("update"),
        },
        "benchmarks": [],
    }
    for bench in selected:
        mod = _load_module(bench)
        resolved = resolve_dataset(bench)
        try:
            dataset = mod.load(resolved.path)
        except Exception as e:  # noqa: BLE001
            report["benchmarks"].append(
                {
                    "benchmark": bench,
                    "source": resolved.source,
                    "status": "load-error",
                    "error": str(e),
                }
            )
            continue
        report["benchmarks"].append(
            {
                "benchmark": bench,
                "source": resolved.source,
                "status": "ok",
                "items": getattr(dataset, "size", None),
                "dataset_files": list(mod.DATASET_FILES),
            }
        )
    if out_dir is not None:
        write_report(out_dir, "track_b_dry", report)
    return report


__all__ = ["BENCHMARKS", "run_track_b", "dry_run"]
