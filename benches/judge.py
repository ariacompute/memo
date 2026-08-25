from __future__ import annotations

"""Optional LLM judge client (OpenAI-compatible chat completions).

Design:
- `from_env()` reads BENCH_LLM_API_KEY / OPENAI_API_KEY + BENCH_LLM_BASE_URL / BENCH_LLM_MODEL.
- When no credentials are present it returns None, and the caller skips judge-affected
  metrics accordingly (no fabricated scores, no silent omission).
- The strict-judgement prompt aligns with the LoCoMo-Refined convention: the prediction must
  be non-contradictory with the gold answer, complete (no missing key fact), non-redundant, and
  match the gold answer's granularity (e.g., date/place at the same specificity).
- Uses the standard-library urllib for HTTP to avoid a hard dependency on the openai SDK.
- Never prints the API key.
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
    """OpenAI-compatible judge; construction failure or missing credentials should yield None (see from_env)."""

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
        # tolerant fallback: contains "yes" but is not negated
        return "yes" in t and "not" not in t and "no" not in t

    def judge(self, question: str, gold: str, pred: str) -> bool | None:
        """Return True/False; on network/parse failure return None (the caller treats it as undecidable and skips the sample)."""
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
        """Return None when no credentials are present."""
        api_key = os.environ.get("BENCH_LLM_API_KEY") or os.environ.get("OPENAI_API_KEY")
        if not api_key:
            return None
        base_url = os.environ.get("BENCH_LLM_BASE_URL") or "https://api.openai.com/v1"
        model = model_override or os.environ.get("BENCH_LLM_MODEL") or "gpt-4o-mini"
        return Judge(api_key=api_key, model=model, base_url=base_url)
