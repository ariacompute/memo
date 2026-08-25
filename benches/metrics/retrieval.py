from __future__ import annotations

from typing import Iterable


def recall_at_k(relevant: Iterable[str], retrieved: Iterable[str], k: int) -> float:
    """标准 per-query Recall@k：retrieved[:k] 中是否召回任一 relevant 项。"""
    rel = set(relevant)
    if not rel:
        return 0.0
    ret = list(retrieved)[:k]
    ret_set = set(ret)
    return 1.0 if rel & ret_set else 0.0


def mrr(relevant: Iterable[str], retrieved: Iterable[str], k: int | None = None) -> float:
    """Mean Reciprocal Rank：首个命中相关项的位置倒数；无命中为 0。"""
    rel = set(relevant)
    if not rel:
        return 0.0
    ret = list(retrieved) if k is None else list(retrieved)[:k]
    for i, item in enumerate(ret, start=1):
        if item in rel:
            return 1.0 / i
    return 0.0


def retrieval_hit_rate(
    pairs: list[tuple[Iterable[str], Iterable[str]]], k: int
) -> tuple[float, int, int]:
    """批量 Recall@k：每对 (relevant, retrieved)。返回 (recall, hits, total)。"""
    total = len(pairs)
    if total == 0:
        return 0.0, 0, 0
    hits = sum(1 for rel, ret in pairs if recall_at_k(rel, ret, k) > 0.0)
    return hits / total, hits, total
