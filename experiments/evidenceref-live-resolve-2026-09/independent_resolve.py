#!/usr/bin/env python3
"""Independent reproducer for the live evidenceRef resolution run.

Reads the pinned octets and `runs/resolve-run.json` and re-derives every verdict with separate code that
imports neither `resolve.py` nor the published consumer. It re-implements the resolution logic from
the public specs (RFC 8785 JCS for the object profile, plain SHA-256 over the received octets for the
proposed profile) and rebuilds each case body from the pinned record itself, then checks that each
recomputed verdict matches the one committed in the run.

Agreement means the run reproduces from the bytes alone rather than from one runner trusting another.

Usage: python3 independent_resolve.py
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).parent
OCTETS_PROFILE = "octets-as-served-v1"
OBJECT_PROFILE = "jcs-json-v1"

REQUIRED = (
    "endpoint",
    "checked_at",
    "status",
    "reachable",
    "checks",
    "absence_vs_failure",
    "establishes",
    "does_not_establish",
)


def jcs_bytes(obj) -> bytes:
    """RFC 8785 over the value space of this record, which is ASCII and float-free. The scope is
    checked rather than assumed: `assert_value_space` below refuses anything outside it, so this
    serialization is never quietly applied where naive key sorting and true JCS would diverge."""
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def assert_value_space(obj, path="") -> None:
    """Refuse the float and non-BMP cases where naive sorting is not RFC 8785."""
    if isinstance(obj, dict):
        for k, v in obj.items():
            if any(ord(c) > 0xFFFF for c in k):
                raise SystemExit(f"non-BMP key at {path}.{k}: naive sorting is not RFC 8785 here")
            assert_value_space(v, f"{path}.{k}")
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            assert_value_space(v, f"{path}[{i}]")
    elif isinstance(obj, float):
        raise SystemExit(f"float at {path}: naive serialization is not RFC 8785 here")


def address(b: bytes) -> str:
    return "sha256:" + hashlib.sha256(b).hexdigest()


def resolve(ref: dict, octets: bytes | None, obj_body: dict | None, supported: tuple) -> str:
    if not ref.get("digest") or not ref.get("canonicalization"):
        return "malformed_ref"
    canon = ref["canonicalization"]
    if ref.get("ref") is None:
        return "unresolvable_digest_only"
    if octets is None and obj_body is None:
        return "unresolved_ref"
    if canon not in supported:
        return "unsupported_canonicalization"

    if canon == OCTETS_PROFILE:
        if address(octets) != ref["digest"]:
            return "digest_mismatch"
        try:
            body = json.loads(octets)
        except ValueError:
            return "malformed_body"
    else:
        body = obj_body
        assert_value_space(body)
        if address(jcs_bytes(body)) != ref["digest"]:
            return "digest_mismatch"
        if f"{body.get('schema')}/{body.get('schema_version')}" != f"{ref.get('schema')}/{ref.get('schema_version')}":
            return "schema_mismatch"

    if f"{ref.get('schema')}/{ref.get('schema_version')}" != "mcp-verification-gate/0.4.1":
        return "schema_mismatch"
    missing = [f for f in REQUIRED if f not in body]
    if missing:
        return "redacted_projection_incomplete"
    return "recomputed"


def main() -> int:
    octets = (HERE / "record" / "gate-record-4dffe218.json").read_bytes()
    body = json.loads(octets)
    assert_value_space(body)
    committed = json.loads((HERE / "runs" / "resolve-run.json").read_text())

    published_addr = committed["subject"]["published_address"]
    if address(octets) != published_addr:
        print(f"pinned octets do not match the published address {published_addr}")
        return 1

    base = {"digest": published_addr, "ref": "gate-record"}
    schema = {"schema": "mcp-verification-gate", "schema_version": "0.4.1"}
    reser = jcs_bytes(body)
    tampered = octets.replace(b'"gate_version":"0.4.1"', b'"gate_version":"0.4.2"', 1)
    trimmed = {k: v for k, v in body.items() if k != "absence_vs_failure"}
    trimmed_octets = json.dumps(trimmed, separators=(",", ":"), ensure_ascii=False).encode()

    # The published consumer supports the two object profiles; the proposed profile is separate.
    obj_supported = ("jcs-json-v1", "cbor-deterministic-v1")
    oct_supported = (OCTETS_PROFILE,)

    derived = {
        "A_as_published_no_canonicalization": resolve(dict(base), None, body, obj_supported),
        "B_charitable_jcs": resolve({**base, **schema, "canonicalization": OBJECT_PROFILE}, None, body, obj_supported),
        "C_octets_profile_named": resolve({**base, **schema, "canonicalization": OCTETS_PROFILE}, octets, None, oct_supported),
        "D_octets_reserialized_body": resolve({**base, **schema, "canonicalization": OCTETS_PROFILE}, reser, None, oct_supported),
        "E_octets_tampered": resolve({**base, **schema, "canonicalization": OCTETS_PROFILE}, tampered, None, oct_supported),
        "F_octets_incomplete_projection": resolve(
            {**base, **schema, "canonicalization": OCTETS_PROFILE, "digest": address(trimmed_octets)},
            trimmed_octets, None, oct_supported),
        "G_unknown_profile_name": resolve({**base, **schema, "canonicalization": "gate-internal-v9"}, octets, None, oct_supported),
        "H_octets_no_canonicalization": resolve(dict(base), octets, None, oct_supported),
        "I_octets_digest_only": resolve({"digest": published_addr, **schema, "canonicalization": OCTETS_PROFILE}, octets, None, oct_supported),
        "J_octets_locator_unresolvable": resolve({**base, **schema, "canonicalization": OCTETS_PROFILE, "ref": "not-in-store"}, None, None, oct_supported),
        "K_octets_unknown_schema_identity": resolve(
            {**base, "canonicalization": OCTETS_PROFILE, "schema": "whatever-the-producer-says", "schema_version": "1"},
            octets, None, oct_supported),
    }

    committed_ids = [c["id"] for c in committed["cases"]]
    if len(set(committed_ids)) != len(committed_ids):
        dupes = sorted({i for i in committed_ids if committed_ids.count(i) > 1})
        print(f"duplicate case ids in the run record: {dupes}")
        return 1
    # Exact set equality, not containment. Without it a case dropped from the run record, or one
    # this reproducer never derives, still prints a clean n-of-n total over a smaller set than the
    # runner measured, and the total reads as agreement about cases nobody compared.
    if set(derived) != set(committed_ids):
        only_run = sorted(set(committed_ids) - set(derived))
        only_here = sorted(set(derived) - set(committed_ids))
        print(f"case sets differ. only in the run record: {only_run}; only derived here: {only_here}")
        return 1

    disagreements = []
    for case in committed["cases"]:
        mine = derived.get(case["id"])
        if mine != case["verdict"]:
            disagreements.append((case["id"], case["verdict"], mine))
        print(f"{'ok ' if mine == case['verdict'] else 'BAD'} {case['id']:36s} runner={case['verdict']:32s} independent={mine}")

    if disagreements:
        print(f"\n{len(disagreements)} disagreement(s): {disagreements}")
        return 1
    n = len(committed["cases"])
    print(f"\nindependent reproduction agrees on {n} of {n} cases")
    return 0


if __name__ == "__main__":
    sys.exit(main())
