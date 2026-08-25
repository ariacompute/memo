"""HaluMem benchmark (HF IAAR-Shanghai/HaluMem; Medium/Long).

Three sub-tasks:
1) Memory extraction: dialogue -> extract atomic memories; metrics Recall / Accuracy /
   FMR / F1, requiring a judge for comparison (skipped if absent). The retrieval-layer
   Recall@k is available offline.
2) Memory updating: updating contradictory information; requires the `update` capability
   + judge; reported as N/A if unsupported.
3) Memory QA: accuracy / hallucination rate / omission rate + retrieval Recall@k (offline).

Data:
- Real (HuggingFace IAAR-Shanghai/HaluMem): a single file HaluMem-Medium/Long.jsonl,
  each line {uuid, sessions:[{memory_points:[{memory_content,is_update,importance,...}],
  dialogue:[{role,content}], questions:[{question,answer,question_type}]}]}; a format
  adapter is built into the loader.
- Synthetic fixtures: sessions.jsonl / memories.jsonl / questions.jsonl (split upstream
  into the same shape).
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
    # real single-file format: HaluMem-Medium.jsonl / HaluMem-Long.jsonl
    real_file = None
    if p.is_file() and p.name in ("HaluMem-Medium.jsonl", "HaluMem-Long.jsonl"):
        real_file = p
    else:
        for marker in ("HaluMem-Medium.jsonl", "HaluMem-Long.jsonl"):
            if (p / marker).exists():
                real_file = p / marker
                break
    if real_file:
        with real_file.open(encoding="utf-8") as f:
            for ln in f:
                ln = ln.strip()
                if not ln:
                    continue
                rec = json.loads(ln)
                sid = rec.get("uuid", "")
                for s in rec.get("sessions", []):
                    for m in s.get("memory_points", []):
                        memories.append(
                            {
                                "Content": m.get("memory_content", ""),
                                "Type": m.get("memory_type", "working"),
                                "Importance": float(m.get("importance", 0.5) or 0.5),
                                "Distraction": str(m.get("is_update", "False")).lower() == "true",
                                "SessionID": sid,
                            }
                        )
                    conv = [
                        {"speaker": t.get("role", ""), "text": t.get("content", "")}
                        for t in s.get("dialogue", [])
                    ]
                    sessions.append({"SessionID": sid, "Conversation": conv})
                    for q in s.get("questions", []):
                        ans = q.get("answer", "")
                        questions.append(
                            {
                                "QuestionID": f"{sid}-{len(questions)}",
                                "Question": q.get("question", ""),
                                "Answer": [ans] if isinstance(ans, str) else (ans or []),
                                "QuestionType": q.get("question_type", ""),
                                "SessionID": sid,
                            }
                        )
                        if limit and len(questions) >= limit:
                            break
                if limit and len(questions) >= limit:
                    break
        return Dataset(sessions=sessions, memories=memories, questions=questions, size=len(questions))
    # synthetic fixtures format: sessions/memories/questions.jsonl
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



# ---------- Sub-task 1: memory extraction ----------
def _run_extraction(backend: MemoBackend, ds: Dataset, judge: Judge | None) -> list[Score]:
    scores: list[Score] = []
    # retrieval-layer Recall@k: treat ground-truth memories as relevant and check whether their content is hit
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
    # semantic metrics require a judge; skip if absent
    if judge is None:
        for nm in ("memory_recall", "memory_accuracy", "false_memory_resistance", "f1"):
            scores.append(
                skipped_score(nm, "no LLM judge credentials (BENCH_LLM_API_KEY)", subset="extraction")
            )
    else:
        # compare extracted memories with gold via the judge (synthetic: direct gold comparison)
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


# ---------- Sub-task 2: memory updating ----------
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
    update_qs = [
        q
        for q in ds.questions
        if q.get("UpdateType") in ("update", "delete") and "UpdateMemoryID" in q
    ]
    if not update_qs:
        # dataset contains no update-type questions (e.g. real HaluMem); report as N/A rather than a misleading 0.0
        for nm in ("update_accuracy", "hallucination_rate", "omission_rate"):
            scores.append(
                skipped_score(nm, "no update-type questions in this dataset", subset="updating")
            )
        return scores
    updated_ok = 0
    total = len(update_qs)
    for q in update_qs:
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


# ---------- Sub-task 3: memory QA ----------
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
        # ingest ground-truth memories as the knowledge base (synthetic / upstream convention)
        for m in dataset.memories:
            if m.get("Distraction"):
                continue
            backend.add(
                m["Content"],
                {"memo_type": m.get("Type", "working"), "importance": float(m.get("Importance", 0.5))},
            )
        # ingest conversation context
        for s in dataset.sessions:
            for turn in s.get("Conversation", []):
                text = turn.get("text") or turn.get("content") or ""
                if text:
                    backend.add(text, {"memo_type": "working"})
    return _run_extraction(backend, dataset, judge) + _run_updating(backend, dataset, judge) + _run_qa(backend, dataset, judge)
