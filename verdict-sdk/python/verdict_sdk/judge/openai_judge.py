from __future__ import annotations

import json
import os
from dataclasses import dataclass
from typing import Any, Dict, Optional

from .client import JudgeClient
from .types import JudgeRequest, JudgeResponse

try:
    from openai import OpenAI
except ImportError:
    OpenAI = None  # type: ignore


@dataclass
class OpenAIJudge(JudgeClient):
    client: Any = None
    model: str = "gpt-4o"

    def __post_init__(self) -> None:
        if self.client is None:
            try:
                from openai import OpenAI
            except ImportError as e:
                raise ImportError(
                    "openai is not installed. Install verdict-sdk[openai]."
                ) from e



            self.client = OpenAI(api_key=os.getenv("OPENAI_API_KEY"))

    def evaluate(self, req: JudgeRequest) -> JudgeResponse:
        system = "You are a strict evaluation assistant. Respond ONLY with valid JSON."
        user = {
            "metric": req.metric.name,
            "question": req.question,
            "context": req.context,
            "answer": req.answer,
        }

        resp = self.client.chat.completions.create(
            model=self.model,
            messages=[
                {"role": "system", "content": system},
                {
                    "role": "user",
                    "content": json.dumps(user, ensure_ascii=False, sort_keys=True),
                },
            ],
            temperature=0.0,
            response_format={"type": "json_object"},
        )

        content = resp.choices[0].message.content or "{}"
        return self._parse_judge_json(content)

    def _parse_judge_json(self, text: str) -> JudgeResponse:
        import json

        from ..errors import JudgeParseError

        try:
            data = json.loads(text)
        except json.JSONDecodeError as e:
            raise JudgeParseError(
                f"Invalid JSON: {str(e)}", snippet=text[:200].strip(), raw=text
            ) from e

        if not isinstance(data, dict):
            raise JudgeParseError(
                "Response is not a JSON object", snippet=str(data)[:200], raw=text
            )

        if "score" not in data:
            raise JudgeParseError(
                "Missing 'score' field", snippet=text[:200].strip(), raw=text
            )

        try:
            score_val = data["score"]
            if score_val is None:
                raise ValueError("Score cannot be null")
            score = float(score_val)
        except (ValueError, TypeError) as e:
            raise JudgeParseError(
                f"Invalid score value: {data.get('score')}",
                snippet=text[:200].strip(),
                raw=text,
            ) from e

        score = max(0.0, min(1.0, score))

        if "passed" not in data:
            raise JudgeParseError(
                "Missing 'passed' field", snippet=text[:200].strip(), raw=text
            )

        passed_val = data["passed"]
        if not isinstance(passed_val, bool):
            raise JudgeParseError(
                "Field 'passed' must be boolean", snippet=text[:200].strip(), raw=text
            )
        passed = passed_val

        rationale_val = data.get("rationale") or data.get("reason")
        if rationale_val is None:
            raise JudgeParseError(
                "Missing 'rationale' field", snippet=text[:200].strip(), raw=text
            )

        rationale = str(rationale_val).strip()
        if not rationale:
            raise JudgeParseError(
                "'rationale' must be non-empty", snippet=text[:200].strip(), raw=text
            )

        return JudgeResponse(score=score, passed=passed, rationale=rationale, raw=data)
