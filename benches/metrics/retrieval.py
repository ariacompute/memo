from __future__ import annotations

from typing import Iterable


def recall_at_k(relevant: Iterable[str], retrieved: Iterable[str], k: int) -> float:
    """Standard per-query Recall@k: whether any relevant item is retrieved within the top k."""
    rel = set(relevant)
    if not rel:
        return 0.0
    ret = list(retrieved)[:k]
    ret_set = set(ret)
    return 1.0 if rel & ret_set else 0.0


def mrr(relevant: Iterable[str], retrieved: Iterable[str], k: int | None = None) -> float:
    """Mean Reciprocal Rank: the reciprocal of the rank of the first relevant item; 0 if none hit."""
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
    """Batched Recall@k over (relevant, retrieved) pairs. Returns (recall, hits, total)."""
    total = len(pairs)
    if total == 0:
        return 0.0, 0, 0
    hits = sum(1 for rel, ret in pairs if recall_at_k(rel, ret, k) > 0.0)
    return hits / total, hits, total
