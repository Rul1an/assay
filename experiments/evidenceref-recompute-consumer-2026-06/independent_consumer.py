#!/usr/bin/env python3
"""Independent reproducer for the evidenceRef recomputation vectors.

Reads `vectors.json` ALONE and re-derives every expected verdict with separate code that does NOT
import `evidenceref_consumer.py`. It implements the two canonicalization profiles from their public
specs (RFC 8785 JCS and RFC 8949 sec 4.2 deterministic CBOR) and the same fail-closed resolution
logic, then checks that each recomputed verdict matches the one committed in the vector. Agreement
means the set reproduces from the bytes alone, with no shared runner and no producer trust: the
two-implementation interop bar, the bar a reference that only verifies itself cannot clear.

Usage: python3 independent_consumer.py [vectors.json]
"""
from __future__ import annotations

import hashlib
import json
import os
import sys

SUPPORTED_CANON = ("jcs-json-v1", "cbor-deterministic-v1")


def _confined_path(arg: str) -> str:
    """Resolve and validate a vectors path, confined to the working-directory tree. The reproducer reads
    a local JSON vectors file only, so the operator-supplied argument is rejected unless it is a relative,
    traversal-free path to an existing .json file inside the current working directory. The explicit
    absolute / parent-traversal rejection and the realpath prefix check together keep an untrusted
    argument from reaching the filesystem unchecked."""
    if not arg or not arg.strip():
        raise SystemExit("refusing an empty vectors path")
    normalized = arg.replace("\\", "/")
    if os.path.isabs(arg) or os.path.isabs(normalized):
        raise SystemExit(f"refusing an absolute vectors path: {arg!r}")
    parts = [p for p in normalized.split("/") if p not in ("", ".")]
    if any(p == ".." for p in parts):
        raise SystemExit(f"refusing a vectors path with parent traversal: {arg!r}")
    base = os.path.realpath(os.getcwd())
    resolved = os.path.realpath(os.path.join(base, *parts))
    if resolved != base and not resolved.startswith(base + os.sep):
        raise SystemExit(f"refusing a vectors path outside the working directory: {arg!r}")
    if not resolved.endswith(".json"):
        raise SystemExit(f"refusing a non-json vectors path: {arg!r}")
    if not os.path.isfile(resolved):
        raise SystemExit(f"refusing a non-file vectors path: {arg!r}")
    return resolved


def jcs(obj) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def _head(major: int, n: int) -> bytes:
    if n < 24:
        return bytes([(major << 5) | n])
    if n < 0x100:
        return bytes([(major << 5) | 24, n])
    if n < 0x10000:
        return bytes([(major << 5) | 25]) + n.to_bytes(2, "big")
    if n < 0x100000000:
        return bytes([(major << 5) | 26]) + n.to_bytes(4, "big")
    return bytes([(major << 5) | 27]) + n.to_bytes(8, "big")


def cbor(obj) -> bytes:
    if obj is True:
        return b"\xf5"
    if obj is False:
        return b"\xf4"
    if obj is None:
        return b"\xf6"
    if isinstance(obj, int):
        return _head(0, obj) if obj >= 0 else _head(1, -1 - obj)
    if isinstance(obj, str):
        b = obj.encode("utf-8")
        return _head(3, len(b)) + b
    if isinstance(obj, list):
        return _head(4, len(obj)) + b"".join(cbor(x) for x in obj)
    if isinstance(obj, dict):
        items = sorted(((cbor(str(k)), cbor(v)) for k, v in obj.items()), key=lambda kv: kv[0])
        return _head(5, len(items)) + b"".join(k + v for k, v in items)
    raise TypeError(f"unencodable: {type(obj).__name__}")


def address(obj, canon: str):
    if canon == "jcs-json-v1":
        return "sha256:" + hashlib.sha256(jcs(obj)).hexdigest()
    if canon == "cbor-deterministic-v1":
        return "sha256:" + hashlib.sha256(cbor(obj)).hexdigest()
    return None


def is_redacted(value) -> bool:
    return value is None or value == "<redacted>" or (isinstance(value, dict) and value.get("_redacted") is True)


def reproduce(ref: dict, body_store: dict, registry: dict) -> dict:
    if not ref.get("digest") or not ref.get("canonicalization"):
        return {"verdict": "malformed_ref", "reason": "missing_required_field_digest_or_canonicalization"}
    canon = ref["canonicalization"]
    locator = ref.get("ref")
    if locator is None:
        return {"verdict": "unresolvable_digest_only", "reason": "digest_alone_insufficient_no_resolvable_body"}
    if locator not in body_store:
        return {"verdict": "unresolved_ref", "reason": "ref_present_but_body_not_resolvable"}
    body = body_store[locator]
    if not isinstance(canon, str) or canon not in SUPPORTED_CANON:
        return {"verdict": "unsupported_canonicalization", "reason": "declared_profile_not_in_consumer_profile_registry"}
    if address(body, canon) != ref["digest"]:
        for alt in SUPPORTED_CANON:
            if alt != canon and address(body, alt) == ref["digest"]:
                return {"verdict": "canonicalization_mismatch", "reason": f"digest_matches_{alt}_not_declared_{canon}"}
        return {"verdict": "digest_mismatch", "reason": "recompute_under_declared_canon_diverges_from_committed_digest"}
    body_schema = f"{body.get('schema')}/{body.get('schema_version')}"
    if body_schema != f"{ref.get('schema')}/{ref.get('schema_version')}":
        return {"verdict": "schema_mismatch", "reason": "body_schema_differs_from_declared"}
    spec = registry.get(body_schema)
    if spec is None:
        return {"verdict": "schema_mismatch", "reason": f"unknown_schema_{body_schema}"}
    incomplete = [f for f in spec["required_fields"] if f not in body or is_redacted(body[f])]
    if incomplete:
        return {"verdict": "redacted_projection_incomplete", "reason": "missing_or_redacted_required_evidence:" + ",".join(incomplete)}
    return {"verdict": "recomputed", "reason": "bytes_match_address_under_declared_canon_and_schema_complete"}


def main() -> int:
    path = sys.argv[1] if len(sys.argv) > 1 else "vectors.json"
    with open(_confined_path(path), encoding="utf-8") as fh:
        doc = json.load(fh)
    registry = doc.get("schema_registry", {})
    failures = [
        f"{c['id']}: got {reproduce(c['ref'], c['body_store'], registry)}, expected {c['expected']}"
        for c in doc["cases"]
        if reproduce(c["ref"], c["body_store"], registry) != c["expected"]
    ]
    print(json.dumps({
        "reproducer": "independent",
        "cases": len(doc["cases"]),
        "all_reproduced": not failures,
        "failures": failures,
    }, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
