#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from common import utc_stamp  # noqa: E402
from track_a import run_track_a  # noqa: E402
from track_b import BENCHMARKS, dry_run, run_track_b  # noqa: E402
from adapters import build_backend  # noqa: E402
from judge import Judge  # noqa: E402

DEFAULT_SYSTEMS = "aria,mem0,memos,mempalace,zep,letta"


def parse_csv(raw: str) -> list[str]:
    return [x.strip() for x in raw.split(",") if x.strip()]


def main() -> int:
    p = argparse.ArgumentParser(description="aria-memo industry benches (Track A + B)")
    p.add_argument("--track", choices=("a", "b", "all"), default="a")
    p.add_argument("--systems", default="aria", help=f"comma list; full set: {DEFAULT_SYSTEMS}")
    p.add_argument("--backend", default=os.environ.get("BENCH_BACKEND", "aria"),
                   help="Track B 单后端（默认 aria）")
    p.add_argument("--size", type=int, default=1000, help="Track A corpus size for microbench")
    p.add_argument("--top-k", type=int, default=5)
    p.add_argument("--warmup", type=int, default=10)
    p.add_argument(
        "--benchmarks",
        default=",".join(BENCHMARKS),
        help=f"Track B benchmarks (default: {','.join(BENCHMARKS)})",
    )
    p.add_argument("--limit", type=int, default=None, help="每条基准最多评测样本数")
    p.add_argument("--ingest-only", action="store_true",
                   help="只写入记忆不检索/打分")
    p.add_argument("--dry-run", action="store_true", help="Track B 仅验证加载与契约")
    p.add_argument("--download", action="store_true",
                   help="运行前自动尝试下载缺失数据集")
    p.add_argument("--judge-model", default=None, help="LLM judge 模型覆盖（需 BENCH_LLM_API_KEY）")
    p.add_argument(
        "--out",
        default="",
        help="output directory (default benches/results/<timestamp>)",
    )
    args = p.parse_args()

    systems = parse_csv(args.systems)
    out = Path(args.out) if args.out else ROOT / "results" / utc_stamp()
    out.mkdir(parents=True, exist_ok=True)

    wrote: list[Path] = []
    if args.track in {"a", "all"}:
        wrote.append(
            run_track_a(
                systems=systems,
                size=args.size,
                top_k=args.top_k,
                warmup=args.warmup,
                out_dir=out,
            )
        )
    if args.track in {"b", "all"}:
        benchmarks = parse_csv(args.benchmarks) or list(BENCHMARKS)
        if args.download:
            from datasets import download

            for b in benchmarks:
                try:
                    download(b)
                    print(f"[download] {b} ok")
                except Exception as e:  # noqa: BLE001
                    print(f"[download] {b} failed: {e}", file=sys.stderr)

        backend = build_backend(args.backend)
        info = backend.info()
        if not info.available:
            print(f"[skip] backend '{args.backend}' unavailable: {info.reason}", file=sys.stderr)
        else:
            judge = None if args.dry_run else Judge.from_env(model_override=args.judge_model)
            if judge is None and not args.dry_run:
                print("[note] no LLM judge credentials (BENCH_LLM_API_KEY); "
                      "judge metrics will be skipped", file=sys.stderr)
            if args.dry_run:
                dry_run(backend, out_dir=out, benchmarks=benchmarks)
            else:
                run_track_b(
                    backend,
                    judge,
                    out_dir=out,
                    benchmarks=benchmarks,
                    top_k=args.top_k,
                    limit=args.limit,
                    do_ingest=not args.ingest_only,
                )
            print(f"[done] Track B reports in {out}")

    print(f"results written under {out}")
    for w in wrote:
        print(f"  - {w}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
