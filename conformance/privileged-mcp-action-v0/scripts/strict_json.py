#!/usr/bin/env python3
"""One strict, bounded JSON-object loader for every hostile input in this corpus.

`conformance/adequacy/published_rows.py` already refuses symlinks, non-regular
files, oversized files, non-UTF-8 bytes, duplicate object names and non-finite
numbers. This module calls that loader rather than restating it, and adds the
two bounds it does not carry: nesting depth, and the JSON number domain.

Depth is scanned on the raw text *before* `json.loads` sees it. That ordering is
the point: CPython raises `RecursionError` out of the decoder on deeply nested
input, and `RecursionError` is not a `ValueError`, so a parse-then-check design
lets a traceback reach the user instead of a typed refusal.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "adequacy"))
import published_rows  # noqa: E402

MAX_JSON_DEPTH = 64
# The interoperable range every JSON implementation agrees on (RFC 7493 §2.2).
# Outside it, two readers of the same bytes can disagree about the value.
MAX_SAFE_INTEGER = 2**53 - 1


def scan_depth(text: str, limit: int, label: str) -> None:
    """Refuse nesting past `limit` without materializing the document."""
    depth = 0
    in_string = False
    escaped = False
    for character in text:
        if in_string:
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == '"':
                in_string = False
        elif character == '"':
            in_string = True
        elif character in "[{":
            depth += 1
            if depth > limit:
                raise ValueError(f"{label} nesting exceeds {limit}")
        elif character in "]}":
            depth -= 1


def reject_number_domain(value: Any, label: str) -> None:
    """Refuse floats outright and integers outside the interoperable range.

    Every number this corpus carries is an exit code, a byte count or a
    sequence: all exact integers. A float in that domain is either a precision
    loss that already happened or a value the producer never meant to send.
    """
    pending = [value]
    while pending:
        current = pending.pop()
        if isinstance(current, bool):
            continue
        if isinstance(current, float):
            raise ValueError(f"{label} carries a JSON number that is not an integer")
        if isinstance(current, int) and abs(current) > MAX_SAFE_INTEGER:
            raise ValueError(
                f"{label} carries an integer outside the interoperable range"
            )
        if isinstance(current, dict):
            pending.extend(current.values())
        elif isinstance(current, list):
            pending.extend(current)


def parse_strict_object(
    data: bytes,
    *,
    label: str,
    max_depth: int = MAX_JSON_DEPTH,
) -> dict:
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError(f"{label} is not UTF-8") from error
    scan_depth(text, max_depth, label)
    document = published_rows.parse_json_object(data, label)
    reject_number_domain(document, label)
    return document


def load_strict_object(
    path: Path,
    *,
    label: str,
    max_bytes: int,
    max_depth: int = MAX_JSON_DEPTH,
) -> dict:
    data = published_rows.read_regular_file(Path(path), limit=max_bytes)
    return parse_strict_object(data, label=label, max_depth=max_depth)
