#!/usr/bin/env python3
"""Deterministic vector generator for the privileged-mcp-action/v0 open profile.

Single-generator, byte-deterministic: running this script twice produces
byte-identical bundles, MANIFEST.json included. Python 3 standard library only.

Canonicalization note: content hashes use RFC 8785 (JCS). For the payloads in
this corpus (ASCII member names, strings, booleans, and small integers; no
floats, no non-BMP text) `json.dumps(obj, sort_keys=True, separators=(",", ":"),
ensure_ascii=False)` is byte-identical to JCS, which is why the standard
library suffices here. A generator emitting other value shapes must use a full
RFC 8785 implementation.

Bundle format (Evidence Contract v1, mirrored from the shipped verifier):
  - tar.gz with exactly two entries, `manifest.json` first, then `events.ndjson`
  - event content_hash = "sha256:" + hex(sha256(JCS({specversion, type,
    datacontenttype, subject?, data})))  (subject omitted when absent)
  - run_root = "sha256:" + hex(sha256(concat(content_hash_i + "\n")))
  - manifest: schema_version 1, bundle_id == run_root, algorithms block,
    files map covering events.ndjson (path, sha256, bytes)
Deterministic archive metadata: mtime 0, uid/gid 0, empty uname/gname,
mode 0644, gzip mtime 0.
"""

import gzip
import hashlib
import io
import json
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE / "vectors"

PROFILE = "privileged-mcp-action/v0"
IMPORT_TIME = "2026-07-24T00:00:00Z"
SOURCE = "urn:assay:external:privileged-mcp-action"
PRODUCER = {"name": "privileged-mcp-action-v0-generator", "version": "0.1.0"}

DECISION_SCHEMA = "assay.enforcement_decision.v0"
OBSERVATION_SCHEMA = "assay.denied_call_observation.v0"
ESTABLISH_SCHEMA = "assay.manifest_establish.v0"

TOOL = "github.add_deploy_key"
DIGEST = "sha256:c3ff823d7fb2ee33b9f1a3f7be6eaf849acb980b6ec960731506436b56384dfc"

DECISION_NON_CLAIMS = [
    "policy decision only; does not assert or verify the upstream side effect (stays asserted, E9 ladder)",
    "an allow is the decision to forward; it does not assert the call reached or was performed by the upstream (a transport failure surfaces as proxy_failed, not here)",
    "credential referenced by alias only, never the token or declared scopes",
    "deny is fail-closed caution and allow is a policy decision — neither is a maliciousness verdict",
    "not the observation artifact (assay.mcp_manifest_observed.v0) and not the mechanism artifact (assay.enforcement_health.v0)",
]

OBSERVATION_NON_CLAIMS = [
    "caller-visible proxy denial observation only; policy decision lives in assay.enforcement_decision.v0",
    "does not assert or verify the upstream side effect",
    "does not assert maliciousness, safety, approval, or whole-action trust",
    "must not be read as a replacement for the bound enforcement decision record",
]


def jcs(obj) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode(
        "utf-8"
    )


def sha256_hex(b: bytes) -> str:
    return "sha256:" + hashlib.sha256(b).hexdigest()


def decision_record(decision="deny", reason="no_declared_allowance", *, digest=DIGEST,
                    fail_closed=None, drift_state=None):
    if fail_closed is None:
        fail_closed = decision == "deny"
    if drift_state is None:
        drift_state = "satisfied" if decision == "allow" else "not_evaluated"
    return {
        "schema": DECISION_SCHEMA,
        "caller": {"id": "ci-agent"},
        "tool": {"name": TOOL, "action_class": "github_deploy_key"},
        "action": {
            "verb": "create",
            "resource_type": "github_deploy_key",
            "target": {"provider": "github", "owner": "acme", "repo": "prod-app"},
            "target_digest": digest,
        },
        "decision": decision,
        "reason": reason,
        "fail_closed": fail_closed,
        "drift_state": drift_state,
        "credential_alias": "gh-deploy",
        "non_claims": list(DECISION_NON_CLAIMS),
    }


def observation_record(*, tool=TOOL, digest=DIGEST, reason="no_declared_allowance"):
    body = f'{{"jsonrpc":"2.0","id":9,"error":{{"code":-32042,"message":"denied by policy: {reason}"}}}}'
    return {
        "schema": OBSERVATION_SCHEMA,
        "call": {"tool_name": tool, "target_digest": digest},
        "caller_visible_error": {"code": -32042, "origin": "assay-proxy", "reason": reason},
        "caller_visible_response_digest": sha256_hex(body.encode("utf-8")),
        "non_claims": list(OBSERVATION_NON_CLAIMS),
    }


def establish_record(path="established_then_denied", outcome="complete"):
    return {
        "schema": ESTABLISH_SCHEMA,
        "establish_path": path,
        "establish_attempted": outcome != "not_performed",
        "action_class": "github_deploy_key",
        "run_outcome": outcome,
    }


def event(run_id: str, seq: int, payload: dict) -> dict:
    subset = {
        "specversion": "1.0",
        "type": payload["schema"],
        "datacontenttype": "application/json",
        "data": payload,
    }
    content_hash = sha256_hex(jcs(subset))
    return {
        "specversion": "1.0",
        "type": payload["schema"],
        "source": SOURCE,
        "id": f"{run_id}:{seq}",
        "time": IMPORT_TIME,
        "datacontenttype": "application/json",
        "assayrunid": run_id,
        "assayseq": seq,
        "assayproducer": PRODUCER["name"],
        "assayproducerversion": PRODUCER["version"],
        "assaygit": "unknown",
        "assaypii": False,
        "assaysecrets": False,
        "assaycontenthash": content_hash,
        "data": payload,
    }


def bundle_bytes(run_id: str, payloads: list, *, tamper: bool = False) -> bytes:
    events = [event(run_id, i, p) for i, p in enumerate(payloads)]
    lines = [jcs(e) for e in events]
    events_ndjson = b"\n".join(lines) + b"\n"
    run_root = sha256_hex(
        b"".join(e["assaycontenthash"].encode() + b"\n" for e in events)
    )
    manifest = {
        "schema_version": 1,
        "bundle_id": run_root,
        "producer": PRODUCER,
        "run_id": run_id,
        "event_count": len(events),
        "run_root": run_root,
        "algorithms": {
            "canon": "jcs-rfc8785",
            "hash": "sha256",
            "root": 'sha256(concat(content_hash + "\\n"))',
        },
        "files": {
            "events.ndjson": {
                "path": "events.ndjson",
                "sha256": sha256_hex(events_ndjson),
                "bytes": len(events_ndjson),
            }
        },
    }
    manifest_bytes = jcs(manifest)
    if tamper:
        # Flip one byte inside the stored events file AFTER the manifest is
        # computed: the file hash in the manifest no longer matches the bytes,
        # so bundle verification must fail before any profile semantics run.
        events_ndjson = events_ndjson.replace(b'"ci-agent"', b'"ci-agenX"', 1)

    buf = io.BytesIO()
    gz = gzip.GzipFile(fileobj=buf, mode="wb", mtime=0)
    with tarfile.open(fileobj=gz, mode="w") as tar:
        for name, blob in (("manifest.json", manifest_bytes), ("events.ndjson", events_ndjson)):
            info = tarfile.TarInfo(name=name)
            info.size = len(blob)
            info.mtime = 0
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mode = 0o644
            tar.addfile(info, io.BytesIO(blob))
    gz.close()
    return buf.getvalue()


def claims(policy, denial, delivery="incomplete", effect="incomplete"):
    out = {}
    for name, status in (
        ("policy_decision_recorded", policy),
        ("caller_visible_denial", denial),
        ("upstream_delivery", delivery),
        ("external_side_effect", effect),
    ):
        cell = {"status": status}
        if status in ("confirmed", "refuted"):
            cell["source_class"] = "producer_reported"
        out[name] = cell
    return out


VECTORS = [
    # ---- accept side: bundle-valid AND profile-valid --------------------------
    {
        "id": "ok-001-deny-bound-observation",
        "payloads": lambda: [
            decision_record("deny", "no_declared_allowance"),
            observation_record(reason="no_declared_allowance"),
        ],
        "expected": {
            "bundle_integrity": "pass",
            "verdict": "valid",
            "claims": claims("confirmed", "confirmed"),
        },
        "description": "Deny with a digest-bound caller-visible denial observation: both confirmable claims confirm; delivery and side effect stay incomplete.",
    },
    {
        "id": "ok-002-deny-observation-missing",
        "payloads": lambda: [decision_record("deny", "credential_scope_insufficient")],
        "expected": {
            "bundle_integrity": "pass",
            "verdict": "valid",
            "claims": claims("confirmed", "incomplete"),
        },
        "description": "Deny without an observation (a denied notification produces no caller-visible record): the decision confirms, the caller-visible denial stays incomplete.",
    },
    {
        "id": "ok-003-allow-no-outcome-observation",
        "payloads": lambda: [decision_record("allow", "allow")],
        "expected": {
            "bundle_integrity": "pass",
            "verdict": "valid",
            "claims": claims("confirmed", "incomplete"),
        },
        "description": "Allow: the decision confirms and nothing else does. An allow is the decision to forward, never delivery or side-effect proof.",
    },
    {
        "id": "ok-004-allow-with-diagnostic-establish",
        "payloads": lambda: [
            decision_record("allow", "allow"),
            establish_record("established_then_allowed", "complete"),
        ],
        "expected": {
            "bundle_integrity": "pass",
            "verdict": "valid",
            "claims": claims("confirmed", "incomplete"),
        },
        "description": "The establish record is diagnostic journey only: its presence changes no claim cell.",
    },
    # ---- refuted: bundle-valid, profile-valid, caller outcome refuted ---------
    {
        "id": "ok-005-allow-contradicted-by-denial",
        "payloads": lambda: [
            decision_record("allow", "allow"),
            observation_record(reason="no_declared_allowance"),
        ],
        "expected": {
            "bundle_integrity": "pass",
            "verdict": "valid",
            "claims": claims("confirmed", "refuted"),
        },
        "description": "An allow decision beside a digest-bound denial observation for the same (tool, target): the caller-visible outcome is refuted, and the contradiction is the finding.",
    },
    # ---- reject side ----------------------------------------------------------
    {
        "id": "bad-101-tampered-bundle",
        "payloads": lambda: [
            decision_record("deny", "no_declared_allowance"),
            observation_record(),
        ],
        "tamper": True,
        "expected": {"bundle_integrity": "fail"},
        "first_failure": "bundle_integrity",
        "description": "One byte flipped in the stored events file after the manifest was written: integrity fails before any profile semantics run.",
    },
    {
        "id": "bad-102-missing-target-digest",
        "payloads": lambda: [
            decision_record("deny", "unclassified_tool_call", digest=None)
        ],
        "expected": {"bundle_integrity": "pass", "verdict": "invalid"},
        "first_failure": "target_digest_missing",
        "description": "A decision with a null target digest cannot be bound and falls outside the profile: v0 covers classified privileged actions only.",
    },
    {
        "id": "bad-103-two-decisions",
        "payloads": lambda: [
            decision_record("deny", "no_declared_allowance"),
            decision_record("allow", "allow"),
        ],
        "expected": {"bundle_integrity": "pass", "verdict": "invalid"},
        "first_failure": "decision_cardinality",
        "description": "Two decision records in one bundle: v0 is single-call by design (no concurrency-safe correlation id exists yet), so cardinality above one is invalid, never heuristically paired.",
    },
    {
        "id": "bad-104-unknown-schema",
        "payloads": lambda: [
            decision_record("deny", "no_declared_allowance"),
            {"schema": "assay.enforcement_decision.v1", "decision": "deny"},
        ],
        "expected": {"bundle_integrity": "pass", "verdict": "invalid"},
        "first_failure": "unknown_schema",
        "description": "An event claiming an unrecognized schema in the profile's namespace fails closed: unknown never degrades to ignored.",
    },
    {
        "id": "bad-105-observation-binding-mismatch",
        "payloads": lambda: [
            decision_record("deny", "no_declared_allowance"),
            observation_record(
                digest="sha256:0000000000000000000000000000000000000000000000000000000000000000"
            ),
        ],
        "expected": {"bundle_integrity": "pass", "verdict": "invalid"},
        "first_failure": "observation_binding",
        "description": "The observation's (tool_name, target_digest) does not equal the decision's (tool.name, action.target_digest): the binding is invalid, not ignored.",
    },
    {
        "id": "bad-106-fail-closed-inconsistent",
        "payloads": lambda: [
            decision_record("deny", "no_declared_allowance", fail_closed=False)
        ],
        "expected": {"bundle_integrity": "pass", "verdict": "invalid"},
        "first_failure": "fail_closed_derivation",
        "description": "fail_closed is derived (true iff deny) in the producer; a record where it diverges is malformed, not a different policy statement.",
    },
    {
        "id": "bad-107-unknown-decision-value",
        "payloads": lambda: [
            decision_record("permit", "no_declared_allowance", fail_closed=True)
        ],
        "expected": {"bundle_integrity": "pass", "verdict": "invalid"},
        "first_failure": "decision_vocabulary",
        "description": "decision outside the closed {allow, deny} vocabulary fails closed.",
    },
    {
        "id": "bad-108-observation-without-decision",
        "payloads": lambda: [observation_record()],
        "expected": {"bundle_integrity": "pass", "verdict": "invalid"},
        "first_failure": "decision_missing",
        "description": "A caller-visible denial marker with no decision record at all: an observed enforcement marker must be backed by a bound decision, so the profile input is invalid.",
    },
]


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    manifest_vectors = []
    for vec in VECTORS:
        run_id = f"pmav0-{vec['id']}"
        blob = bundle_bytes(run_id, vec["payloads"](), tamper=vec.get("tamper", False))
        path = OUT / f"{vec['id']}.bundle.tar.gz"
        path.write_bytes(blob)
        entry = {
            "id": vec["id"],
            "file": f"vectors/{vec['id']}.bundle.tar.gz",
            "sha256": sha256_hex(blob),
            "description": vec["description"],
            "expected": vec["expected"],
        }
        if "first_failure" in vec:
            entry["first_failure_informative"] = vec["first_failure"]
        manifest_vectors.append(entry)

    corpus_digest = sha256_hex(
        b"".join(v["sha256"].encode() + b"\n" for v in manifest_vectors)
    )
    manifest = {
        "suite": "privileged-mcp-action-v0-conformance",
        "profile": PROFILE,
        "spec": "docs/profiles/privileged-mcp-action/v0.md",
        "generator": "gen_vectors.py (single generator; byte-deterministic)",
        "normative_surface": (
            "expected.bundle_integrity, expected.verdict and expected.claims are the "
            "normative comparison surface; first_failure_informative is this "
            "generator's own vocabulary and is informative only"
        ),
        "corpus_digest_method": (
            'sha256 over concat(vector sha256 + "\\n") in listed order, mirroring '
            "the bundle run_root construction"
        ),
        "corpus_digest": corpus_digest,
        "counts": {
            "accept": sum(1 for v in manifest_vectors if v["expected"].get("verdict") == "valid"),
            "reject": sum(1 for v in manifest_vectors if v["expected"].get("verdict") != "valid"),
        },
        "vectors": manifest_vectors,
    }
    (HERE / "MANIFEST.json").write_bytes(
        json.dumps(manifest, indent=2, sort_keys=False, ensure_ascii=False).encode() + b"\n"
    )
    print(f"wrote {len(manifest_vectors)} vectors, corpus_digest {corpus_digest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
