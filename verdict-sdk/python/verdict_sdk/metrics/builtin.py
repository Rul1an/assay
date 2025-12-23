from __future__ import annotations

import re
from typing import Any, Dict, List, Optional

from ..config import MetricSpec
from ..errors import ConfigError
from ..result import MetricResult
from ..trace_reader import extract_last_model_content, extract_tool_calls


def eval_builtin(spec: MetricSpec, events: List[Dict[str, Any]]) -> MetricResult:
    """Dispatch builtin metric evaluation."""
    if spec.name == "regex_match":
        return _eval_regex_match(spec, events)
    elif spec.name == "trace_must_call_tool":
        return _eval_trace_must_call_tool(spec, events)
    elif spec.name == "trace_must_not_call_tool":
        return _eval_trace_must_not_call_tool(spec, events)
    else:
        raise ConfigError(f"Unknown builtin metric: {spec.name}")


def _eval_regex_match(spec: MetricSpec, events: List[Dict[str, Any]]) -> MetricResult:
    pattern = spec.params.get("pattern", "")
    content = extract_last_model_content(events)

    passed = False
    val = 0.0
    meta = {}

    if pattern and re.search(pattern, content, re.DOTALL):
        passed = True
        val = 1.0
    else:
        # Include snippet in meta for debugging
        snippet = content[:100] + "..." if len(content) > 100 else content
        meta["found"] = snippet
        meta["pattern"] = pattern

    return MetricResult(
        name=spec.name, value=val, passed=passed, threshold=spec.threshold, meta=meta
    )


def _eval_trace_must_call_tool(
    spec: MetricSpec, events: List[Dict[str, Any]]
) -> MetricResult:
    target_tool = spec.params.get("tool", "")
    calls = extract_tool_calls(events)

    count = sum(1 for c in calls if c.get("tool_name") == target_tool)

    passed = count > 0
    return MetricResult(
        name=spec.name,
        value=1.0 if passed else 0.0,
        passed=passed,
        threshold=spec.threshold,
        meta={"call_count": count},
    )


def _eval_trace_must_not_call_tool(
    spec: MetricSpec, events: List[Dict[str, Any]]
) -> MetricResult:
    target_tool = spec.params.get("tool", "")
    calls = extract_tool_calls(events)

    count = sum(1 for c in calls if c.get("tool_name") == target_tool)

    passed = count == 0
    return MetricResult(
        name=spec.name,
        value=1.0 if passed else 0.0,
        passed=passed,
        threshold=spec.threshold,
        meta={"call_count": count},
    )
