from __future__ import annotations

import re
from typing import (Any, Callable, Dict, Iterable, List, Optional, Pattern,
                    Tuple)

_DEFAULT_KEY_DENYLIST = (
    "authorization",
    "api_key",
    "openai_api_key",
    "x-api-key",
    "cookie",
    "set-cookie",
)


def _compile(patterns: Iterable[str]) -> List[Pattern[str]]:
    return [re.compile(p) for p in patterns]


def make_redactor(
    *,
    patterns: Optional[List[str]] = None,
    key_denylist: Optional[Tuple[str, ...]] = None,
    replacement: str = "[REDACTED]",
) -> Callable[[Dict[str, Any]], Dict[str, Any]]:
    """
    Returns a redact_fn(event) -> redacted_event.
    - patterns: regexes applied to *all string values*
    - key_denylist: if a dict key matches (case-insensitive), its value becomes replacement
    """
    pats = _compile(patterns or [])
    deny = tuple(k.lower() for k in (key_denylist or _DEFAULT_KEY_DENYLIST))

    def redact_value(key: Optional[str], v: Any) -> Any:
        if key is not None and key.lower() in deny:
            return replacement

        if isinstance(v, str):
            out = v
            for p in pats:
                out = p.sub(replacement, out)
            return out

        if isinstance(v, list):
            return [redact_value(None, x) for x in v]

        if isinstance(v, dict):
            return {k: redact_value(k, vv) for k, vv in v.items()}

        return v

    def redact_event(event: Dict[str, Any]) -> Dict[str, Any]:
        # do not mutate caller
        return {k: redact_value(k, v) for k, v in event.items()}

    return redact_event
