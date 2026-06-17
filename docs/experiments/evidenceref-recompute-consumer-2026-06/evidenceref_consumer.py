#!/usr/bin/env python3
"""E43 - independent evidenceRef recomputation consumer (reference runner + emitter).

Resolves a content-addressed `evidenceRef` and re-derives one fail-closed verdict FROM COMMITTED
BYTES, the recomputation layer beneath claim grounding. The consumer never trusts the producer: a `recomputed`
(clean) verdict is reached only by resolving the referenced body, recomputing its digest under the
DECLARED canonicalization profile, validating the declared schema, and confirming the body is a
complete evidence record. Anything short of that is non-clean - either a positive disagreement
(`digest_mismatch` / `canonicalization_mismatch` / `schema_mismatch` / `malformed_ref`) or an
inconclusive state (`unresolvable_digest_only` / `unresolved_ref` / `unsupported_canonicalization` /
`redacted_projection_incomplete`). The producer's own `producer_state` ("clean") is NEVER an input.

Two canonicalization profiles are implemented, so the reference is a neutral join point and not one
producer's envelope in disguise: `jcs-json-v1` (RFC 8785 JCS over the JSON object) and
`cbor-deterministic-v1` (RFC 8949 sec 4.2 core deterministic encoding). The same logical object
yields a different digest under each, and each ref resolves only under its own declared profile.

Hard guardrails (the point of this slice, not just interop):
  - clean ONLY on independent recomputation; `producer_state` is never consulted.
  - a redacted or silently-elided projection cannot launder missing evidence into clean: completeness
    is schema-driven (required evidence fields present AND not redaction placeholders), so both a
    marked redaction and a silent strip land on `redacted_projection_incomplete`.
  - digest-only / unresolved / unsupported-canon are inconclusive and fail closed, never clean.

Usage:
  python3 evidenceref_consumer.py emit   > vectors.json     # regenerate the vector bytes
  python3 evidenceref_consumer.py verify vectors.json        # reproduce every verdict + measurement
  python3 evidenceref_consumer.py                            # run the in-memory corpus + measurement
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import sys

# --------------------------------------------------------------------------------------------------
# Canonicalization profiles (public spec; any conformant implementation reproduces these bytes).
# --------------------------------------------------------------------------------------------------

SUPPORTED_CANON = ("jcs-json-v1", "cbor-deterministic-v1")

CANON_PROFILES = {
    "jcs-json-v1": {
        "description": "RFC 8785 (JCS) over the JSON object, float-free value space in these vectors",
        "hash": "sha256",
        "digest_encoding": "hex",
        "digest_prefix": "sha256:",
    },
    "cbor-deterministic-v1": {
        "description": "RFC 8949 section 4.2 core deterministic encoding (definite lengths, shortest "
        "ints, map keys sorted by encoded-key bytes), value space limited to these vectors",
        "hash": "sha256",
        "digest_encoding": "hex",
        "digest_prefix": "sha256:",
    },
}


def _jcs(obj) -> bytes:
    # RFC 8785-equivalent for the float-free value space used here (matches assay's pinned serde_jcs
    # on this value space, cross-implementation checked): sorted keys, no insignificant whitespace, UTF-8.
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def _cbor_head(major: int, n: int) -> bytes:
    if n < 24:
        return bytes([(major << 5) | n])
    if n < 0x100:
        return bytes([(major << 5) | 24, n])
    if n < 0x10000:
        return bytes([(major << 5) | 25]) + n.to_bytes(2, "big")
    if n < 0x100000000:
        return bytes([(major << 5) | 26]) + n.to_bytes(4, "big")
    return bytes([(major << 5) | 27]) + n.to_bytes(8, "big")


def _cbor(obj) -> bytes:
    # RFC 8949 sec 4.2 core deterministic encoding. bool is checked before int (bool subclasses int).
    if obj is True:
        return b"\xf5"
    if obj is False:
        return b"\xf4"
    if obj is None:
        return b"\xf6"
    if isinstance(obj, int):
        return _cbor_head(0, obj) if obj >= 0 else _cbor_head(1, -1 - obj)
    if isinstance(obj, str):
        b = obj.encode("utf-8")
        return _cbor_head(3, len(b)) + b
    if isinstance(obj, list):
        return _cbor_head(4, len(obj)) + b"".join(_cbor(x) for x in obj)
    if isinstance(obj, dict):
        items = sorted(((_cbor(str(k)), _cbor(v)) for k, v in obj.items()), key=lambda kv: kv[0])
        return _cbor_head(5, len(items)) + b"".join(k + v for k, v in items)
    raise TypeError(f"cbor-deterministic-v1 does not encode {type(obj).__name__}")


def canonical_bytes(obj, canon: str) -> bytes | None:
    if canon == "jcs-json-v1":
        return _jcs(obj)
    if canon == "cbor-deterministic-v1":
        return _cbor(obj)
    return None  # unsupported: never assume a default profile


def content_address(obj, canon: str) -> str | None:
    cb = canonical_bytes(obj, canon)
    return None if cb is None else "sha256:" + hashlib.sha256(cb).hexdigest()


# --------------------------------------------------------------------------------------------------
# Schema registry: each schema names the evidence fields required for a record to be COMPLETE. A
# present-but-redacted placeholder counts as missing, which is what defeats projection laundering.
# --------------------------------------------------------------------------------------------------

SCHEMA_REGISTRY = {
    "assay.policy_decision/v1": {
        "required_fields": ["schema", "schema_version", "decision", "effect", "target"],
    },
    "assay.sequence/v1": {
        "required_fields": ["schema", "schema_version", "sequence", "terminal"],
    },
}


def _is_redacted(value) -> bool:
    """A field present only as a redaction placeholder is not usable evidence."""
    if value is None:
        return True
    if value == "<redacted>":
        return True
    if isinstance(value, dict) and value.get("_redacted") is True:
        return True
    return False


# --------------------------------------------------------------------------------------------------
# The consumer: one fail-closed verdict per evidenceRef, recomputed from bytes, no producer trust.
# --------------------------------------------------------------------------------------------------


def _v(verdict: str, reason: str) -> dict:
    return {"verdict": verdict, "reason": reason}


def consume(ref: dict, body_store: dict, schema_registry: dict = SCHEMA_REGISTRY) -> dict:
    """Resolve and recompute one evidenceRef. `producer_state` on the ref is deliberately ignored."""
    # 1. A ref missing its content address or its canonicalization profile cannot be recomputed.
    if not ref.get("digest") or not ref.get("canonicalization"):
        return _v("malformed_ref", "missing_required_field_digest_or_canonicalization")

    canon = ref["canonicalization"]
    locator = ref.get("ref")  # None => digest-only form (no body to recompute from)

    # 2. Resolve the referenced bytes. A committed digest alone is not sufficient.
    if locator is None:
        return _v("unresolvable_digest_only", "digest_alone_insufficient_no_resolvable_body")
    if locator not in body_store:
        return _v("unresolved_ref", "ref_present_but_body_not_resolvable")
    body = body_store[locator]

    # 3. The declared canonicalization is a NAME resolved by this consumer's profile registry; the
    #    producer may name a profile but may not define what it means. A non-string definition (a
    #    producer trying to embed its own rules) or an unknown name is refused, never assumed.
    if not isinstance(canon, str) or canon not in SUPPORTED_CANON:
        return _v("unsupported_canonicalization", "declared_profile_not_in_consumer_profile_registry")

    # 4. Recompute the digest under the DECLARED profile and compare to the committed value.
    recomputed = content_address(body, canon)
    if recomputed != ref["digest"]:
        # Distinguish a wrong-profile declaration from a tampered object: does the committed digest
        # match the body under a different supported profile? If so the bytes are intact but the ref
        # named the wrong canonicalization.
        for alt in SUPPORTED_CANON:
            if alt != canon and content_address(body, alt) == ref["digest"]:
                return _v("canonicalization_mismatch", f"digest_matches_{alt}_not_declared_{canon}")
        return _v("digest_mismatch", "recompute_under_declared_canon_diverges_from_committed_digest")

    # 5. The body must carry the declared schema. Identity is bound into the digest, so this also
    #    fails closed if the schema fields were tampered.
    body_schema = f"{body.get('schema')}/{body.get('schema_version')}"
    if body_schema != f"{ref.get('schema')}/{ref.get('schema_version')}":
        return _v("schema_mismatch", "body_schema_differs_from_declared")
    # The completeness rules come ONLY from this consumer's trusted registry, keyed by the schema
    # identity. Producer-supplied completeness metadata - a `producer_schema_hint` on the ref or a
    # body-local `_schema` override - is never read, so a producer cannot declare a redacted field
    # non-required. Schema authority is consumer-controlled, not producer-controlled.
    spec = schema_registry.get(body_schema)
    if spec is None:
        return _v("schema_mismatch", f"unknown_schema_{body_schema}")

    # 6. Completeness: a projection that elides or redacts a required evidence field is inconclusive,
    #    even though its own digest recomputes. The projection existing does not make the claim clean.
    incomplete = [f for f in spec["required_fields"] if f not in body or _is_redacted(body[f])]
    if incomplete:
        return _v("redacted_projection_incomplete", "missing_or_redacted_required_evidence:" + ",".join(incomplete))

    # 7. Independent clean: the bytes match the content address under the declared canonicalization,
    #    the schema holds, the record is complete. This is NOT a trust verdict on the producer.
    return _v("recomputed", "bytes_match_address_under_declared_canon_and_schema_complete")


# --------------------------------------------------------------------------------------------------
# Corpus: the full happy + negative matrix. Digests are computed here, so every vector is recomputable.
# --------------------------------------------------------------------------------------------------

PD = ("assay.policy_decision", "v1")


def _obj(schema, **fields) -> dict:
    o = {"schema": schema[0], "schema_version": schema[1]}
    o.update(fields)
    return o


def _ref(digest, locator, canon, schema, **extra) -> dict:
    r = {
        "type": "policy_decision",
        "digest": digest,
        "ref": locator,
        "canonicalization": canon,
        "schema": schema[0],
        "schema_version": schema[1],
    }
    r.update(extra)
    return r


def build_corpus() -> list[dict]:
    """Each case is self-contained: an evidenceRef, its own body_store, and a kind tag for invariants."""
    cases: list[dict] = []

    def add(cid, kind, ref, body_store):
        cases.append({"id": cid, "kind": kind, "ref": ref, "body_store": body_store})

    # c1 - happy path, JCS profile.
    o1 = _obj(PD, decision="allow", effect={"wrote": "x"}, target={"path": "/etc/x"})
    add("c1_happy_jcs", "happy",
        _ref(content_address(o1, "jcs-json-v1"), "loc1", "jcs-json-v1", PD), {"loc1": o1})

    # c2 - happy path, deterministic-CBOR profile (envelope-neutral: same shape, different profile).
    o2 = _obj(PD, decision="deny", effect={"blocked": "connect"}, target={"host": "10.0.0.1"})
    add("c2_happy_cbor", "happy",
        _ref(content_address(o2, "cbor-deterministic-v1"), "loc2", "cbor-deterministic-v1", PD), {"loc2": o2})

    # c3 - digest-only, no resolvable body. Inconclusive, never clean.
    o3 = _obj(PD, decision="allow", effect={"wrote": "y"}, target={"path": "/y"})
    add("c3_digest_only_no_body", "inconclusive",
        _ref(content_address(o3, "jcs-json-v1"), None, "jcs-json-v1", PD), {})

    # c4 - ref present but body not resolvable. Fail closed.
    o4 = _obj(PD, decision="allow", effect={"wrote": "z"}, target={"path": "/z"})
    add("c4_unresolved_ref", "inconclusive",
        _ref(content_address(o4, "jcs-json-v1"), "missing", "jcs-json-v1", PD), {"other": o4})

    # c5 - body resolves but is a different object than the committed digest (tamper). Positive fail.
    o5a = _obj(PD, decision="allow", effect={"wrote": "a"}, target={"path": "/a"})
    o5b = _obj(PD, decision="allow", effect={"wrote": "b"}, target={"path": "/b"})
    add("c5_digest_mismatch", "fail",
        _ref(content_address(o5a, "jcs-json-v1"), "loc5", "jcs-json-v1", PD), {"loc5": o5b})

    # c6 - bytes intact but the ref names the wrong profile: digest is the CBOR digest, canon says JCS.
    o6 = _obj(PD, decision="deny", effect={"blocked": "write"}, target={"path": "/c"})
    add("c6_canon_mismatch", "fail",
        _ref(content_address(o6, "cbor-deterministic-v1"), "loc6", "jcs-json-v1", PD), {"loc6": o6})

    # c7 - canonicalization profile this consumer does not implement. Inconclusive, never assumed.
    o7 = _obj(PD, decision="allow", effect={"wrote": "d"}, target={"path": "/d"})
    add("c7_unsupported_canon", "inconclusive",
        _ref("sha256:" + "0" * 64, "loc7", "protobuf-canonical-v1", PD), {"loc7": o7})

    # c8 - body recomputes but carries a different schema than declared. Positive fail.
    o8 = _obj(("assay.other", "v1"), decision="allow", effect={"wrote": "e"}, target={"path": "/e"})
    add("c8_schema_mismatch", "fail",
        _ref(content_address(o8, "jcs-json-v1"), "loc8", "jcs-json-v1", PD), {"loc8": o8})

    # c9 - redacted projection: a required evidence field present only as a redaction placeholder.
    o9 = _obj(PD, decision="allow", effect={"_redacted": True}, target={"path": "/f"}, redacted_fields=["effect"])
    add("c9_redacted_projection_marked", "redaction",
        _ref(content_address(o9, "jcs-json-v1"), "loc9", "jcs-json-v1", PD), {"loc9": o9})

    # c10 - silent elision: the required evidence field is simply absent, with no redaction marker.
    o10 = _obj(PD, decision="allow", target={"path": "/g"})  # no "effect" key at all
    add("c10_silent_elision", "redaction",
        _ref(content_address(o10, "jcs-json-v1"), "loc10", "jcs-json-v1", PD), {"loc10": o10})

    # c11 - producer self-declares clean, digest-only, no body. The flag must not promote it.
    o11 = _obj(PD, decision="allow", effect={"wrote": "h"}, target={"path": "/h"})
    add("c11_producer_clean_no_body", "producer_self_clean",
        _ref(content_address(o11, "jcs-json-v1"), None, "jcs-json-v1", PD, producer_state="clean"), {})

    # c12 - producer self-declares clean over a tampered body. The flag must not launder the mismatch.
    o12a = _obj(PD, decision="allow", effect={"wrote": "i"}, target={"path": "/i"})
    o12b = _obj(PD, decision="allow", effect={"wrote": "j"}, target={"path": "/j"})
    add("c12_producer_clean_mismatch", "producer_self_clean",
        _ref(content_address(o12a, "jcs-json-v1"), "loc12", "jcs-json-v1", PD, producer_state="clean"),
        {"loc12": o12b})

    # c13 - malformed ref: no content address.
    add("c13_malformed_no_digest", "fail",
        {"type": "policy_decision", "ref": "loc13", "canonicalization": "jcs-json-v1",
         "schema": PD[0], "schema_version": PD[1]}, {"loc13": _obj(PD, decision="allow", effect={}, target={})})

    # c14 - malformed ref: no canonicalization profile.
    o14 = _obj(PD, decision="allow", effect={"wrote": "k"}, target={"path": "/k"})
    add("c14_malformed_no_canon", "fail",
        {"type": "policy_decision", "digest": content_address(o14, "jcs-json-v1"), "ref": "loc14",
         "schema": PD[0], "schema_version": PD[1]}, {"loc14": o14})

    # c15 - schema authority: the ref carries a producer `producer_schema_hint` that would make the
    #       redacted field non-required. The consumer ignores it; the redaction still fails closed.
    o15 = _obj(PD, decision="allow", effect={"_redacted": True}, target={"path": "/l"})
    add("c15_producer_schema_hint", "schema_authority",
        _ref(content_address(o15, "jcs-json-v1"), "loc15", "jcs-json-v1", PD,
             producer_schema_hint={"required_fields": []}), {"loc15": o15})

    # c16 - schema authority: a body-local `_schema` override claims no required fields. The consumer
    #       uses its own registry, never the body's self-description; the redaction still fails closed.
    o16 = _obj(PD, decision="allow", effect={"_redacted": True}, target={"path": "/m"})
    o16["_schema"] = {"required_fields": []}
    add("c16_body_local_schema_override", "schema_authority",
        _ref(content_address(o16, "jcs-json-v1"), "loc16", "jcs-json-v1", PD), {"loc16": o16})

    # c17 - profile authority: the ref tries to embed its own canonicalization definition instead of
    #       naming a profile. The consumer resolves profiles by name from its registry and refuses a
    #       producer-provided definition.
    o17 = _obj(PD, decision="allow", effect={"wrote": "n"}, target={"path": "/n"})
    add("c17_producer_defined_canon", "profile_authority",
        _ref("sha256:" + "0" * 64, "loc17",
             {"name": "jcs-json-v1", "rules": "producer-defined-do-not-honor"}, PD), {"loc17": o17})

    # Attach the expected verdict from the reference consumer, so the independent runner reproduces it.
    for case in cases:
        case["expected"] = consume(case["ref"], case["body_store"])
    return cases


def emit() -> dict:
    return {
        "schema": "assay.experiment.e43_evidenceref_recompute_consumer.v0",
        "canonicalization_profiles": CANON_PROFILES,
        "schema_registry": SCHEMA_REGISTRY,
        "verdicts": [
            "recomputed", "digest_mismatch", "canonicalization_mismatch", "schema_mismatch",
            "malformed_ref", "unresolvable_digest_only", "unresolved_ref",
            "unsupported_canonicalization", "redacted_projection_incomplete",
        ],
        "cases": build_corpus(),
    }


# --------------------------------------------------------------------------------------------------
# Measurement: reproduce every verdict and assert the load-bearing invariants.
# --------------------------------------------------------------------------------------------------


def measure(doc: dict) -> dict:
    cases = doc["cases"]
    registry = doc.get("schema_registry", SCHEMA_REGISTRY)
    counts: dict[str, int] = {}
    failures = []
    by_id = {}
    for case in cases:
        got = consume(case["ref"], case["body_store"], registry)
        by_id[case["id"]] = got
        counts[got["verdict"]] = counts.get(got["verdict"], 0) + 1
        if got != case["expected"]:
            failures.append(f"{case['id']}: got {got}, expected {case['expected']}")

    clean = {cid for cid, v in by_id.items() if v["verdict"] == "recomputed"}

    # Invariant 1: producer_state is never consulted - flip it on a happy ref and on a tamper ref;
    # the verdict must not move.
    happy = next(c for c in cases if c["id"] == "c1_happy_jcs")
    tamper = next(c for c in cases if c["id"] == "c5_digest_mismatch")
    happy_dirty = consume({**happy["ref"], "producer_state": "dirty"}, happy["body_store"], registry)
    tamper_clean = consume({**tamper["ref"], "producer_state": "clean"}, tamper["body_store"], registry)
    producer_state_never_consulted = (
        happy_dirty == by_id["c1_happy_jcs"] and tamper_clean == by_id["c5_digest_mismatch"]
    )

    o = {"schema": "assay.policy_decision", "schema_version": "v1", "decision": "x",
         "effect": {"e": 1}, "target": {"t": 1}}
    envelope_distinct = content_address(o, "jcs-json-v1") != content_address(o, "cbor-deterministic-v1")

    # Schema authority: a producer hint cannot turn a redacted projection clean, and cannot move a
    # clean verdict; the completeness rules come only from the consumer registry.
    redacted = next(c for c in cases if c["id"] == "c9_redacted_projection_marked")
    redacted_hinted = consume(
        {**redacted["ref"], "producer_schema_hint": {"required_fields": []}}, redacted["body_store"], registry)
    happy_hinted = consume(
        {**happy["ref"], "producer_schema_hint": {"required_fields": []}}, happy["body_store"], registry)
    schema_authority_is_consumer_controlled = (
        by_id["c15_producer_schema_hint"]["verdict"] == "redacted_projection_incomplete"
        and by_id["c16_body_local_schema_override"]["verdict"] == "redacted_projection_incomplete"
        and redacted_hinted == by_id["c9_redacted_projection_marked"]
        and happy_hinted == by_id["c1_happy_jcs"]
    )

    # Profile authority: a producer may name a profile but not define it; an embedded definition is
    # refused, and a stray producer `canonicalization_rules` sibling on a valid ref is never read.
    happy_ruled = consume(
        {**happy["ref"], "canonicalization_rules": "producer-defined-bogus"}, happy["body_store"], registry)
    canonicalization_profile_authority_is_consumer_controlled = (
        by_id["c17_producer_defined_canon"]["verdict"] == "unsupported_canonicalization"
        and happy_ruled == by_id["c1_happy_jcs"]
    )

    invariants = {
        # clean is reached ONLY for the two happy paths, both by independent recomputation.
        "clean_only_on_independent_recomputation": clean == {"c1_happy_jcs", "c2_happy_cbor"},
        # a marked redaction AND a silent elision both fail to launder into clean.
        "redacted_projection_cannot_launder": (
            by_id["c9_redacted_projection_marked"]["verdict"] == "redacted_projection_incomplete"
            and by_id["c10_silent_elision"]["verdict"] == "redacted_projection_incomplete"
        ),
        # a producer self-declared clean never promotes a missing or tampered body.
        "producer_self_clean_never_promotes": (
            by_id["c11_producer_clean_no_body"]["verdict"] == "unresolvable_digest_only"
            and by_id["c12_producer_clean_mismatch"]["verdict"] == "digest_mismatch"
        ),
        "producer_state_never_consulted": producer_state_never_consulted,
        # digest alone, unresolved refs, and unsupported profiles all fail closed.
        "digest_only_is_inconclusive": by_id["c3_digest_only_no_body"]["verdict"] == "unresolvable_digest_only",
        "unresolved_ref_fails_closed": by_id["c4_unresolved_ref"]["verdict"] == "unresolved_ref",
        "canonicalization_explicit": (
            by_id["c6_canon_mismatch"]["verdict"] == "canonicalization_mismatch"
            and by_id["c7_unsupported_canon"]["verdict"] == "unsupported_canonicalization"
            and envelope_distinct
        ),
        # completeness rules are the consumer's, never the producer's: hints and body-local overrides
        # cannot launder a redacted projection into clean.
        "schema_authority_is_consumer_controlled": schema_authority_is_consumer_controlled,
        # profile meaning is the consumer's: a producer names a profile, it does not define one.
        "canonicalization_profile_authority_is_consumer_controlled": canonicalization_profile_authority_is_consumer_controlled,
        # headline: the producer envelope is not the verdict authority. Its state flag, schema hints,
        # and profile definitions never decide the verdict, and both canonicalization profiles reach
        # clean through the one consumer code path.
        "producer_envelope_not_authoritative": (
            producer_state_never_consulted
            and schema_authority_is_consumer_controlled
            and canonicalization_profile_authority_is_consumer_controlled
            and {"c1_happy_jcs", "c2_happy_cbor"} <= clean
        ),
    }

    return {
        "schema": "assay.experiment.e43_evidenceref_recompute_consumer.v0",
        "cases": len(cases),
        "verdict_counts": counts,
        "all_expected": not failures,
        "invariants": invariants,
        "all_invariants_hold": all(invariants.values()),
        "failures": failures,
        "non_claims": [
            "recomputation is not trust: a recomputed verdict means the bytes match the content "
            "address under the declared canonicalization and schema, not that the producer is honest, "
            "the issuer or signature is trusted, or the claimed effect occurred",
            "grounding the claim against an independent observed basis (agrees/contradicts/unobserved) "
            "is a separate axis; verifying issuer or signature trust is another",
            "operates on committed bytes only; queries no live producer, MCP, or attestation service",
            "required fields, completeness rules, and canonicalization profile meanings are resolved "
            "by the consumer's own trusted registries; producer-supplied schema hints, body-local "
            "schema overrides, and embedded profile definitions are never an input to the verdict",
            "jcs-json-v1 is RFC 8785 over a float-free value space and cbor-deterministic-v1 is RFC "
            "8949 sec 4.2 over the value types used here; full float and value-space coverage is out "
            "of scope for this vector set",
            "counts describe this vector set only, not real-world prevalence",
        ],
    }


def main(argv: list[str]) -> int:
    if len(argv) >= 2 and argv[1] == "emit":
        print(json.dumps(emit(), indent=2, sort_keys=True))
        return 0
    if len(argv) >= 2 and argv[1] == "verify":
        path = argv[2] if len(argv) > 2 else "vectors.json"
        doc = json.loads(pathlib.Path(path).read_text())
    else:
        doc = emit()
    result = measure(doc)
    print(json.dumps(result, indent=2, sort_keys=True))
    ok = result["all_expected"] and result["all_invariants_hold"]
    if not ok:
        for f in result["failures"]:
            print("FAIL:", f, file=sys.stderr)
        for name, held in result["invariants"].items():
            if not held:
                print("INVARIANT FAILED:", name, file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
