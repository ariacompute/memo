#!/usr/bin/env python3
from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from common import utc_stamp  # noqa: E402
from track_a import run_track_a  # noqa: E402
from track_b import BENCHMARKS, run_track_b  # noqa: E402

DEFAULT_SYSTEMS = "aria,mem0,memos,mempalace,zep,letta"


def parse_csv(raw: str) -> list[str]:
    return [x.strip() for x in raw.split(",") if x.strip()]


def main() -> int:
    p = argparse.ArgumentParser(description="aria-memo industry benches (Track A + B)")
    p.add_argument("--track", choices=("a", "b", "all"), default="a")
    p.add_argument("--systems", default="aria", help=f"comma list; full set: {DEFAULT_SYSTEMS}")
    p.add_argument("--size", type=int, default=1000, help="Track A corpus size for microbench")
    p.add_argument("--top-k", type=int, default=5)
    p.add_argument("--warmup", type=int, default=10)
    p.add_argument(
        "--benchmarks",
        default="locomo,longmemeval,beam",
        help="Track B benchmarks",
    )
    p.add_argument("--dry-run", action="store_true", help="Track B skeleton only")
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
        wrote.append(
            run_track_b(
                systems=systems,
                benchmarks=parse_csv(args.benchmarks) or list(BENCHMARKS),
                dry_run=args.dry_run,
                out_dir=out,
            )
        )

    print(f"results written under {out}")
    for w in wrote:
        print(f"  - {w}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
