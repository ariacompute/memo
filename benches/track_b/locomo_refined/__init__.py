"""LoCoMo-Refined 基准（CC BY-NC 4.0；github mem-eval-suite/LoCoMo_refined）。

数据：
- questions.jsonl: {qa_id, sample_id, question, answer:[...], category, evidence:[...]}
- conversations.jsonl: {conversation_id, conversation:[{speaker,text}]}
指标：
- 离线：token-F1 / BLEU（多答案取最优），按 category 分组。
- judge：严格 judge 准确率（需 OpenAI 兼容 LLM，缺则 skip）。
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from common.reporting import Score, skipped_score
from metrics.lexical import bleu, multiple_candidate_max, token_f1
from adapters import MemoBackend
from judge import Judge


NAME = "locomo_refined"
DATASET_FILES = ("questions.jsonl", "conversations.jsonl")


@dataclass
class Dataset:
    questions: list[dict] = field(default_factory=list)
    conversations: dict[str, list[dict]] = field(default_factory=dict)
    size: int = 0


def _conv_turns(obj: dict) -> list[dict]:
    """Extract a flat turn list from either the real or synthetic conversation record.

    Real LoCoMo-Refined (mem-eval-suite/LoCoMo_refined) layout:
        {"sample_id":..., "sessions":[{"messages":[{"text",...}]}]}
    Synthetic fixture layout:
        {"conversation_id":..., "conversation":[{"speaker","text"}]}
    """
    if "sessions" in obj:
        turns: list[dict] = []
        for sess in obj.get("sessions", []):
            for m in sess.get("messages", []):
                turns.append(m)
        return turns
    return obj.get("conversation", [])


def load(path: Path, limit: int | None = None) -> Dataset:
    conv_path = Path(path) / "conversations.jsonl"
    q_path = Path(path) / "questions.jsonl"
    conversations: dict[str, list[dict]] = {}
    if conv_path.exists():
        with conv_path.open(encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                obj = json.loads(line)
                cid = obj.get("conversation_id") or obj.get("sample_id")
                if cid is None:
                    continue
                conversations[str(cid)] = _conv_turns(obj)
    questions: list[dict] = []
    if q_path.exists():
        with q_path.open(encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                questions.append(json.loads(line))
                if limit and len(questions) >= limit:
                    break
    return Dataset(questions=questions, conversations=conversations, size=len(questions))


def _ingest(backend: MemoBackend, ds: Dataset) -> None:
    backend.reset()
    skipped = 0
    for cid, turns in ds.conversations.items():
        for turn in turns:
            text = turn.get("text") or turn.get("content") or ""
            norm = text.strip()
            # 跳过纯空白或纯标点/符号（embedder 视为空 embedding）的对话轮。
            if not norm or not any(ch.isalnum() for ch in norm):
                skipped += 1
                continue
            backend.add(norm, {"memo_type": "working"})
    if skipped:
        print(f"[locomo] skipped {skipped} empty/symbol-only turns", file=sys.stderr)


def _answer(backend: MemoBackend, question: str, top_k: int) -> str:
    hits = backend.search(question, top_k)
    return " ".join(h.content for h in hits)


def run(
    backend: MemoBackend,
    dataset: Dataset,
    judge: Judge | None,
    top_k: int,
    do_ingest: bool = True,
) -> list[Score]:
    if do_ingest:
        _ingest(backend, dataset)

    f1_by_cat: dict[str, list[float]] = {}
    bleu_by_cat: dict[str, list[float]] = {}
    judge_correct = 0
    judge_total = 0

    for q in dataset.questions:
        golds = q.get("answer", []) or []
        if isinstance(golds, str):
            golds = [golds]
        pred = _answer(backend, q["question"], top_k)
        cat = q.get("category", "all")
        f1_by_cat.setdefault(cat, []).append(multiple_candidate_max(token_f1, pred, golds))
        bleu_by_cat.setdefault(cat, []).append(bleu(pred, golds))
        if judge is not None:
            ok = judge.judge(q["question"], " ".join(golds), pred)
            if ok is not None:
                judge_total += 1
                judge_correct += 1 if ok else 0

    scores: list[Score] = []
    for cat, vals in f1_by_cat.items():
        scores.append(Score(name="f1", value=sum(vals) / len(vals), requires_llm=False, subset=cat))
    for cat, vals in bleu_by_cat.items():
        scores.append(Score(name="bleu", value=sum(vals) / len(vals), requires_llm=False, subset=cat))
    if judge is not None:
        acc = judge_correct / judge_total if judge_total else 0.0
        scores.append(Score(name="judge_accuracy", value=acc, requires_llm=True, subset="all"))
    else:
        scores.append(
            skipped_score("judge_accuracy", "no LLM judge credentials (BENCH_LLM_API_KEY)", subset="all")
        )
    return scores
