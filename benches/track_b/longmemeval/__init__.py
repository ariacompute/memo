"""LongMemEval 基准（github xiaowu0162/LongMemEval；S/M/Oracle 三变体）。

数据：longmemeval_{s,m,oracle}.json，列表元素：
    {qa_id, user_id, qa_type, question, answer, haystack_sessions:[{session_id, session_time, chat:[{role,content}]}]}
指标：
- 离线：检索层 Recall@k（基于 gold answer 是否命中检索结果）。
- judge：QA 准确率（严格判定，需 OpenAI 兼容 LLM，缺则 skip）。
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from common.reporting import Score, skipped_score
from metrics.retrieval import retrieval_hit_rate
from adapters import MemoBackend
from judge import Judge


NAME = "longmemeval"
DATASET_FILES = ("longmemeval_s.json", "longmemeval_m.json", "longmemeval_oracle.json")


@dataclass
class Dataset:
    items: list[dict] = field(default_factory=list)
    size: int = 0


def load(path: Path, limit: int | None = None) -> Dataset:
    p = Path(path)
    items: list[dict] = []
    # 优先真实变体文件；fixture 仅 s
    for variant in ("longmemeval_s.json", "longmemeval_m.json", "longmemeval_oracle.json"):
        fp = p / variant
        if fp.exists():
            with fp.open(encoding="utf-8") as f:
                for obj in json.load(f):
                    items.append(obj)
                    if limit and len(items) >= limit:
                        break
            break  # 仅加载首个可用变体，避免 fixture 与真实重复
    return Dataset(items=items, size=len(items))


def _ingest(backend: MemoBackend, ds: Dataset) -> None:
    backend.reset()
    skipped = 0
    for item in ds.items:
        for sess in item.get("haystack_sessions", []) or []:
            # 真实格式：session 为 [{role,content},...] 的列表；
            # 合成 fixture：session 为 {session_id, chat:[...]} 的字典。
            turns = sess if isinstance(sess, list) else sess.get("chat", []) or []
            for turn in turns:
                if not isinstance(turn, dict):
                    continue
                text = (turn.get("content") or turn.get("text") or "").strip()
                if not text or not any(ch.isalnum() for ch in text):
                    skipped += 1
                    continue
                backend.add(text, {"memo_type": "working"})
    if skipped:
        print(f"[longmemeval] skipped {skipped} empty/symbol-only turns", file=sys.stderr)


def _answer(backend: MemoBackend, question: str, top_k: int) -> str:
    return " ".join(h.content for h in backend.search(question, top_k))


def run(
    backend: MemoBackend,
    dataset: Dataset,
    judge: Judge | None,
    top_k: int,
    do_ingest: bool = True,
) -> list[Score]:
    if do_ingest:
        _ingest(backend, dataset)

    scores: list[Score] = []
    rel_pairs = []
    for item in dataset.items:
        gold = item.get("answer", "")
        gold = gold if isinstance(gold, str) else " ".join(gold)
        hits = backend.search(item["question"], 5)
        rel_pairs.append(([gold], [h.content for h in hits]))
    if rel_pairs:
        r, _, _ = retrieval_hit_rate(rel_pairs, 5)
        scores.append(Score(name="retrieval_recall@5", value=r, requires_llm=False, subset="all"))

    if judge is None:
        scores.append(
            skipped_score("qa_accuracy", "no LLM judge credentials (BENCH_LLM_API_KEY)", subset="all")
        )
    else:
        correct = 0
        total = len(dataset.items)
        for item in dataset.items:
            gold = item.get("answer", "")
            gold = gold if isinstance(gold, str) else " ".join(gold)
            pred = _answer(backend, item["question"], 5)
            ok = judge.judge(item["question"], gold, pred)
            if ok is not None and ok:
                correct += 1
        acc = correct / total if total else 0.0
        scores.append(Score(name="qa_accuracy", value=acc, requires_llm=True, subset="all"))
    return scores
