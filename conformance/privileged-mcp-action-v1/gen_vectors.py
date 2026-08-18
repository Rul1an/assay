#!/usr/bin/env python3
"""Deterministic vector generator for privileged-mcp-action/v1."""

import gzip
import hashlib
import io
import json
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE / "vectors"

PROFILE = "privileged-mcp-action/v1"
IMPORT_TIME = "2026-08-18T00:00:00Z"
SOURCE = "urn:assay:external:privileged-mcp-action"
PRODUCER = {"name": "privileged-mcp-action-v1-generator", "version": "0.1.0"}

DECISION_SCHEMA = "assay.enforcement_decision.v0"
OBSERVATION_V0 = "assay.denied_call_observation.v0"
OBSERVATION_V1 = "assay.denied_call_observation.v1"
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
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def sha256_hex(b: bytes) -> str:
    return "sha256:" + hashlib.sha256(b).hexdigest()


def decision_record(decision="deny", reason="no_declared_allowance"):
    return {
        "schema": DECISION_SCHEMA,
        "caller": {"id": "ci-agent"},
        "tool": {"name": TOOL, "action_class": "github_deploy_key"},
        "action": {
            "verb": "create",
            "resource_type": "github_deploy_key",
            "target": {"provider": "github", "owner": "acme", "repo": "prod-app"},
            "target_digest": DIGEST,
        },
        "decision": decision,
        "reason": reason,
        "fail_closed": decision == "deny",
        "drift_state": "satisfied" if decision == "allow" else "not_evaluated",
        "credential_alias": "gh-deploy",
        "non_claims": list(DECISION_NON_CLAIMS),
    }


def observation_record(schema, code, *, origin="assay-proxy", reason="no_declared_allowance"):
    body = f'{{"jsonrpc":"2.0","id":9,"error":{{"code":{code},"message":"denied by policy: {reason}"}}}}'
    return {
        "schema": schema,
        "call": {"tool_name": TOOL, "target_digest": DIGEST},
        "caller_visible_error": {"code": code, "origin": origin, "reason": reason},
        "caller_visible_response_digest": sha256_hex(body.encode("utf-8")),
        "non_claims": list(OBSERVATION_NON_CLAIMS),
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


def bundle_bytes(run_id: str, payloads: list) -> bytes:
    events = [event(run_id, i, p) for i, p in enumerate(payloads)]
    events_ndjson = b"\n".join(jcs(e) for e in events) + b"\n"
    run_root = sha256_hex(b"".join(e["assaycontenthash"].encode() + b"\n" for e in events))
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


def claims(policy, denial):
    out = {}
    for name, status in (
        ("policy_decision_recorded", policy),
        ("caller_visible_denial", denial),
        ("upstream_delivery", "incomplete"),
        ("external_side_effect", "incomplete"),
    ):
        cell = {"status": status}
        if status in ("confirmed", "refuted"):
            cell["source_class"] = "producer_reported"
        out[name] = cell
    return out


VECTORS = [
    {
        "id": "ok-001-deny-bound-v1-observation",
        "payloads": lambda: [
            decision_record("deny"),
            observation_record(OBSERVATION_V1, -31999),
        ],
        "expected": {
            "bundle_integrity": "pass",
            "verdict": "valid",
            "claims": claims("confirmed", "confirmed"),
        },
        "description": "Deny with the exact v1 marker triple confirms caller_visible_denial.",
    },
    {
        "id": "ok-002-deny-observation-missing",
        "payloads": lambda: [decision_record("deny", "credential_scope_insufficient")],
        "expected": {
            "bundle_integrity": "pass",
            "verdict": "valid",
            "claims": claims("confirmed", "incomplete"),
        },
        "description": "Deny without an observation stays incomplete; selected profile is still v1.",
    },
    {
        "id": "ok-003-cross-pair-inert",
        "payloads": lambda: [
            decision_record("deny"),
            observation_record(OBSERVATION_V1, -32042),
        ],
        "expected": {
            "bundle_integrity": "pass",
            "verdict": "valid",
            "claims": claims("confirmed", "incomplete"),
        },
        "description": "v1 schema with legacy -32042 is well-formed and inert, not a marker.",
    },
    {
        "id": "ok-004-allow-contradicted-by-v1-denial",
        "payloads": lambda: [
            decision_record("allow", "allow"),
            observation_record(OBSERVATION_V1, -31999),
        ],
        "expected": {
            "bundle_integrity": "pass",
            "verdict": "valid",
            "claims": claims("confirmed", "refuted"),
        },
        "description": "Allow plus bound v1 marker refutes caller_visible_denial.",
    },
    {
        "id": "bad-201-v0-observation-under-v1",
        "payloads": lambda: [
            decision_record("deny"),
            observation_record(OBSERVATION_V0, -32042),
        ],
        "expected": {"bundle_integrity": "pass", "verdict": "invalid"},
        "first_failure": "unknown_profile_schema",
        "description": "A v0 observation under the v1 profile fails closed.",
    },
    {
        "id": "bad-202-mixed-marker-versions",
        "payloads": lambda: [
            decision_record("deny"),
            observation_record(OBSERVATION_V1, -31999),
            observation_record(OBSERVATION_V0, -32042),
        ],
        "expected": {"bundle_integrity": "pass", "verdict": "invalid"},
        "first_failure": "unknown_profile_schema",
        "description": "Mixed v0 and v1 observation records fail closed.",
    },
    {
        "id": "bad-203-v1-marker-without-decision",
        "payloads": lambda: [observation_record(OBSERVATION_V1, -31999)],
        "expected": {"bundle_integrity": "pass", "verdict": "invalid"},
        "first_failure": "decision_missing",
        "description": "A v1 marker with no decision record is invalid.",
    },
]


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    manifest_vectors = []
    for vec in VECTORS:
        run_id = f"pmav1-{vec['id']}"
        blob = bundle_bytes(run_id, vec["payloads"]())
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

    corpus_digest = sha256_hex(b"".join(v["sha256"].encode() + b"\n" for v in manifest_vectors))
    manifest = {
        "suite": "privileged-mcp-action-v1-conformance",
        "profile": PROFILE,
        "spec": "docs/profiles/privileged-mcp-action/v1.md",
        "generator": "gen_vectors.py (single generator; byte-deterministic)",
        "normative_surface": (
            "expected.bundle_integrity, expected.verdict and expected.claims are the "
            "normative comparison surface; first_failure_informative is informative only"
        ),
        "corpus_digest_method": 'sha256 over concat(vector sha256 + "\\n") in listed order',
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
