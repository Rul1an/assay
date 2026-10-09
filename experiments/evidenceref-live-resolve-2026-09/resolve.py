#!/usr/bin/env python3
"""Resolve one LIVE, third-party evidenceRef with the published Assay consumer.

The June experiment (`../evidenceref-recompute-consumer-2026-06`) states as its fourth non-claim that
it operates on committed bytes only and queries no producer. This experiment is the one step past
that: it takes a reference published by a producer we do not control, pins the referenced octets by
their own content address, and resolves them with that consumer unmodified.

Three reference forms over one pinned body, because the finding is about the REFERENCE and not about
the body:

  A  digest + locator, exactly the shape the producer publishes. No canonicalization is named.
  B  the same, read charitably as `jcs-json-v1`, which is the scheme the producer names for the
     surfaces it grades (its own condition 07 requires `rfc8785-jcs` of every server it measures).
  C  the same, naming a proposed `octets-as-served-v1` profile plus a schema identity the consumer
     resolves from its own registry.

A and B run through the published `consume()` untouched. C cannot: an octets profile hashes the
received bytes, so a consumer that parses into an object before hashing structurally cannot support
it. That is a finding in its own right and is why C is carried here as a proposed profile rather than
smuggled into the published consumer's registry.

Usage: python3 resolve.py            # writes runs/resolve-run.json, exit 0 iff every expectation holds
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import sys
import types

HERE = pathlib.Path(__file__).parent
PUBLISHED = HERE.parent / "evidenceref-recompute-consumer-2026-06"

# The consumer linked from the June SEP-1913 comment, by its git blob id. The runner refuses to
# consume anything else, so "unmodified from the one I published" is enforced where the object is
# loaded rather than asserted in a test that hashes a path this module never reads.
PUBLISHED_BLOB = "f48bb7410935caddd19f3d3b6d4a789b4bd02e97"
# The pinned consumer's exact length. A candidate longer than this cannot be the pinned blob, so
# there is no reason to hold it in memory to find that out: an 8 MiB file was fully materialised
# before the digest refused it. The ceiling belongs before the hash, as it does on the wire.
PUBLISHED_LEN = 27787


def git_blob_sha1(data: bytes) -> str:
    """Git's object id for a blob: sha1 over "blob <len>\0" + content. An identity, not a
    security primitive; the bytes it names are already public."""
    header = b"blob %d\0" % len(data)
    try:
        return hashlib.sha1(header + data, usedforsecurity=False).hexdigest()
    except TypeError:  # pragma: no cover  Python < 3.9
        return hashlib.sha1(header + data).hexdigest()


def load_published_consumer(directory: pathlib.Path = PUBLISHED, expected: str = PUBLISHED_BLOB):
    """Import the consumer only after its bytes match the pinned blob. Refusal is the point: a
    resolution by 'the published consumer' means nothing if the runner will consume any file
    sitting at that path."""
    path = directory / "evidenceref_consumer.py"
    if not path.is_file():
        raise SystemExit(f"published consumer not found at {path}")
    # One bounded read. The bytes verified below are the bytes executed below; nothing goes back
    # to the filesystem or to a module name in between.
    #
    # Two ways this was wrong before, both reproduced. `import evidenceref_consumer` is answered
    # from sys.modules when a module of that name is already loaded, so a pre-loaded module whose
    # `_is_redacted` returned False was handed back intact while the check reported a clean pin.
    # Loading by path instead fixed that but left a swap between the two reads: hash the file,
    # have it replaced, execute the replacement. Reading once closes both, because there is only
    # one byte object and it is the one that was hashed.
    with path.open("rb") as handle:
        source = handle.read(PUBLISHED_LEN + 1)
    if len(source) > PUBLISHED_LEN:
        raise SystemExit(
            f"refusing an oversize consumer: {path} exceeds the pinned {PUBLISHED_LEN} bytes"
        )
    actual = git_blob_sha1(source)
    if actual != expected:
        raise SystemExit(
            f"refusing an unpinned consumer: {path} is blob {actual}, expected {expected}"
        )
    module = types.ModuleType("_assay_pinned_evidenceref_consumer")
    module.__file__ = str(path)
    exec(compile(source, str(path), "exec"), module.__dict__)  # noqa: S102  verified bytes only
    return module


published = load_published_consumer()

RECORD = HERE / "record" / "gate-record-4dffe218.json"
ADDRESS = "sha256:4dffe218167d40bb5940ba6cd404225b95b3d20ca5cdac880fbd4c62f0e343fc"

# The proposed profile. It is deliberately NOT added to the published consumer's registry: naming a
# profile the published consumer does not know is exactly what the producer's reference would have to
# do today, and a consumer must refuse a name it cannot resolve rather than assume one.
OCTETS_PROFILE = "octets-as-served-v1"

# Schema authority stays consumer-side, as in the published consumer. The producer's record carries no
# schema identity of its own, so the identity is declared ON THE REFERENCE and the completeness rules
# come from here. A producer cannot declare one of its own fields non-required.
SCHEMA_REGISTRY = {
    "mcp-verification-gate/0.4.1": {
        "required_fields": [
            "endpoint",
            "checked_at",
            "status",
            "reachable",
            "checks",
            "absence_vs_failure",
            "establishes",
            "does_not_establish",
        ]
    }
}


def _v(verdict: str, reason: str) -> dict:
    return {"verdict": verdict, "reason": reason}


def consume_octets(ref: dict, octet_store: dict, schema_registry: dict = SCHEMA_REGISTRY) -> dict:
    """The published consumer's shape, over octets instead of over a parsed object.

    The ordering is load-bearing and is tested: the content address is checked BEFORE the bytes are
    parsed or interpreted, so a tampered body is refused as a digest failure and never reaches the
    completeness rules. Interpreting first and hashing second would let a body decide which rule
    judges it.
    """
    if not ref.get("digest") or not ref.get("canonicalization"):
        return _v("malformed_ref", "missing_required_field_digest_or_canonicalization")

    canon = ref["canonicalization"]
    locator = ref.get("ref")

    if locator is None:
        return _v("unresolvable_digest_only", "digest_alone_insufficient_no_resolvable_body")
    if locator not in octet_store:
        return _v("unresolved_ref", "ref_present_but_body_not_resolvable")
    octets = octet_store[locator]

    if not isinstance(canon, str) or canon != OCTETS_PROFILE:
        return _v("unsupported_canonicalization", "declared_profile_not_in_consumer_profile_registry")

    # 1. Integrity before interpretation.
    recomputed = "sha256:" + hashlib.sha256(octets).hexdigest()
    if recomputed != ref["digest"]:
        return _v("digest_mismatch", "recompute_over_received_octets_diverges_from_committed_digest")

    # 2. Only now is it safe to read the body.
    try:
        body = json.loads(octets)
    except ValueError:
        return _v("malformed_body", "octets_match_address_but_do_not_parse")

    declared = f"{ref.get('schema')}/{ref.get('schema_version')}"
    spec = schema_registry.get(declared)
    if spec is None:
        return _v("schema_mismatch", f"unknown_schema_{declared}")

    incomplete = [f for f in spec["required_fields"] if f not in body or published._is_redacted(body[f])]
    if incomplete:
        return _v("redacted_projection_incomplete", "missing_or_redacted_required_evidence:" + ",".join(incomplete))

    return _v("recomputed", "octets_match_address_under_declared_profile_and_schema_complete")


def build_cases() -> list[dict]:
    octets = RECORD.read_bytes()
    body = json.loads(octets)

    reserialized = json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    tampered = octets.replace(b'"gate_version":"0.4.1"', b'"gate_version":"0.4.2"', 1)
    incomplete_body = {k: v for k, v in body.items() if k != "absence_vs_failure"}
    incomplete_octets = json.dumps(incomplete_body, separators=(",", ":"), ensure_ascii=False).encode()

    base = {"digest": ADDRESS, "ref": "gate-record"}
    schema = {"schema": "mcp-verification-gate", "schema_version": "0.4.1"}

    return [
        {
            "id": "A_as_published_no_canonicalization",
            "mode": "published_consumer",
            "note": "the reference shape the producer serves today: an address and a locator",
            "ref": dict(base),
            "expect": "malformed_ref",
        },
        {
            "id": "B_charitable_jcs",
            "mode": "published_consumer",
            "note": "read as RFC 8785 JCS, the scheme the producer requires of every server it grades",
            "ref": {**base, **schema, "canonicalization": "jcs-json-v1"},
            "expect": "digest_mismatch",
        },
        {
            "id": "C_octets_profile_named",
            "mode": "octets_consumer",
            "note": "the same bytes, with the profile and the schema identity named on the reference",
            "ref": {**base, **schema, "canonicalization": OCTETS_PROFILE},
            "octets": octets,
            "expect": "recomputed",
        },
        {
            "id": "D_octets_reserialized_body",
            "mode": "octets_consumer",
            "note": "same object, sorted-key reserialization: a byte-pinned reference does not survive it",
            "ref": {**base, **schema, "canonicalization": OCTETS_PROFILE},
            "octets": reserialized,
            "expect": "digest_mismatch",
        },
        {
            "id": "E_octets_tampered",
            "mode": "octets_consumer",
            "note": "one flipped version digit; must be refused at the address, not at the schema",
            "ref": {**base, **schema, "canonicalization": OCTETS_PROFILE},
            "octets": tampered,
            "expect": "digest_mismatch",
        },
        {
            "id": "F_octets_incomplete_projection",
            "mode": "octets_consumer",
            "note": "a required evidence field elided, re-addressed honestly: recomputes and is still not clean",
            "ref": {
                **base,
                **schema,
                "canonicalization": OCTETS_PROFILE,
                "digest": "sha256:" + hashlib.sha256(incomplete_octets).hexdigest(),
            },
            "octets": incomplete_octets,
            "expect": "redacted_projection_incomplete",
        },
        {
            "id": "G_unknown_profile_name",
            "mode": "octets_consumer",
            "note": "a profile the consumer cannot resolve is refused, never defaulted",
            "ref": {**base, **schema, "canonicalization": "gate-internal-v9"},
            "octets": octets,
            "expect": "unsupported_canonicalization",
        },
        # H to K exist because the mutation run found their rules unexercised. A rule no case reaches
        # is decoration: it cannot be shown to discriminate, so it cannot be relied on.
        {
            "id": "H_octets_no_canonicalization",
            "mode": "octets_consumer",
            "note": "the producer shape again, this time against the octets consumer",
            "ref": dict(base),
            "octets": octets,
            "expect": "malformed_ref",
        },
        {
            "id": "I_octets_digest_only",
            "mode": "octets_consumer",
            "note": "an address with nothing to resolve is inconclusive, never clean",
            "ref": {"digest": ADDRESS, **schema, "canonicalization": OCTETS_PROFILE},
            "octets": octets,
            "expect": "unresolvable_digest_only",
        },
        {
            "id": "J_octets_locator_unresolvable",
            "mode": "octets_consumer",
            "note": "a locator the consumer cannot fetch fails closed rather than passing on the address",
            "ref": {**base, **schema, "canonicalization": OCTETS_PROFILE, "ref": "not-in-store"},
            "octets": octets,
            "expect": "unresolved_ref",
        },
        {
            "id": "K_octets_unknown_schema_identity",
            "mode": "octets_consumer",
            "note": "schema authority is consumer-side: an identity the registry does not hold is refused",
            "ref": {**base, "canonicalization": OCTETS_PROFILE,
                    "schema": "whatever-the-producer-says", "schema_version": "1"},
            "octets": octets,
            "expect": "schema_mismatch",
        },
    ]


def run() -> dict:
    octets = RECORD.read_bytes()
    body = json.loads(octets)
    cases = build_cases()
    results = []

    for c in cases:
        if c["mode"] == "published_consumer":
            got = published.consume(c["ref"], {"gate-record": body})
        else:
            got = consume_octets(c["ref"], {"gate-record": c["octets"]})
        results.append(
            {
                "id": c["id"],
                "mode": c["mode"],
                "note": c["note"],
                "declared_canonicalization": c["ref"].get("canonicalization"),
                "expected": c["expect"],
                "verdict": got["verdict"],
                "reason": got["reason"],
                "match": got["verdict"] == c["expect"],
            }
        )

    digests = {
        "octets_as_served": "sha256:" + hashlib.sha256(octets).hexdigest(),
        "jcs_json_v1": published.content_address(body, "jcs-json-v1"),
        "cbor_deterministic_v1": published.content_address(body, "cbor-deterministic-v1"),
    }

    return {
        "subject": {
            "producer": "MCP Verification Gate 0.4.1 (commit a2c5fdf7d26c)",
            "source_url": json.loads((HERE / "record" / "capture.json").read_text())["source_url"],
            "byte_length": len(octets),
            "published_address": ADDRESS,
        },
        "consumer": {
            "published": "../evidenceref-recompute-consumer-2026-06/evidenceref_consumer.py, unmodified",
            "proposed_profile": OCTETS_PROFILE,
            "schema_authority": "consumer-side registry in resolve.py, not producer-declared",
        },
        "content_addresses_of_one_object": digests,
        "same_object_same_length_different_address": (
            len(published.canonical_bytes(body, "jcs-json-v1")) == len(octets)
            and digests["jcs_json_v1"] != digests["octets_as_served"]
        ),
        "cases": results,
        "all_expected": all(r["match"] for r in results),
        "establishes": [
            "the published address recomputes from the received octets, so the reference resolves for a party holding those bytes",
            "the reference as served names no canonicalization and no schema, so the published consumer refuses it fail-closed rather than guessing",
            "naming a profile and a schema identity ON THE REFERENCE is sufficient to reach a clean verdict over the same unmodified bytes",
            "a byte-pinned reference does not survive a reserialization of the same object, which is why the scheme has to travel with the digest",
        ],
        "does_not_establish": [
            "nothing about the honesty, competence or correctness of the producer",
            "nothing about whether the measured server behaved as the record says",
            "nothing about records other than this one capture",
            "no claim that octets-as-served is the right profile to standardize; it is one profile that makes this reference resolvable",
        ],
    }


if __name__ == "__main__":
    out = run()
    (HERE / "runs").mkdir(exist_ok=True)
    (HERE / "runs" / "resolve-run.json").write_text(json.dumps(out, indent=2, sort_keys=True) + "\n")
    for r in out["cases"]:
        mark = "ok " if r["match"] else "BAD"
        print(f"{mark} {r['id']:36s} {r['verdict']}")
    print(f"\nall_expected={out['all_expected']}")
    sys.exit(0 if out["all_expected"] else 1)
