"""PersonaMem 基准（github bowen-upenn/PersonaMem；32k/128k/1M）。

数据（HuggingFace bowen-upenn/PersonaMem-v1）：
- shared_contexts_{N}.jsonl: 每行一个 JSON 对象 {hash: [{"role","content"}, ...]}
- questions_{N}.csv: persona_id / user_question_or_message / correct_answer("(c)") /
  all_options(JSON 字符串) / shared_context_id / end_index_in_shared_context
  end_index_in_shared_context 决定写入记忆的上下文截断点（严防信息泄漏）
指标：多选准确率（完全离线：选项与检索结果确定性匹配）。
"""

from __future__ import annotations

import csv
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from common.reporting import Score
from metrics.choice import multiple_choice_accuracy
from adapters import MemoBackend
from judge import Judge


NAME = "personamem"
DATASET_FILES = ("shared_contexts_32k.jsonl", "questions_32k.csv")


@dataclass
class Question:
    qid: str
    question: str
    gold: str
    question_type: str
    options: dict[str, str]
    shared_context_id: str
    end_index: int


@dataclass
class Dataset:
    questions: list[Question] = field(default_factory=list)
    shared_contexts: dict[str, list[str]] = field(default_factory=dict)
    size: int = 0


def _parse_options(raw: str) -> dict[str, str]:
    """解析 all_options 字段（JSON 字符串，如 ['(a) foo', '(b) bar']）为 {letter: text}。"""
    raw = (raw or "").strip()
    if not raw:
        return {}
    try:
        items = json.loads(raw)
    except (json.JSONDecodeError, ValueError):
        return {}
    opts: dict[str, str] = {}
    for it in items:
        it = it.strip()
        m = re.match(r"^\(?([A-Za-z])\)?\s*(.*)$", it, re.DOTALL)
        if m:
            opts[m.group(1).upper()] = m.group(2).strip()
    return opts


def load(path: Path, limit: int | None = None) -> Dataset:
    p = Path(path)
    shared: dict[str, list[str]] = {}
    sc_path = None
    for variant in ("shared_contexts_32k.jsonl", "shared_contexts_128k.jsonl", "shared_contexts_1M.jsonl"):
        if (p / variant).exists():
            sc_path = p / variant
            break
    if sc_path:
        with sc_path.open(encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                obj = json.loads(line)
                if "context" in obj:
                    # 合成 fixture 格式：{shared_context_id, context:[str,...]}
                    shared[obj["shared_context_id"]] = list(obj["context"])
                else:
                    # 真实格式：{hash: [{"role","content"}, ...]}
                    for hash_key, messages in obj.items():
                        shared[hash_key] = [m.get("content", "") for m in messages if isinstance(m, dict)]

    qs: list[Question] = []
    q_path = None
    for variant in ("questions_32k.csv", "questions_128k.csv", "questions_1M.csv"):
        if (p / variant).exists():
            q_path = p / variant
            break
    if q_path:
        with q_path.open(encoding="utf-8", newline="") as f:
            reader = csv.DictReader(f)
            real_fmt = "all_options" in (reader.fieldnames or [])
            f.seek(0)
            reader = csv.DictReader(f)
            for row in reader:
                if real_fmt:
                    opts = _parse_options(row.get("all_options", ""))
                    ca = (row.get("correct_answer") or "").strip()
                    gm = re.match(r"^\(?([A-Za-z])\)?", ca)
                    gold = gm.group(1).upper() if gm else ""
                    question = row.get("user_question_or_message", "")
                else:
                    opts = {k: row[k] for k in ("A", "B", "C", "D") if k in row and row.get(k)}
                    gold = (row.get("answer") or "").strip().upper()
                    question = row.get("question", "")
                end = int(row.get("end_index_in_shared_context", 0) or 0)
                qs.append(
                    Question(
                        qid=row.get("question_id", ""),
                        question=question,
                        gold=gold,
                        question_type=row.get("question_type", ""),
                        options=opts,
                        shared_context_id=row.get("shared_context_id", ""),
                        end_index=end,
                    )
                )
                if limit and len(qs) >= limit:
                    break
    return Dataset(questions=qs, shared_contexts=shared, size=len(qs))


def _ingest(backend: MemoBackend, ds: Dataset) -> None:
    backend.reset()
    skipped = 0
    for q in ds.questions:
        ctx = ds.shared_contexts.get(q.shared_context_id, [])
        # 仅写入截断点之前的上下文，避免信息泄漏
        truncated = ctx[: q.end_index] if q.end_index else ctx
        for text in truncated:
            norm = text.strip()
            if not norm or not any(ch.isalnum() for ch in norm):
                skipped += 1
                continue
            backend.add(norm, {"memo_type": "working"})
    if skipped:
        print(f"[personamem] skipped {skipped} empty/symbol-only context turns", file=sys.stderr)


def run(
    backend: MemoBackend,
    dataset: Dataset,
    judge: Judge | None,
    top_k: int,
    do_ingest: bool = True,
) -> list[Score]:
    if do_ingest:
        _ingest(backend, dataset)

    rows = []
    for q in dataset.questions:
        hits = backend.search(q.question, top_k)
        # 检索结果拼接作为「预测文本」，与正确选项匹配
        pred = " ".join(h.content for h in hits)
        rows.append((q.options, q.gold, pred))

    acc, correct, total = multiple_choice_accuracy(rows)
    return [Score(name="multiple_choice_accuracy", value=acc, requires_llm=False, subset="all")]
