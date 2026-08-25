from __future__ import annotations

"""可选的 LLM judge 客户端（OpenAI 兼容 chat completions）。

设计：
- `from_env()` 读取 BENCH_LLM_API_KEY / OPENAI_API_KEY + BENCH_LLM_BASE_URL / BENCH_LLM_MODEL。
- 无凭据时返回 None，调用方据此 skip 受 judge 影响的指标（不伪造分数、不静默忽略）。
- 严格判定 prompt 对齐 LoCoMo-Refined 口径：预测需与真值不矛盾、完整不冗余、时间/地点粒度与真值一致。
- 使用标准库 urllib 发起 HTTP，避免强依赖 openai SDK。
- 禁止打印 API Key。
"""

import json
import os
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Callable

_SYSTEM = (
    "You are a strict evaluation judge for long-term memory question answering. "
    "Answer ONLY 'Yes' or 'No'. A predicted answer is correct iff it is not "
    "contradictory with the gold answer, is complete (no missing key fact), is not "
    "redundant, and matches the gold answer's granularity (e.g., a date/place at the "
    "same specificity). For multiple-info or open-ended questions, partial correctness "
    "is NOT sufficient."
)

_USER_TMPL = (
    "Question: {question}\n"
    "Gold answer: {gold}\n"
    "Predicted answer: {pred}\n\n"
    "Is the predicted answer correct given the criteria above? Reply 'Yes' or 'No'."
)


@dataclass
class JudgeStats:
    model: str
    calls: int = 0
    errors: int = 0
    yes: int = 0


class Judge:
    """OpenAI 兼容 judge；构造失败或凭据缺失应返回 None（见 from_env）。"""

    def __init__(
        self,
        api_key: str,
        model: str,
        base_url: str = "https://api.openai.com/v1",
        timeout_s: float = 30.0,
        parse: Callable[[str], bool] | None = None,
    ) -> None:
        self.api_key = api_key
        self.model = model
        self.base_url = base_url.rstrip("/")
        self.timeout_s = timeout_s
        self.stats = JudgeStats(model=model)
        self._parse = parse or self._default_parse

    @staticmethod
    def _default_parse(text: str) -> bool:
        t = text.strip().lower()
        if t.startswith("yes"):
            return True
        if t.startswith("no"):
            return False
        # 容错：含 yes 但非否定
        return "yes" in t and "not" not in t and "no" not in t

    def judge(self, question: str, gold: str, pred: str) -> bool | None:
        """返回 True/False；网络/解析失败返回 None（调用方视为无法判定，skip 该样本）。"""
        self.stats.calls += 1
        body = {
            "model": self.model,
            "messages": [
                {"role": "system", "content": _SYSTEM},
                {
                    "role": "user",
                    "content": _USER_TMPL.format(
                        question=question, gold=gold, pred=pred
                    ),
                },
            ],
            "temperature": 0.0,
        }
        req = urllib.request.Request(
            f"{self.base_url}/chat/completions",
            data=json.dumps(body).encode("utf-8"),
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.api_key}",
            },
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=self.timeout_s) as resp:
                data: Any = json.loads(resp.read().decode("utf-8"))
            content = data["choices"][0]["message"]["content"]
            ok = self._parse(content)
            if ok:
                self.stats.yes += 1
            return ok
        except Exception:
            self.stats.errors += 1
            return None

    @staticmethod
    def from_env(model_override: str | None = None) -> "Judge | None":
        """无凭据返回 None。"""
        api_key = os.environ.get("BENCH_LLM_API_KEY") or os.environ.get("OPENAI_API_KEY")
        if not api_key:
            return None
        base_url = os.environ.get("BENCH_LLM_BASE_URL") or "https://api.openai.com/v1"
        model = model_override or os.environ.get("BENCH_LLM_MODEL") or "gpt-4o-mini"
        return Judge(api_key=api_key, model=model, base_url=base_url)
