from __future__ import annotations

from dataclasses import asdict, dataclass, field


@dataclass
class Score:
    """Unified metric structure.

    - value: None means not computed (paired with skipped/reason, e.g. a judge metric lacking credentials).
    - requires_llm: the report uses this to separate offline / LLM-pipeline conditions.
    - subset: a finer-grained dimension (category / question_type / sub-task).
    """

    name: str
    value: float | None = None
    requires_llm: bool = False
    subset: str = ""
    skipped: bool = False
    reason: str = ""

    def as_dict(self) -> dict:
        return asdict(self)


def skipped_score(name: str, reason: str, requires_llm: bool = True, subset: str = "") -> Score:
    return Score(name=name, value=None, requires_llm=requires_llm, subset=subset, skipped=True, reason=reason)
