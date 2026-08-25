from __future__ import annotations

from dataclasses import asdict, dataclass, field


@dataclass
class Score:
    """统一指标结构。

    - value: None 表示未计算（配合 skipped/reason，如 judge 指标缺凭据）。
    - requires_llm: 报告据此分列离线 / LLM 管线条件。
    - subset: 细分维度（category / question_type / 子任务）。
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
