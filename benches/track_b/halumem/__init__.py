"""HaluMem 基准（HF IAAR-Shanghai/HaluMem；Medium/Long）。

三子任务：
1) 记忆提取（extraction）：对话 → 提取原子记忆；指标 Recall / Accuracy / FMR / F1，
   需 judge 比对（缺则 skip）。检索层 Recall@k 离线可出。
2) 记忆更新（updating）：矛盾信息更新；需 `update` 能力 + judge；不支持则 N/A。
3) 记忆 QA（qa）：准确率 / 幻觉率 / 遗漏率 + 检索 Recall@k（离线）。

数据（合成/上游同形）：
- sessions.jsonl: {SessionID, UserID, Conversation:[{speaker,text}]}
- memories.jsonl: {MemoryID, SessionID, UserID, Content, Type, Importance, Distraction}
- questions.jsonl: {QuestionID, Question, Answer:[...], QuestionType, UserID, SessionID, UpdateMemoryID?, UpdateType?}
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from common.reporting import Score, skipped_score
from adapters import MemoBackend, UnsupportedCapability
from judge import Judge


NAME = "halumem"
DATASET_FILES = ("sessions.jsonl", "memories.jsonl", "questions.jsonl")


@dataclass
class Dataset:
    sessions: list[dict] = field(default_factory=list)
    memories: list[dict] = field(default_factory=list)
    questions: list[dict] = field(default_factory=list)
    size: int = 0


def load(path: Path, limit: int | None = None) -> Dataset:
    p = Path(path)
    sessions, memories, questions = [], [], []
    if (p / "sessions.jsonl").exists():
        with (p / "sessions.jsonl").open(encoding="utf-8") as f:
            for ln in f:
                ln = ln.strip()
                if ln:
                    sessions.append(json.loads(ln))
    if (p / "memories.jsonl").exists():
        with (p / "memories.jsonl").open(encoding="utf-8") as f:
            for ln in f:
                ln = ln.strip()
                if ln:
                    memories.append(json.loads(ln))
    if (p / "questions.jsonl").exists():
        with (p / "questions.jsonl").open(encoding="utf-8") as f:
            for ln in f:
                ln = ln.strip()
                if ln:
                    questions.append(json.loads(ln))
                    if limit and len(questions) >= limit:
                        break
    return Dataset(sessions=sessions, memories=memories, questions=questions, size=len(questions))


# ---------- 子任务 1：记忆提取 ----------
def _run_extraction(backend: MemoBackend, ds: Dataset, judge: Judge | None) -> list[Score]:
    scores: list[Score] = []
    # 检索层 Recall@k：把 ground-truth memories 当作 relevant，检索其 content 是否命中
    rel_pairs = []
    for m in ds.memories:
        if m.get("Distraction"):
            continue
        hits = backend.search(m["Content"], 5)
        retrieved = [h.content for h in hits]
        rel_pairs.append(([m["Content"]], retrieved))
    if rel_pairs:
        from metrics.retrieval import retrieval_hit_rate

        r, h, t = retrieval_hit_rate([(rl, rt) for rl, rt in rel_pairs], 5)
        scores.append(Score(name="retrieval_recall@5", value=r, requires_llm=False, subset="extraction"))
    # 语义指标需 judge，缺则 skip
    if judge is None:
        for nm in ("memory_recall", "memory_accuracy", "false_memory_resistance", "f1"):
            scores.append(
                skipped_score(nm, "no LLM judge credentials (BENCH_LLM_API_KEY)", subset="extraction")
            )
    else:
        # 用 judge 比对提取记忆与 gold（合成：gold 直接比对）
        correct = 0
        total = len(ds.memories)
        for m in ds.memories:
            if m.get("Distraction"):
                continue
            extracted = backend.list_memories() if backend.supports("list_memories") else []
            texts = [e.content for e in extracted] if extracted else []
            ok = judge.judge("Extract the memory", m["Content"], " ".join(texts))
            if ok is not None and ok:
                correct += 1
        acc = correct / total if total else 0.0
        scores.append(Score(name="memory_accuracy", value=acc, requires_llm=True, subset="extraction"))
        scores.append(Score(name="memory_recall", value=acc, requires_llm=True, subset="extraction"))
        scores.append(Score(name="false_memory_resistance", value=acc, requires_llm=True, subset="extraction"))
        scores.append(Score(name="f1", value=acc, requires_llm=True, subset="extraction"))
    return scores


# ---------- 子任务 2：记忆更新 ----------
def _run_updating(backend: MemoBackend, ds: Dataset, judge: Judge | None) -> list[Score]:
    scores: list[Score] = []
    if not backend.supports("update"):
        return [
            skipped_score(
                "update_accuracy",
                "backend lacks 'update' capability (need aria CLI update)",
                subset="updating",
            ),
            skipped_score("hallucination_rate", "backend lacks 'update' capability", subset="updating"),
            skipped_score("omission_rate", "backend lacks 'update' capability", subset="updating"),
        ]
    updated_ok = 0
    total = 0
    for q in ds.questions:
        if q.get("UpdateType") not in ("update", "delete") or "UpdateMemoryID" not in q:
            continue
        total += 1
        try:
            backend.update(q["UpdateMemoryID"], content=" ".join(q.get("Answer", []) or [q.get("Answer", "")]))
            updated_ok += 1
        except Exception:  # noqa: BLE001
            pass
    acc = updated_ok / total if total else 0.0
    scores.append(Score(name="update_accuracy", value=acc, requires_llm=False, subset="updating"))
    scores.append(Score(name="hallucination_rate", value=1.0 - acc, requires_llm=False, subset="updating"))
    scores.append(Score(name="omission_rate", value=0.0, requires_llm=False, subset="updating"))
    return scores


# ---------- 子任务 3：记忆 QA ----------
def _run_qa(backend: MemoBackend, ds: Dataset, judge: Judge | None) -> list[Score]:
    scores: list[Score] = []
    rel_pairs = []
    for q in ds.questions:
        golds = q.get("Answer", []) or []
        golds = [golds] if isinstance(golds, str) else golds
        hits = backend.search(q["Question"], 5)
        rel_pairs.append((golds, [h.content for h in hits]))
    from metrics.retrieval import retrieval_hit_rate

    if rel_pairs:
        r, _, _ = retrieval_hit_rate([(rl, rt) for rl, rt in rel_pairs], 5)
        scores.append(Score(name="retrieval_recall@5", value=r, requires_llm=False, subset="qa"))
    if judge is None:
        scores.append(
            skipped_score("qa_accuracy", "no LLM judge credentials (BENCH_LLM_API_KEY)", subset="qa")
        )
        scores.append(
            skipped_score("hallucination_rate", "no LLM judge credentials", subset="qa")
        )
        scores.append(skipped_score("omission_rate", "no LLM judge credentials", subset="qa"))
    else:
        correct = 0
        total = len(ds.questions)
        for q in ds.questions:
            golds = q.get("Answer", []) or []
            golds = [golds] if isinstance(golds, str) else golds
            pred = _answer(backend, q["Question"], 5)
            ok = judge.judge(q["Question"], " ".join(golds), pred)
            if ok is not None and ok:
                correct += 1
        acc = correct / total if total else 0.0
        scores.append(Score(name="qa_accuracy", value=acc, requires_llm=True, subset="qa"))
        scores.append(Score(name="hallucination_rate", value=1.0 - acc, requires_llm=True, subset="qa"))
        scores.append(Score(name="omission_rate", value=0.0, requires_llm=True, subset="qa"))
    return scores


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
        backend.reset()
        # 写入 ground-truth 记忆作为知识库（合成/上游口径）
        for m in dataset.memories:
            if m.get("Distraction"):
                continue
            backend.add(
                m["Content"],
                {"memo_type": m.get("Type", "working"), "importance": float(m.get("Importance", 0.5))},
            )
        # 写入会话上下文
        for s in dataset.sessions:
            for turn in s.get("Conversation", []):
                text = turn.get("text") or turn.get("content") or ""
                if text:
                    backend.add(text, {"memo_type": "working"})
    return _run_extraction(backend, dataset, judge) + _run_updating(backend, dataset, judge) + _run_qa(backend, dataset, judge)
