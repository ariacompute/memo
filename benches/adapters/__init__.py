from __future__ import annotations

from .aria_memo import AriaMemoBackend
from .base import BackendInfo, MemoBackend, SearchHit, UnsupportedCapability
from .chromem import ChromemBackend
from .letta_adapter import LettaBackend
from .mem0_adapter import Mem0Backend
from .skip import SkipBackend
from .sqlite_vec import SqliteVecBackend
from .zep_adapter import ZepBackend


def build_backend(name: str) -> MemoBackend:
    key = name.strip().lower()
    if key in {"aria", "aria-memo", "aria_memo"}:
        return AriaMemoBackend()
    if key == "sqlite_vec":
        return SqliteVecBackend()
    if key == "chromem":
        return ChromemBackend()
    if key == "mem0":
        return Mem0Backend()
    if key == "zep":
        return ZepBackend()
    if key == "letta":
        return LettaBackend()
    return SkipBackend(key, f"unknown system: {name}")


__all__ = [
    "AriaMemoBackend",
    "BackendInfo",
    "ChromemBackend",
    "LettaBackend",
    "Mem0Backend",
    "MemoBackend",
    "SearchHit",
    "SkipBackend",
    "SqliteVecBackend",
    "UnsupportedCapability",
    "ZepBackend",
    "build_backend",
]
