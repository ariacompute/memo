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

DATA_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), "data")


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
    qpath = os.path.join(DATA_DIR, "locomo_refined", "questions.jsonl")
    if not os.path.isfile(qpath):
        return None
    docs: dict[str, str] = {}
    queries: list[dict[str, Any]] = []
    with open(qpath, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            relevant: list[str] = []
            for m in obj.get("evidence_messages") or []:
                did = m.get("dia_id")
                txt = (m.get("text") or "").strip()
                if did and txt:
                    docs.setdefault(did, txt)
                    relevant.append(did)
            q = (obj.get("question") or "").strip()
            if q and relevant:
                queries.append(
                    {"id": obj.get("qa_id", str(len(queries))), "text": q, "relevant_ids": relevant}
                )
            if len(queries) >= max_queries:
                break
    corpus = [{"id": k, "content": v} for k, v in docs.items()]
    return {"source": "track_b:locomo_refined", "corpus": corpus, "queries": queries}


def _load_halumem(max_queries: int = 200) -> dict[str, Any] | None:
    """Best-effort reuse of HaluMem memory points as a retrieval corpus.

    HaluMem is an operation-level (hallucination) benchmark, not a retrieval one, so we build a
    heuristic but real-data retrieval task: each memory point is indexed as a document, and a
    *partial* query (its leading sentences) is used to retrieve the full memory point. This keeps
    the corpus and queries grounded in real data while exercising lexical+semantic retrieval.
    """
    path = os.path.join(DATA_DIR, "halumem", "HaluMem-Medium.jsonl")
    if not os.path.isfile(path):
        return None
    docs: dict[str, str] = {}
    queries: list[dict[str, Any]] = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            for si, sess in enumerate(obj.get("sessions") or []):
                for mp in sess.get("memory_points") or []:
                    content = (mp.get("memory_content") or "").strip()
                    if not content:
                        continue
                    did = f"{obj.get('uuid', 'x')}#{si}#{mp.get('index', '?')}"
                    docs.setdefault(did, content)
                    # partial query: first sentence(s) -> retrieve the full memory point
                    sentences = [s.strip() for s in content.split(".") if s.strip()]
                    qtext = sentences[0] if sentences else content
                    queries.append({"id": did, "text": qtext, "relevant_ids": [did]})
                    if len(queries) >= max_queries:
                        break
                if len(queries) >= max_queries:
                    break
            if len(queries) >= max_queries:
                break
    corpus = [{"id": k, "content": v} for k, v in docs.items()]
    return {"source": "track_b:halumem", "corpus": corpus, "queries": queries}
