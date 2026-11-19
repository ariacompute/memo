from __future__ import annotations

"""Loaders for A2 retrieval-quality query corpora.

Sources
-------
- ``synthetic`` / ``synthetic_v1`` : benches/data/synthetic_retrieval.json (small, ~8 queries)
- ``synthetic_v2``           : benches/data/synthetic_retrieval_v2.json (>=50 queries, realistic mix)
- ``track_b:locomo_refined`` : reuse the real LoCoMo-refined QA set as a query corpus
- ``track_b:halumem``        : reuse the real HaluMem memory points as a query corpus (best-effort proxy)

Each loader returns ``{"corpus": [{"id", "content"}], "queries": [{"id", "text", "relevant_ids"}]}``
or ``None`` when the source is unavailable, so the harness can skip with a reason instead of
fabricating numbers.
"""

import json
import os
from typing import Any

from datasets import resolve_dataset

DATA_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), "data")


def _jsonl(path: str) -> list[dict[str, Any]]:
    if not os.path.isfile(path):
        return []
    rows: list[dict[str, Any]] = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def load_dataset(name: str) -> dict[str, Any] | None:
    key = (name or "").strip().lower()
    if key in {"synthetic", "synthetic_v1"}:
        return _load_json_file("synthetic_retrieval.json")
    if key == "synthetic_v2":
        return _load_json_file("synthetic_retrieval_v2.json")
    if key.startswith("track_b:"):
        return load_track_b(key.split(":", 1)[1])
    return None


def _load_json_file(fname: str) -> dict[str, Any] | None:
    path = os.path.join(DATA_DIR, fname)
    if not os.path.isfile(path):
        return None
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def load_track_b(name: str) -> dict[str, Any] | None:
    name = (name or "").strip().lower()
    if name == "locomo_refined":
        return _load_locomo_refined()
    if name == "halumem":
        return _load_halumem()
    return None


def _load_locomo_refined(max_queries: int = 200) -> dict[str, Any] | None:
    """Load LoCoMo-Refined QA as an A2 corpus.

    Real data uses ``evidence_messages[].dia_id/.text``; the bundled fixture uses
    ``evidence: [conversation_id]`` plus a separate conversations file. Both schemas are
    adapted here, and the resolved source (real/fixture) is reported to the harness.
    """
    try:
        resolved = resolve_dataset("locomo_refined")
    except (FileNotFoundError, KeyError):
        return None
    base = str(resolved.path)

    conv_docs: dict[str, str] = {}
    for obj in _jsonl(os.path.join(base, "conversations.jsonl")):
        cid = str(obj.get("conversation_id") or obj.get("sample_id") or "").strip()
        if not cid:
            continue
        turns: list[str] = []
        for turn in obj.get("conversation") or []:
            speaker = str(turn.get("speaker") or "").strip()
            text = str(turn.get("text") or "").strip()
            turns.append(f"{speaker}: {text}" if speaker else text)
        conv_docs[cid] = "\n".join(t for t in turns if t)

    docs: dict[str, str] = {}
    queries: list[dict[str, Any]] = []
    for obj in _jsonl(os.path.join(base, "questions.jsonl")):
        relevant: list[str] = []
        for m in obj.get("evidence_messages") or []:
            did = str(m.get("dia_id") or "").strip()
            txt = str(m.get("text") or "").strip()
            if did and txt:
                docs.setdefault(did, txt)
                relevant.append(did)
        if not relevant:
            for eid in obj.get("evidence") or []:
                key = str(eid).strip()
                if key in conv_docs:
                    docs.setdefault(key, conv_docs[key])
                    relevant.append(key)
        q = str(obj.get("question") or "").strip()
        if q and relevant:
            queries.append(
                {
                    "id": str(obj.get("qa_id") or len(queries)),
                    "text": q,
                    "relevant_ids": list(dict.fromkeys(relevant)),
                }
            )
        if len(queries) >= max_queries:
            break
    if not queries:
        return None
    corpus = [{"id": k, "content": v} for k, v in docs.items()]
    return {
        "source": "track_b:locomo_refined",
        "dataset_source": resolved.source,
        "corpus": corpus,
        "queries": queries,
    }


def _load_halumem(max_queries: int = 200) -> dict[str, Any] | None:
    """Reuse HaluMem as an A2 retrieval corpus from real or fixture data.

    The real single-file format stores ``sessions[].memory_points``; the bundled fixture
    splits memories and questions into three JSONL files. Both map to the same
    ``{corpus, queries}`` contract, with answers used to select relevant fixture memories.
    """
    try:
        resolved = resolve_dataset("halumem")
    except (FileNotFoundError, KeyError):
        return None
    base = str(resolved.path)
    docs: dict[str, str] = {}
    queries: list[dict[str, Any]] = []

    single = next(
        (
            os.path.join(base, name)
            for name in ("HaluMem-Medium.jsonl", "HaluMem-Long.jsonl")
            if os.path.isfile(os.path.join(base, name))
        ),
        None,
    )
    if single:
        for obj in _jsonl(single):
            for si, sess in enumerate(obj.get("sessions") or []):
                for mp in sess.get("memory_points") or []:
                    content = str(mp.get("memory_content") or "").strip()
                    if not content:
                        continue
                    did = f"{obj.get('uuid', 'x')}#{si}#{mp.get('index', '?')}"
                    docs.setdefault(did, content)
                    sentences = [s.strip() for s in content.split(".") if s.strip()]
                    qtext = sentences[0] if sentences else content
                    queries.append({"id": did, "text": qtext, "relevant_ids": [did]})
                    if len(queries) >= max_queries:
                        break
                if len(queries) >= max_queries:
                    break
            if len(queries) >= max_queries:
                break
    else:
        for obj in _jsonl(os.path.join(base, "memories.jsonl")):
            mid = str(obj.get("MemoryID") or obj.get("id") or obj.get("memory_id") or "").strip()
            content = str(obj.get("Content") or obj.get("content") or "").strip()
            if mid and content:
                docs.setdefault(mid, content)
        for obj in _jsonl(os.path.join(base, "questions.jsonl")):
            q = str(obj.get("Question") or obj.get("question") or "").strip()
            answers = obj.get("Answer") or obj.get("answer") or []
            if isinstance(answers, str):
                answers = [answers]
            relevant: list[str] = []
            for mid, content in docs.items():
                if any(
                    str(a).strip().lower() in content.lower()
                    for a in answers
                    if str(a).strip()
                ):
                    relevant.append(mid)
            if not relevant:
                fallback = str(
                    obj.get("UpdateMemoryID") or obj.get("update_memory_id") or ""
                ).strip()
                if fallback in docs:
                    relevant = [fallback]
            if q and relevant:
                queries.append(
                    {
                        "id": str(obj.get("QuestionID") or len(queries)),
                        "text": q,
                        "relevant_ids": list(dict.fromkeys(relevant)),
                    }
                )
            if len(queries) >= max_queries:
                break
    if not queries:
        return None
    corpus = [{"id": k, "content": v} for k, v in docs.items()]
    return {
        "source": "track_b:halumem",
        "dataset_source": resolved.source,
        "corpus": corpus,
        "queries": queries,
    }
