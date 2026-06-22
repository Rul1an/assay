#!/usr/bin/env python3
"""Reference canonical + content digest for the semantic-digest contract goldens. Pure stdlib.

This is a JCS-EQUIVALENT canonical for the string-only fixtures here:
`json.dumps(sort_keys=True, separators=(",", ":"))`. The product module uses serde_jcs / a full RFC 8785
implementation; for these string-only records the byte output is the same, and these goldens are what that
module must reproduce. Set normalization is applied ONLY to the registered set-paths, never globally
(RFC 8785 sorts object keys, not arrays).
"""

from __future__ import annotations

import hashlib
import json


def canonical(obj) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(",", ":")).encode("utf-8")


def content_id(obj) -> str:
    return "sha256:" + hashlib.sha256(canonical(obj)).hexdigest()


def canon_set(value):
    """A registered semantic-set value: a list of strings -> sorted, deduped. Anything else (not a list,
    or a non-string member) is malformed -> None (reject, never coerce)."""
    if not isinstance(value, list) or not all(isinstance(v, str) for v in value):
        return None
    return sorted(set(value))


def normalize_sets(record, set_paths):
    """Return a copy of `record` with each registered set-path (a list of keys) set-normalized, or None if
    a set-path value is malformed. Non-registered fields are left exactly as produced (order-significant)."""
    out = json.loads(json.dumps(record))  # deep copy
    for path in set_paths:
        node = out
        for key in path[:-1]:
            node = node.get(key) if isinstance(node, dict) else None
            if node is None:
                break
        if not isinstance(node, dict) or path[-1] not in node:
            continue  # path absent in this record -> nothing to normalize
        norm = canon_set(node[path[-1]])
        if norm is None:
            return None
        node[path[-1]] = norm
    return out
