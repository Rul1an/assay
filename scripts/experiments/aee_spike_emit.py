#!/usr/bin/env python3
"""Emit a fixture-only Assay -> AEE v0.7 spike statement.

This script intentionally lives outside production crates. It tests the AEE
field shape against Assay evidence carriers and makes the missing production
primitive explicit: Assay can synthesize this fixture seal, but current Assay
carriers do not emit an AEE-compatible substrate-signed sealed record.
"""

from __future__ import annotations

import argparse
import base64
import copy
import hashlib
import hmac
import json
from pathlib import Path
from typing import Any

AEE_PR_HEAD = "c0c4da67defdf0f186f162e7ecb3f9527b6a94f8"
AEE_SPEC_SHA256 = "fda0f5f7885d56feb93194cfa604f57c060c12677f77fa5579888b15dc1d1a2d"
AEE_PREDICATE_TYPE = "https://in-toto.io/attestation/adversarial-execution-evidence/v0.7"
AEE_VERSION = "0.7"
PAYLOAD_TYPE = "application/vnd.assay.aee-spike.observation.v0+json"
FIXTURE_KEY_ID = "assay-aee-spike-fixture-key-v0"
FIXTURE_KEY = b"assay-aee-spike-fixture-key-v0-not-production"

ROOT = Path(__file__).resolve().parent
FIXTURES = ROOT / "fixtures" / "aee"
DEFAULT_OUT = ROOT / "fixtures" / "aee" / "statement-valid.json"


def canonical_bytes(value: Any) -> bytes:
    """Return deterministic JSON bytes close to the JCS shape used by AEE.

    The fixture uses ASCII-only member names/values and safe integers. This is
    not a general RFC 8785 implementation; the checker keeps the same helper so
    the spike has one rule in one function rather than parallel approximations.
    """

    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def sha256_hex_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def digest_json(value: Any) -> str:
    return sha256_hex_bytes(canonical_bytes(value))


def read_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def dsse_pae(payload_type: str, payload: bytes) -> bytes:
    return b"DSSEv1 " + str(len(payload_type)).encode("ascii") + b" " + payload_type.encode("utf-8") + b" " + str(len(payload)).encode("ascii") + b" " + payload


def sign_payload(payload_type: str, payload: bytes) -> str:
    """Deterministic fixture-only signature, not a production DSSE algorithm."""

    return base64.b64encode(hmac.new(FIXTURE_KEY, dsse_pae(payload_type, payload), hashlib.sha256).digest()).decode("ascii")


def observation_record(payload: dict[str, Any], seq: int) -> dict[str, Any]:
    payload_bytes = canonical_bytes(payload)
    return {
        "payload": base64.b64encode(payload_bytes).decode("ascii"),
        "payloadType": PAYLOAD_TYPE,
        "signatures": [{"keyid": FIXTURE_KEY_ID, "sig": sign_payload(PAYLOAD_TYPE, payload_bytes)}],
        "seq": seq,
    }


def merkle_root(leaves: list[dict[str, Any]]) -> str:
    """RFC6962-style SHA-256 Merkle root over canonical observation records."""

    if not leaves:
        return ""
    layer = [hashlib.sha256(b"\x00" + canonical_bytes(leaf)).digest() for leaf in leaves]
    while len(layer) > 1:
        next_layer: list[bytes] = []
        for idx in range(0, len(layer), 2):
            if idx + 1 == len(layer):
                next_layer.append(layer[idx])
            else:
                next_layer.append(hashlib.sha256(b"\x01" + layer[idx] + layer[idx + 1]).digest())
        layer = next_layer
    return layer[0].hex()


def build_statement() -> dict[str, Any]:
    corpus_manifest = read_json(FIXTURES / "corpus-manifest.json")
    substrate_descriptor = read_json(FIXTURES / "substrate-descriptor.json")
    catch_policy = read_json(FIXTURES / "catch-policy.json")
    proxy_observation = read_json(FIXTURES / "proxy-deny-observation.json")
    enforcement_health = read_json(FIXTURES / "enforcement-health-v1-active-probe.json")

    subject = {
        "name": "assay-aee-spike-fixture-subject",
        "digest": {"sha256": digest_json({"fixture": "assay-aee-spike-subject", "version": 1})},
    }
    substrate = {
        "name": "assay-runtime-substrate-spike",
        "digest": {"sha256": digest_json(substrate_descriptor)},
    }
    catch_policy_descriptor = {
        "name": "assay-aee-spike-catch-policy",
        "digest": {"sha256": digest_json(catch_policy)},
    }
    network_posture = {
        "posture": "sinkhole",
        "digest": {"sha256": digest_json({"landlock": enforcement_health["landlock"], "scope": enforcement_health["scope"]})},
    }
    observation_vocabulary = {
        "labels": ["connect_blocked", "no_connect_block", "no_proxy_denial", "proxy_denied"],
        "caught": ["connect_blocked", "proxy_denied"],
    }
    observation_vocabulary["digest"] = {"sha256": digest_json({"caught": observation_vocabulary["caught"], "labels": observation_vocabulary["labels"]})}
    run_entropy = {
        "digest": {"sha256": digest_json({"fixtureRun": "assay-aee-spike-2026-08-04T00:00:00Z", "operator": "assay-spike-fixture"})}
    }

    corpus = {
        "name": "assay-aee-spike-corpus",
        "uri": "pkg:assay/aee-spike-corpus@2026-08-04",
        "digest": {"sha256": digest_json(corpus_manifest)},
        "manifest": corpus_manifest,
    }

    run_binding_input = {
        "aeeBindingVersion": "2",
        "catchPolicy": catch_policy_descriptor["digest"]["sha256"],
        "corpus": corpus["digest"]["sha256"],
        "networkPosture": digest_json(network_posture),
        "observationVocabulary": observation_vocabulary["digest"]["sha256"],
        "runEntropy": run_entropy["digest"]["sha256"],
        "subject": subject["digest"]["sha256"],
        "substrate": substrate["digest"]["sha256"],
    }
    run_binding = digest_json(run_binding_input)

    proxy_commitment = proxy_observation["caller_visible_response_digest"].removeprefix("sha256:")
    network_commitment = enforcement_health["probe"]["payload_commitment"].removeprefix("sha256:")

    arming_payload = {
        "aeeKind": "arming",
        "aeeVersion": AEE_VERSION,
        "aeeBindingVersion": "2",
        "aeeRunBinding": run_binding,
        "armedAt": "2026-08-04T00:00:00Z",
        "aeePostureDigest": network_posture["digest"]["sha256"],
        "aeeAssessedAttacks": ["MCP-PROXY-DENY-001", "NET-CONNECT-BLOCK-001"],
        "aeeChainScope": ["subject", "substrate"],
        "fixtureSource": "synthetic arming record; not emitted by production Assay",
    }
    proxy_payload = {
        "aeeKind": "interception",
        "aeeVersion": AEE_VERSION,
        "aeeRunBinding": run_binding,
        "aeeMethod": "intercepted",
        "aeePayloadCommitment": proxy_commitment,
        "layer": "proxy",
        "schema": proxy_observation["schema"],
        "fixtureSource": "derived from assay.denied_call_observation.v0 fixture",
    }
    network_payload = {
        "aeeKind": "interception",
        "aeeVersion": AEE_VERSION,
        "aeeRunBinding": run_binding,
        "aeeMethod": "intercepted",
        "aeePayloadCommitment": network_commitment,
        "layer": "landlock",
        "schema": f"{enforcement_health['schema']}.probe",
        "fixtureSource": "derived from assay.enforcement_health.v1 active probe fixture",
    }

    # The seal's aeeObservedSet is computed over the carried interception labels
    # and attacks. It is a fixture stand-in for the production primitive Assay lacks.
    sealed_payload = {
        "aeeKind": "sealed",
        "aeeVersion": AEE_VERSION,
        "aeeRunBinding": run_binding,
        "aeeMethod": "intercepted",
        "aeePostureDigest": network_posture["digest"]["sha256"],
        "aeeStillArmed": True,
        "aeeDropCount": 0,
        "aeeDropBound": 0,
        "aeeObservedSet": ["connect_blocked", "proxy_denied"],
        "aeeObservedAttacks": ["MCP-PROXY-DENY-001", "NET-CONNECT-BLOCK-001"],
        "fixtureSource": "synthetic sealed record; current Assay does not emit this production primitive",
    }

    records = [
        observation_record(proxy_payload, 1),
        observation_record(arming_payload, 2),
        observation_record(sealed_payload, 3),
        observation_record(network_payload, 4),
    ]

    statement = {
        "_type": "https://in-toto.io/Statement/v1",
        "subject": [subject],
        "predicateType": AEE_PREDICATE_TYPE,
        "predicate": {
            "result": "fail",
            "observationEnvironment": {
                "substrate": substrate,
                "corpus": corpus,
                "catchPolicy": catch_policy_descriptor,
                "networkPosture": network_posture,
                "observationVocabulary": observation_vocabulary,
                "runEntropy": run_entropy,
            },
            "coverage": {"assessedClasses": ["MCP", "NET"], "outOfScope": {}, "routedElsewhere": {}},
            "attackResults": [
                {
                    "attackId": "MCP-PROXY-DENY-001",
                    "containmentObserved": "proxy_denied",
                    "basis": "substrate",
                    "method": "intercepted",
                    "attribution": "pinned",
                    "actualLayer": "proxy",
                    "observationRefs": [0, 1, 2],
                },
                {
                    "attackId": "NET-CONNECT-BLOCK-001",
                    "containmentObserved": "connect_blocked",
                    "basis": "substrate",
                    "method": "intercepted",
                    "attribution": "pinned",
                    "actualLayer": "landlock",
                    "observationRefs": [3, 1, 2],
                },
            ],
            "observationRecords": records,
            "batchRoot": merkle_root(records),
            "doesNotAssert": [
                "production AEE support",
                "stable AEE support before in-toto predicate acceptance",
                "complete run population",
                "agent safety",
                "independent substrate operation when one fixture key signs both paths",
                "that production Assay emits substrate-signed sealed records",
                "that ProtoJSON or generated bindings are canonical evidence",
            ],
            "issuedAt": "2026-08-04T00:00:00Z",
            "_ext": {
                "assaySpike": {
                    "aeePrHead": AEE_PR_HEAD,
                    "aeeSpecSha256": AEE_SPEC_SHA256,
                    "runBindingInput": run_binding_input,
                    "nonProduction": True,
                }
            },
        },
    }
    return statement


def write_variant(path: Path, statement: dict[str, Any], variant: str) -> None:
    mutated = copy.deepcopy(statement)
    predicate = mutated["predicate"]
    if variant == "missing-seal":
        predicate["observationRecords"] = [r for r in predicate["observationRecords"] if decode_payload(r)["aeeKind"] != "sealed"]
        for row in predicate["attackResults"]:
            row["observationRefs"] = [idx for idx in row["observationRefs"] if idx != 2]
        predicate["batchRoot"] = merkle_root(predicate["observationRecords"])
    elif variant == "defective-unreferenced-seal":
        bad_payload = {
            "aeeKind": "sealed",
            "aeeVersion": AEE_VERSION,
            "aeeRunBinding": predicate["_ext"]["assaySpike"]["runBindingInput"]["subject"],
            "aeeMethod": "intercepted",
            "aeePostureDigest": "0" * 64,
            "aeeStillArmed": False,
            "aeeDropCount": 1,
            "aeeDropBound": 0,
            "aeeObservedSet": ["proxy_denied"],
            "aeeObservedAttacks": ["MCP-PROXY-DENY-001"],
            "fixtureSource": "intentionally defective unreferenced seal",
        }
        predicate["observationRecords"].append(observation_record(bad_payload, 5))
        predicate["batchRoot"] = merkle_root(predicate["observationRecords"])
    elif variant == "artifact-labelled-substrate":
        predicate["attackResults"][0]["basis"] = "substrate"
        predicate["attackResults"][0]["observationRefs"] = []
    elif variant == "reconstructed-priced-intercepted":
        predicate["attackResults"][0]["method"] = "intercepted"
        payload = decode_payload(predicate["observationRecords"][0])
        payload["aeeMethod"] = "reconstructed"
        predicate["observationRecords"][0] = observation_record(payload, 1)
        predicate["batchRoot"] = merkle_root(predicate["observationRecords"])
    elif variant == "run-population-overclaim":
        predicate["doesNotAssert"] = [claim for claim in predicate["doesNotAssert"] if claim != "complete run population"]
        predicate["_ext"]["assaySpike"]["invalidClaim"] = "no sibling runs existed"
    else:
        raise ValueError(f"unknown variant: {variant}")
    path.write_text(json.dumps(mutated, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def decode_payload(record: dict[str, Any]) -> dict[str, Any]:
    return json.loads(base64.b64decode(record["payload"]).decode("utf-8"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT, help="Path for the valid emitted statement")
    parser.add_argument("--variants", action="store_true", help="Also emit intentionally invalid negative-control statements")
    args = parser.parse_args()

    statement = build_statement()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(statement, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    if args.variants:
        variants_dir = args.out.parent / "negative-controls"
        variants_dir.mkdir(parents=True, exist_ok=True)
        for name in [
            "missing-seal",
            "defective-unreferenced-seal",
            "artifact-labelled-substrate",
            "reconstructed-priced-intercepted",
            "run-population-overclaim",
        ]:
            write_variant(variants_dir / f"statement-{name}.json", statement, name)

    print(f"wrote {args.out}")
    if args.variants:
        print(f"wrote negative controls under {args.out.parent / 'negative-controls'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
