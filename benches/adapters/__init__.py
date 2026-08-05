from __future__ import annotations

from .aria_memo import AriaMemoBackend
from .base import BackendInfo, MemoBackend, SearchHit
from .letta_adapter import LettaBackend
from .mem0_adapter import Mem0Backend
from .memos_adapter import MemosBackend
from .mempalace_adapter import MemPalaceBackend
from .skip import SkipBackend
from .zep_adapter import ZepBackend


def build_backend(name: str) -> MemoBackend:
    key = name.strip().lower()
    if key in {"aria", "aria-memo", "aria_memo"}:
        return AriaMemoBackend()
    if key == "mem0":
        return Mem0Backend()
    if key in {"memos", "mem-os"}:
        return MemosBackend()
    if key == "mempalace":
        return MemPalaceBackend()
    if key == "zep":
        return ZepBackend()
    if key == "letta":
        return LettaBackend()
    return SkipBackend(key, f"unknown system: {name}")


__all__ = [
    "AriaMemoBackend",
    "BackendInfo",
    "LettaBackend",
    "Mem0Backend",
    "MemPalaceBackend",
    "MemoBackend",
    "MemosBackend",
    "SearchHit",
    "SkipBackend",
    "ZepBackend",
    "build_backend",
]
