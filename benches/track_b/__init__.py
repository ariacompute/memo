"""Track B — 四基准端到端记忆质量评测注册表。

基准：locomo_refined / halumem / longmemeval / personamem（移除早期 beam 与旧 locomo）。
每个基准子包导出统一契约：
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
    """对所选基准跑端到端评测，聚合并报告。"""
    selected = benchmarks or list(BENCHMARKS)
    datasets_source: dict[str, str] = {}
    bench_scores: list[dict[str, Any]] = []

    for bench in selected:
        if bench not in BENCHMARKS:
            raise ValueError(f"unknown benchmark: {bench}; valid={BENCHMARKS}")
        mod = _load_module(bench)
        resolved = resolve_dataset(bench)
        datasets_source[bench] = resolved.source
        print(f"[track_b] {bench}: dataset={resolved.path} (source={resolved.source})")
        dataset = mod.load(resolved.path, limit=limit)
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
    """顶层 `systems` 保持列表结构，每个元素含 benchmark 与 scores，兼容既有报告渲染。"""
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
    """只验证数据集加载与后端能力探测，不打分。"""
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
