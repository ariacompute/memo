from __future__ import annotations

import json
import statistics
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def percentile(xs: list[float], p: float) -> float:
    if not xs:
        return 0.0
    ys = sorted(xs)
    idx = int(round((len(ys) - 1) * p))
    return ys[min(max(idx, 0), len(ys) - 1)]


def timed_ms(fn) -> tuple[Any, float]:
    t0 = time.perf_counter()
    out = fn()
    return out, (time.perf_counter() - t0) * 1000.0


def write_report(out_dir: Path, stem: str, payload: dict[str, Any]) -> tuple[Path, Path]:
    out_dir.mkdir(parents=True, exist_ok=True)
    jp = out_dir / f"{stem}.json"
    mp = out_dir / f"{stem}.md"
    jp.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    mp.write_text(_to_markdown(stem, payload), encoding="utf-8")
    return jp, mp


def _to_markdown(stem: str, payload: dict[str, Any]) -> str:
    lines = [f"# {stem}", "", f"Generated: `{payload.get('generated_at', '')}`", ""]
    systems = payload.get("systems") or payload.get("results") or []
    if isinstance(systems, dict):
        systems = [{"name": k, **v} for k, v in systems.items()]
    for item in systems:
        if not isinstance(item, dict):
            continue
        name = item.get("name") or item.get("system") or "?"
        if item.get("skipped"):
            lines.append(f"- **{name}**: skipped — {item.get('reason', '')}")
            continue
        lines.append(f"## {name}")
        for k, v in item.items():
            if k in {"name", "system"}:
                continue
            lines.append(f"- `{k}`: `{v}`")
        lines.append("")
    if "notes" in payload:
        lines.extend(["## Notes", str(payload["notes"]), ""])
    return "\n".join(lines) + "\n"


def mean(xs: list[float]) -> float:
    return float(statistics.mean(xs)) if xs else 0.0
