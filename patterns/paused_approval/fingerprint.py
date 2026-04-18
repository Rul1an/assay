"""Derived continuation anchor helpers for paused approval state."""

from __future__ import annotations

import hashlib
import json
import math
from typing import Any


def _normalize_for_hash(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, bool)):
        return value
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("non-finite floats are not valid in canonical JSON")
        if value.is_integer():
            return int(value)
        raise ValueError("non-integer floats are not valid in this pattern's canonical JSON subset")
    if isinstance(value, dict):
        return {str(key): _normalize_for_hash(nested) for key, nested in value.items()}
    if isinstance(value, list):
        return [_normalize_for_hash(item) for item in value]
    if isinstance(value, tuple):
        return [_normalize_for_hash(item) for item in value]
    raise TypeError(f"unsupported canonical JSON value: {type(value).__name__}")


def _canonical_json(value: Any) -> str:
    normalized = _normalize_for_hash(value)
    return json.dumps(
        normalized,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
        allow_nan=False,
    )


def derive_resume_state_ref(serialized_state: Any) -> str:
    """Derive the canonical pause-only continuation anchor.

    This helper always returns a derived `resume_state_ref`. It does not expose
    raw serialized state as part of the pattern output.
    """

    if isinstance(serialized_state, bytes):
        payload = serialized_state
    elif isinstance(serialized_state, str):
        payload = serialized_state.encode("utf-8")
    else:
        payload = _canonical_json(serialized_state).encode("utf-8")
    digest = hashlib.sha256(payload).hexdigest()
    return f"runstate:sha256:{digest}"
