from __future__ import annotations

"""Dataset location and download.

- `DATASET_SPECS`: each benchmark's upstream URLs + required file list (real data).
- `resolve_dataset(bench)`: prefer the real directory `benches/data/<bench>/`; fall back to
  the repo-bundled `benches/data/fixtures/<bench>/`, tagging the result with
  `dataset_source='fixture'` (to prevent misreading as real data).
- `download(bench)`: fetch via urllib; on network/dependency failure, raise and print manual
  instructions rather than failing silently.
"""

from dataclasses import dataclass
from pathlib import Path
import os
import sys
import urllib.request

_BENCH_ROOT = Path(__file__).resolve().parent
_DATA_ROOT = _BENCH_ROOT / "data"


@dataclass(frozen=True)
class DatasetSpec:
    bench: str
    # required files (present in data/<bench>/ or fixtures/<bench>/)
    files: tuple[str, ...]
    # human-readable download instructions
    manual: str
    # optional auto-download: list of (source URL, on-disk filename); the on-disk name may
    # differ from `files` (e.g. when the upstream renames the file). Empty means manual-only.
    urls: tuple[tuple[str, str], ...] = ()
    # real single-file markers: if present, treated as source=real (overrides the missing-file
    # check). Used for benchmarks whose real data is a single file split differently from the
    # multi-file fixtures (e.g. halumem).
    real_markers: tuple[str, ...] = ()


DATASET_SPECS: dict[str, DatasetSpec] = {
    "locomo_refined": DatasetSpec(
        bench="locomo_refined",
        files=("questions.jsonl", "conversations.jsonl"),
        urls=(
            ("https://raw.githubusercontent.com/mem-eval-suite/LoCoMo_refined/main/data/public/questions.jsonl", "questions.jsonl"),
            ("https://raw.githubusercontent.com/mem-eval-suite/LoCoMo_refined/main/data/public/conversations.jsonl", "conversations.jsonl"),
        ),
        manual=(
            "git clone https://github.com/mem-eval-suite/LoCoMo_refined ; "
            "place data/public/questions.jsonl and data/public/conversations.jsonl into benches/data/locomo_refined/. "
            "License: CC BY-NC 4.0 (research use only)."
        ),
    ),
    "halumem": DatasetSpec(
        bench="halumem",
        files=("sessions.jsonl", "memories.jsonl", "questions.jsonl"),
        urls=(
            ("https://huggingface.co/datasets/IAAR-Shanghai/HaluMem/resolve/main/HaluMem-Medium.jsonl", "HaluMem-Medium.jsonl"),
        ),
        real_markers=("HaluMem-Medium.jsonl", "HaluMem-Long.jsonl"),
        manual=(
            "huggingface-cli download IAAR-Shanghai/HaluMem --local-dir benches/data/halumem ; "
            "place HaluMem-Medium.jsonl / HaluMem-Long.jsonl (real single-file format; the loader has a built-in format adapter)."
        ),
    ),
    "longmemeval": DatasetSpec(
        bench="longmemeval",
        # only the S variant is needed for evaluation; M is 2.7GB and Oracle is optional.
        # the upstream filename has a _cleaned suffix and is renamed to longmemeval_s.json on disk.
        files=("longmemeval_s.json",),
        urls=(
            ("https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_s_cleaned.json", "longmemeval_s.json"),
        ),
        manual=(
            "huggingface-cli download xiaowu0162/longmemeval-cleaned --local-dir benches/data/longmemeval ; "
            "rename longmemeval_s_cleaned.json to longmemeval_s.json and place it in benches/data/longmemeval/."
        ),
    ),
    "personamem": DatasetSpec(
        bench="personamem",
        files=("shared_contexts_32k.jsonl", "questions_32k.csv"),
        urls=(
            ("https://huggingface.co/datasets/bowen-upenn/PersonaMem-v1/resolve/main/shared_contexts_32k.jsonl", "shared_contexts_32k.jsonl"),
            ("https://huggingface.co/datasets/bowen-upenn/PersonaMem-v1/resolve/main/questions_32k.csv", "questions_32k.csv"),
        ),
        manual=(
            "huggingface-cli download bowen-upenn/PersonaMem-v1 --local-dir benches/data/personamem ; "
            "place shared_contexts_{32k,128k,1M}.jsonl and questions_{32k,128k,1M}.csv."
        ),
    ),
}


@dataclass
class Resolved:
    path: Path
    source: str  # 'real' | 'fixture'
    missing: tuple[str, ...] = ()


def _check_files(base: Path, files: tuple[str, ...]) -> tuple[str, ...]:
    return tuple(f for f in files if not (base / f).exists())


def resolve_dataset(bench: str) -> Resolved:
    spec = DATASET_SPECS[bench]
    real = _DATA_ROOT / bench
    # real single-file markers take priority (override the missing-file check for `files`)
    if spec.real_markers and any((real / m).exists() for m in spec.real_markers):
        return Resolved(path=real, source="real", missing=())
    missing = _check_files(real, spec.files)
    if not missing:
        return Resolved(path=real, source="real", missing=())
    fixture = _DATA_ROOT / "fixtures" / bench
    fmissing = _check_files(fixture, spec.files)
    if not fmissing:
        return Resolved(path=fixture, source="fixture", missing=())
    raise FileNotFoundError(
        f"[{bench}] missing files {missing} in {real} and {fmissing} in {fixture}.\n"
        f"Download via `python benches/datasets.py --download {bench}` or manually:\n{spec.manual}"
    )



def download(bench: str, dest: Path | None = None) -> Path:
    spec = DATASET_SPECS[bench]
    if not spec.urls:
        raise RuntimeError(f"[{bench}] no auto-download URL; manual steps:\n{spec.manual}")
    target = dest or (_DATA_ROOT / bench)
    target.mkdir(parents=True, exist_ok=True)
    # spec.urls is a list of (source URL, on-disk filename); the on-disk name may differ from
    # spec.files (upstream rename/rename).
    for url, dest_name in spec.urls:
        out = target / dest_name
        if out.exists():
            continue
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "aria-memo-bench/1.0"})
            with urllib.request.urlopen(req, timeout=60) as r, open(out, "wb") as w:
                while True:
                    chunk = r.read(1 << 16)
                    if not chunk:
                        break
                    w.write(chunk)
        except Exception as e:  # noqa: BLE001
            out.unlink(missing_ok=True)
            raise RuntimeError(
                f"[{bench}] download failed for {dest_name}: {e}\n"
                f"Network unavailable or blocked. Manual steps:\n{spec.manual}"
            ) from e
    return target


def _cli(argv: list[str]) -> int:
    args = argv[1:]
    if not args or args[0] in ("-h", "--help"):
        print("usage: python datasets.py --download <bench> [--list]")
        return 0
    if args[0] == "--list":
        for k in DATASET_SPECS:
            print(k)
        return 0
    if args[0] == "--download":
        bench = args[1] if len(args) > 1 else ""
        if bench not in DATASET_SPECS:
            print(f"unknown benchmark: {bench}", file=sys.stderr)
            return 2
        download(bench)
        print(f"downloaded {bench} -> {_DATA_ROOT / bench}")
        return 0
    print(f"unknown args: {args}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(_cli(sys.argv))
