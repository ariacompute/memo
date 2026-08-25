from __future__ import annotations

"""数据集定位与下载。

- `DATASET_SPECS`：各基准上游 URL + 必需文件清单（真实数据）。
- `resolve_dataset(bench)`：优先 `benches/data/<bench>/` 真实目录；缺失则回退仓库内置
  `benches/data/fixtures/<bench>/`，并在结果标注 `dataset_source='fixture'`（防误读）。
- `download(bench)`：urllib 拉取；网络/依赖失败抛错并打印手动指引，不静默失败。
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
    # 必需文件（出现在 data/<bench>/ 或 fixtures/<bench>/）
    files: tuple[str, ...]
    # 人类可读下载指引
    manual: str
    # 可选自动下载 URL（按顺序对应 files；为空表示仅手动）
    urls: tuple[str, ...] = ()


DATASET_SPECS: dict[str, DatasetSpec] = {
    "locomo_refined": DatasetSpec(
        bench="locomo_refined",
        files=("questions.jsonl", "conversations.jsonl"),
        urls=(
            "https://github.com/mem-eval-suite/LoCoMo_refined/raw/main/data/questions.jsonl",
            "https://github.com/mem-eval-suite/LoCoMo_refined/raw/main/data/conversations.jsonl",
        ),
        manual=(
            "git clone https://github.com/mem-eval-suite/LoCoMo_refined ；"
            "将 data/questions.jsonl 与 data/conversations.jsonl 放入 benches/data/locomo_refined/。"
            "许可：CC BY-NC 4.0（仅研究用途）。"
        ),
    ),
    "halumem": DatasetSpec(
        bench="halumem",
        files=("sessions.jsonl", "memories.jsonl", "questions.jsonl"),
        urls=(
            "https://huggingface.co/datasets/IAAR-Shanghai/HaluMem/resolve/main/sessions.jsonl",
            "https://huggingface.co/datasets/IAAR-Shanghai/HaluMem/resolve/main/memories.jsonl",
            "https://huggingface.co/datasets/IAAR-Shanghai/HaluMem/resolve/main/questions.jsonl",
        ),
        manual=(
            "huggingface-cli download IAAR-Shanghai/HaluMem --local-dir benches/data/halumem ；"
            "放置 sessions.jsonl / memories.jsonl / questions.jsonl。"
        ),
    ),
    "longmemeval": DatasetSpec(
        bench="longmemeval",
        # 仅需 S 变体即可评测；M/Oracle 为可选扩展（加载首个可用变体）
        files=("longmemeval_s.json",),
        urls=(
            "https://github.com/xiaowu0162/LongMemEval/raw/main/data/longmemeval_s.json",
            "https://github.com/xiaowu0162/LongMemEval/raw/main/data/longmemeval_m.json",
            "https://github.com/xiaowu0162/LongMemEval/raw/main/data/longmemeval_oracle.json",
        ),
        manual=(
            "git clone https://github.com/xiaowu0162/LongMemEval ；"
            "将 data/longmemeval_{s,m,oracle}.json 放入 benches/data/longmemeval/ "
            "（至少提供 longmemeval_s.json）。"
        ),
    ),
    "personamem": DatasetSpec(
        bench="personamem",
        files=("shared_contexts_32k.jsonl", "questions_32k.csv"),
        urls=(
            "https://github.com/bowen-upenn/PersonaMem/raw/main/data/shared_contexts_32k.jsonl",
            "https://github.com/bowen-upenn/PersonaMem/raw/main/data/questions_32k.csv",
        ),
        manual=(
            "git clone https://github.com/bowen-upenn/PersonaMem ；"
            "将 data/shared_contexts_{32k,128k,1M}.jsonl 与 data/questions_{32k,128k,1M}.csv "
            "放入 benches/data/personamem/。"
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
    if len(spec.urls) != len(spec.files):
        raise RuntimeError(f"[{bench}] url/file count mismatch in DATASET_SPECS")
    target = dest or (_DATA_ROOT / bench)
    target.mkdir(parents=True, exist_ok=True)
    for url, fname in zip(spec.urls, spec.files):
        out = target / fname
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "aria-memo-bench/1.0"})
            with urllib.request.urlopen(req, timeout=60) as r, open(out, "wb") as w:
                w.write(r.read())
        except Exception as e:  # noqa: BLE001
            out.unlink(missing_ok=True)
            raise RuntimeError(
                f"[{bench}] download failed for {fname}: {e}\n"
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
