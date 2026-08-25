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
    # 可选自动下载：(源 URL, 落盘文件名) 列表；落盘名可与 files 不同（如上游改名）。
    # 为空表示仅手动下载。
    urls: tuple[tuple[str, str], ...] = ()
    # 真实单文件标记：若存在，则视为 source=real（覆盖 files 的缺失判定）。
    # 用于真实数据为单文件、与 fixtures 多文件切分不同的基准（如 halumem）。
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
            "git clone https://github.com/mem-eval-suite/LoCoMo_refined ；"
            "将 data/public/questions.jsonl 与 data/public/conversations.jsonl 放入 benches/data/locomo_refined/。"
            "许可：CC BY-NC 4.0（仅研究用途）。"
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
            "huggingface-cli download IAAR-Shanghai/HaluMem --local-dir benches/data/halumem ；"
            "放置 HaluMem-Medium.jsonl / HaluMem-Long.jsonl（真实单文件格式，loader 已内置格式适配器）。"
        ),
    ),
    "longmemeval": DatasetSpec(
        bench="longmemeval",
        # 仅需 S 变体即可评测；M 为 2.7GB、Oracle 为可选。上游文件名带 _cleaned 后缀，落盘重命名为 longmemeval_s.json。
        files=("longmemeval_s.json",),
        urls=(
            ("https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_s_cleaned.json", "longmemeval_s.json"),
        ),
        manual=(
            "huggingface-cli download xiaowu0162/longmemeval-cleaned --local-dir benches/data/longmemeval ；"
            "将 longmemeval_s_cleaned.json 重命名为 longmemeval_s.json 放入 benches/data/longmemeval/。"
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
            "huggingface-cli download bowen-upenn/PersonaMem-v1 --local-dir benches/data/personamem ；"
            "放置 shared_contexts_{32k,128k,1M}.jsonl 与 questions_{32k,128k,1M}.csv。"
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
    # 真实单文件标记优先（覆盖 files 的缺失判定）
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
    # spec.urls 为 (源 URL, 落盘文件名) 列表；落盘名可与 spec.files 不同（上游改名/重命名）。
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
