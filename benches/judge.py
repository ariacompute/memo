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
import sys
import time
import urllib.request
from urllib.error import HTTPError
from dataclasses import dataclass, field
from typing import Any, Callable

# Transient failures worth retrying (with backoff): timeout / network blips,
# and these HTTP statuses (rate-limit / server errors).
_RETRY_HTTP = frozenset({429, 500, 502, 503, 504})
_TRANSIENT = (TimeoutError, HTTPError)  # HTTPError covered for URLError-wrapped timeouts

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
        self._last_error: str | None = None

    @staticmethod
    def _default_parse(text: str) -> bool:
        t = text.strip().lower()
        if t.startswith("yes"):
            return True
        if t.startswith("no"):
            return False
        # tolerant fallback: contains "yes" but is not negated
        return "yes" in t and "not" not in t and "no" not in t

    @staticmethod
    def _extract_content(data: Any) -> str:
        """Tolerate OpenAI chat, text-completion, and streaming-shaped responses."""
        if not isinstance(data, dict):
            raise ValueError("response is not a JSON object")
        choices = data.get("choices")
        if not isinstance(choices, list) or not choices:
            raise ValueError(f"missing/empty 'choices' (keys={list(data)[:5]})")
        ch = choices[0]
        if not isinstance(ch, dict):
            raise ValueError("choice entry is not an object")
        if "message" in ch and isinstance(ch["message"], dict) and "content" in ch["message"]:
            return str(ch["message"]["content"])
        if "text" in ch:
            return str(ch["text"])
        if "delta" in ch and isinstance(ch["delta"], dict) and "content" in ch["delta"]:
            return str(ch["delta"]["content"])
        raise ValueError(f"unrecognized choice shape: {list(ch)[:5]}")

    def _record_error(self, exc: Exception, tried: list[str]) -> None:
        """Surface judge failures (HTTP status/body or exception) instead of swallowing them."""
        if isinstance(exc, HTTPError):
            try:
                detail = exc.read().decode("utf-8", "replace")[:400]
            except Exception:  # noqa: BLE001
                detail = ""
            msg = f"HTTP {exc.code} {exc.reason} :: {detail}"
        else:
            msg = f"{type(exc).__name__}: {exc}"
        self._last_error = msg
        urls = " | ".join(tried)
        if self.stats.errors == 1:
            print(f"[judge] ERROR (first): {msg}\n[judge] tried: {urls}",
                  file=sys.stderr, flush=True)
        elif self.stats.errors % 10 == 0:
            print(f"[judge] {self.stats.errors} errors so far; last: {msg}\n[judge] tried: {urls}",
                  file=sys.stderr, flush=True)

    def judge(self, question: str, gold: str, pred: str) -> bool | None:
        """Return True/False; on network/parse failure return None (the caller treats it as undecidable and skips the sample).

        Tries the OpenAI-style URL, and on HTTP 404 retries the alternate `/v1` mount, because
        self-hosted OpenAI-compatible servers differ on whether `/v1` is present in the path.
        """
        self.stats.calls += 1
        if self.stats.calls % 25 == 0:
            print(f"[judge] {self.stats.calls} calls, {self.stats.errors} errors",
                  file=sys.stderr, flush=True)
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
            "stream": False,
        }
        b = self.base_url
        if b.endswith("/v1"):
            urls = [f"{b}/chat/completions", f"{b[:-3]}/chat/completions"]
        else:
            urls = [f"{b}/chat/completions", f"{b}/v1/chat/completions"]

        max_retries = int(os.environ.get("BENCH_LLM_RETRIES", "2"))
        backoff = 2.0

        last_exc: Exception | None = None
        for url in urls:
            for attempt in range(max_retries + 1):
                # Escalate the timeout on retries so a slow endpoint (long prompts) gets more time.
                timeout_s = self.timeout_s * (2 ** min(attempt, 3))
                req = urllib.request.Request(
                    url,
                    data=json.dumps(body).encode("utf-8"),
                    headers={
                        "Content-Type": "application/json",
                        "Authorization": f"Bearer {self.api_key}",
                    },
                    method="POST",
                )
                try:
                    with urllib.request.urlopen(req, timeout=timeout_s) as resp:
                        data: Any = json.loads(resp.read().decode("utf-8"))
                    content = self._extract_content(data)
                    ok = self._parse(content)
                    if ok:
                        self.stats.yes += 1
                    return ok
                except HTTPError as e:
                    last_exc = e
                    if e.code == 404 and url != urls[-1]:
                        break  # try the alternate mount, don't retry a 404
                    if attempt < max_retries and e.code in _RETRY_HTTP:
                        time.sleep(backoff * (2 ** attempt))
                        continue
                    break
                except _TRANSIENT as e:
                    last_exc = e
                    if attempt < max_retries:
                        time.sleep(backoff * (2 ** attempt))
                        continue
                    break
                except Exception as e:  # noqa: BLE001
                    last_exc = e
                    break
            # After exhausting retries on this URL: a 404 falls through to the alternate mount.
            if isinstance(last_exc, HTTPError) and last_exc.code == 404 and url != urls[-1]:
                continue
            break
        self.stats.errors += 1
        self._record_error(last_exc or RuntimeError("unknown judge failure"), urls)
        return None

    @staticmethod
    def from_env(model_override: str | None = None, timeout_s: float | None = None) -> "Judge | None":
        """Return None when no credentials are present."""
        api_key = os.environ.get("BENCH_LLM_API_KEY") or os.environ.get("OPENAI_API_KEY")
        if not api_key:
            return None
        base_url = os.environ.get("BENCH_LLM_BASE_URL") or "https://api.openai.com/v1"
        model = model_override or os.environ.get("BENCH_LLM_MODEL") or "gpt-4o-mini"
        if timeout_s is None:
            timeout_s = float(os.environ.get("BENCH_LLM_TIMEOUT", "30"))
        return Judge(api_key=api_key, model=model, base_url=base_url, timeout_s=timeout_s)
