from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, Optional

from ..config import MetricSpec


@dataclass(frozen=True)
class JudgeRequest:
    metric: MetricSpec
    question: str
    answer: str
    context: str
    trace_meta: Optional[Dict[str, Any]] = (
        None  # optional future: episode_id, tool calls, etc.
    )


@dataclass(frozen=True)
class JudgeResponse:
    score: float  # normalized 0..1
    passed: bool  # judge's own pass (optional)
    rationale: str
    raw: Optional[Dict[str, Any]] = None  # original parsed JSON for debugging
