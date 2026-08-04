#!/usr/bin/env python3
"""ADR-045 Landlock run-end seal fixture/checker harness.

This is the first implementation slice for issue #1998: fixtures and checker
semantics before producer code. It deliberately does not add a production AEE
exporter or production signing primitive.
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

AEE_PREDICATE_TYPE = "https://in-toto.io/attestation/adversarial-execution-evidence/v0.7"
AEE_VERSION = "0.7"
PAYLOAD_TYPE = "application/vnd.assay.aee-landlock-seal.fixture.v0+json"
STRUCTURAL_KEY = "assay-test-observation-key-landlock-v0"
FIXTURE_KEY = "assay-aee-spike-fixture-key-v0"
FIXTURE_KEY_PREFIX = "assay-aee-spike-fixture-key"
SECRET = b"assay-aee-landlock-seal-fixture-key-not-production"
ROOT = Path(__file__).resolve().parent
FIXTURE_ROOT = ROOT / "fixtures" / "aee-landlock-seal"
NEGATIVE_ROOT = FIXTURE_ROOT / "negative-controls"

CASES = [
    "valid-landlock-seal",
    "missing-seal",
    "bad-run-binding",
    "not-still-armed",
    "bad-drop-accounting",
    "uncounted-channel-without-eligible-seal",
    "bad-observed-set",
    "unsupported-observed-attack",
    "substrate-runner-observed-attacks-mismatch",
    "fixture-key-production-scope",
]


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def digest_json(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def pae(payload_type: str, payload: bytes) -> bytes:
    pt = payload_type.encode("utf-8")
    return b"DSSEv1 " + str(len(pt)).encode("ascii") + b" " + pt + b" " + str(len(payload)).encode("ascii") + b" " + payload


def sign(payload: dict[str, Any], keyid: str) -> str:
    return base64.b64encode(hmac.new(SECRET + keyid.encode("utf-8"), pae(PAYLOAD_TYPE, canonical_bytes(payload)), hashlib.sha256).digest()).decode("ascii")


def record(payload: dict[str, Any], seq: int, keyid: str = STRUCTURAL_KEY) -> dict[str, Any]:
    return {
        "payload": payload,
        "payloadType": PAYLOAD_TYPE,
        "seq": seq,
        "signatures": [{"keyid": keyid, "sig": sign(payload, keyid)}],
    }


def leaf_hash(rec: dict[str, Any]) -> str:
    return hashlib.sha256(b"\x00" + pae(rec["payloadType"], canonical_bytes(rec["payload"]))).hexdigest()


def observed_set(records: list[dict[str, Any]]) -> str:
    leaves = sorted({leaf_hash(rec) for rec in records if rec["payload"].get("aeeKind") in {"interception", "examination"}})
    return hashlib.sha256(canonical_bytes(leaves)).hexdigest()


def run_binding_input(statement: dict[str, Any]) -> dict[str, str]:
    env = statement["predicate"]["observationEnvironment"]
    return {
        "aeeBindingVersion": "2",
        "catchPolicy": env["catchPolicy"]["digest"]["sha256"],
        "corpus": env["corpus"]["digest"]["sha256"],
        "networkPosture": digest_json(env["networkPosture"]),
        "observationVocabulary": env["observationVocabulary"]["digest"]["sha256"],
        "runEntropy": env["runEntropy"]["digest"]["sha256"],
        "subject": statement["subject"][0]["digest"]["sha256"],
        "substrate": env["substrate"]["digest"]["sha256"],
    }


def run_binding(statement: dict[str, Any]) -> str:
    return digest_json(run_binding_input(statement))


def base_statement() -> dict[str, Any]:
    subject = {"name": "assay-aee-landlock-seal-fixture-subject", "digest": {"sha256": digest_json({"artifact": "landlock-seal", "v": 1})}}
    substrate_descriptor = {"name": "assay-landlock-fixture-substrate", "collectionPaths": ["landlock-tcp-connect"]}
    corpus_manifest = {"classes": {"NET": ["NET-CONNECT-BLOCK-001"]}, "expectedPayloads": {"NET-CONNECT-BLOCK-001": [digest_json({"probe": "denied-connect", "port": 443})]}}
    catch_policy = {"name": "deny-default-landlock-probe", "scope": "tcp_connect_landlock_port", "allowedPorts": []}
    network_posture = {"mode": "deny-default", "mechanism": "landlock", "scope": "tcp_connect_landlock_port"}
    network_posture["digest"] = {"sha256": digest_json(network_posture)}
    vocab = {"labels": ["connect_blocked", "no_connect_block"], "caught": ["connect_blocked"]}
    vocab["digest"] = {"sha256": digest_json({"caught": vocab["caught"], "labels": vocab["labels"]})}
    stmt = {
        "_type": "https://in-toto.io/Statement/v1",
        "subject": [subject],
        "predicateType": AEE_PREDICATE_TYPE,
        "predicate": {
            "result": "fail",
            "observationEnvironment": {
                "substrate": {"name": substrate_descriptor["name"], "digest": {"sha256": digest_json(substrate_descriptor)}},
                "corpus": {"name": "assay-aee-landlock-seal-corpus", "digest": {"sha256": digest_json(corpus_manifest)}, "manifest": corpus_manifest},
                "catchPolicy": {"name": catch_policy["name"], "digest": {"sha256": digest_json(catch_policy)}},
                "networkPosture": network_posture,
                "observationVocabulary": vocab,
                "runEntropy": {"digest": {"sha256": digest_json({"fixtureRun": "adr-045-landlock-seal", "nonce": "0001"})}},
            },
            "coverage": {"assessedClasses": ["NET"], "outOfScope": {}, "routedElsewhere": {}},
            "attackResults": [{"attackId": "NET-CONNECT-BLOCK-001", "containmentObserved": "connect_blocked", "basis": "substrate", "method": "intercepted", "attribution": "pinned", "actualLayer": "landlock", "observationRefs": [0, 1, 2]}],
            "doesNotAssert": ["production AEE support", "stable AEE export", "complete run population", "agent safety", "provider side effects"],
            "_ext": {"assayLandlockSeal": {"productionPath": False, "trustedKeyScopes": [{"keyid": STRUCTURAL_KEY, "role": "substrate-observation", "collectionPaths": ["landlock-tcp-connect"], "substrate": substrate_descriptor["name"]}]}},
        },
    }
    rb = run_binding(stmt)
    env = stmt["predicate"]["observationEnvironment"]
    interception = {"aeeKind": "interception", "aeeVersion": AEE_VERSION, "aeeRunBinding": rb, "aeeMethod": "intercepted", "aeePayloadCommitment": corpus_manifest["expectedPayloads"]["NET-CONNECT-BLOCK-001"][0], "assayCollectionPath": "landlock-tcp-connect", "assaySourceSchema": "assay.enforcement_health.v1.probe"}
    arming = {"aeeKind": "arming", "aeeVersion": AEE_VERSION, "aeeRunBinding": rb, "aeePostureDigest": env["networkPosture"]["digest"]["sha256"], "assayCollectionPath": "landlock-tcp-connect"}
    records = [record(interception, 1), record(arming, 2)]
    sealed = {"aeeKind": "sealed", "aeeVersion": AEE_VERSION, "aeeRunBinding": rb, "aeeMethod": "intercepted", "aeePostureDigest": env["networkPosture"]["digest"]["sha256"], "aeeStillArmed": True, "aeeDropCount": 0, "aeeDropBound": 0, "assayDropProofModel": "synchronous-probe", "aeeObservedSet": observed_set(records), "aeeObservedAttacks": [], "assayObservedLabels": ["connect_blocked"], "assayCollectionPath": "landlock-tcp-connect", "assaySourceSchema": "assay.enforcement_health.v1", "assaySealScope": "tcp_connect_landlock_port", "assayAttackRowAttributionSource": "assembly-plane", "assayNonClaims": ["does not prove complete run population", "does not prove agent safety", "does not prove provider side effects", "does not prove independent substrate operation"]}
    records.append(record(sealed, 3))
    stmt["predicate"]["observationRecords"] = records
    stmt["predicate"]["batchRoot"] = digest_json([rec["seq"] for rec in records])
    return stmt


def replace_payload(statement: dict[str, Any], idx: int, payload: dict[str, Any], keyid: str = STRUCTURAL_KEY) -> None:
    statement["predicate"]["observationRecords"][idx] = record(payload, idx + 1, keyid)


def case_statement(name: str) -> dict[str, Any]:
    stmt = base_statement()
    pred = stmt["predicate"]
    seal = copy.deepcopy(pred["observationRecords"][2]["payload"])
    if name == "valid-landlock-seal":
        return stmt
    if name == "missing-seal":
        pred["observationRecords"] = pred["observationRecords"][:2]
        pred["attackResults"][0]["observationRefs"] = [0, 1]
    elif name == "bad-run-binding":
        seal["aeeRunBinding"] = "0" * 64
        replace_payload(stmt, 2, seal)
    elif name == "not-still-armed":
        seal["aeeStillArmed"] = False
        replace_payload(stmt, 2, seal)
    elif name == "bad-drop-accounting":
        seal["aeeDropCount"] = 1
        replace_payload(stmt, 2, seal)
    elif name == "uncounted-channel-without-eligible-seal":
        seal["assayDropProofModel"] = "uncounted-queue"
        replace_payload(stmt, 2, seal)
    elif name == "bad-observed-set":
        seal["aeeObservedSet"] = "0" * 64
        replace_payload(stmt, 2, seal)
    elif name == "unsupported-observed-attack":
        seal["aeeObservedAttacks"] = ["NET-CONNECT-BLOCK-999"]
        replace_payload(stmt, 2, seal)
    elif name == "substrate-runner-observed-attacks-mismatch":
        seal["assayAttackRowAttributionSource"] = "substrate-runner"
        seal["aeeObservedAttacks"] = []
        replace_payload(stmt, 2, seal)
    elif name == "fixture-key-production-scope":
        pred["_ext"]["assayLandlockSeal"]["productionPath"] = True
        pred["_ext"]["assayLandlockSeal"]["trustedKeyScopes"] = [{"keyid": FIXTURE_KEY, "role": "substrate-observation", "collectionPaths": ["landlock-tcp-connect"], "substrate": "assay-landlock-fixture-substrate"}]
        for idx, rec in enumerate(list(pred["observationRecords"])):
            replace_payload(stmt, idx, rec["payload"], FIXTURE_KEY)
    else:
        raise ValueError(f"unknown fixture case: {name}")
    return stmt


def fixture_path(name: str) -> Path:
    if name == "valid-landlock-seal":
        return FIXTURE_ROOT / "valid-landlock-seal.json"
    return NEGATIVE_ROOT / f"{name}.json"


def emit_fixtures() -> None:
    for name in CASES:
        path = fixture_path(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(case_statement(name), indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(f"wrote {path}")


def load_case(name: str) -> dict[str, Any]:
    path = fixture_path(name)
    if path.exists():
        with path.open("r", encoding="utf-8") as handle:
            return json.load(handle)
    return case_statement(name)


def validate(statement: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    pred = statement.get("predicate", {})
    env = pred.get("observationEnvironment", {})
    rows = pred.get("attackResults", [])
    records = pred.get("observationRecords", [])
    production_path = pred.get("_ext", {}).get("assayLandlockSeal", {}).get("productionPath") is True
    try:
        rb = run_binding(statement)
    except Exception as exc:  # noqa: BLE001
        errors.append(f"run binding cannot be derived: {exc}")
        rb = None
    payloads = []
    keyids = []
    for idx, rec in enumerate(records):
        payload = rec.get("payload")
        if not isinstance(payload, dict):
            errors.append(f"record {idx} payload is not a JSON object")
            payload = {}
        payloads.append(payload)
        sigs = rec.get("signatures", [])
        if len(sigs) != 1:
            errors.append(f"record {idx} must carry exactly one fixture signature")
            keyids.append("")
            continue
        keyid = sigs[0].get("keyid", "")
        keyids.append(keyid)
        if sigs[0].get("sig") != sign(payload, keyid):
            errors.append(f"record {idx} fixture signature does not verify")
        if production_path and keyid.startswith(FIXTURE_KEY_PREFIX):
            errors.append(f"record {idx} uses fixture key in production path")
        if rb and payload.get("aeeKind") in {"arming", "interception", "examination", "sealed"} and payload.get("aeeRunBinding") != rb:
            errors.append(f"record {idx} aeeRunBinding mismatch")
    seals = [idx for idx, payload in enumerate(payloads) if payload.get("aeeKind") == "sealed"]
    if any(row.get("basis") == "substrate" for row in rows) and not seals:
        errors.append("substrate row lacks required sealed coverage")
    caught = set(env.get("observationVocabulary", {}).get("caught", []))
    caught_attacks = sorted({row.get("attackId") for row in rows if row.get("containmentObserved") in caught})
    for idx in seals:
        seal = payloads[idx]
        if seal.get("aeePostureDigest") != env.get("networkPosture", {}).get("digest", {}).get("sha256"):
            errors.append(f"sealed record {idx} aeePostureDigest mismatch")
        if seal.get("aeeStillArmed") is not True:
            errors.append(f"sealed record {idx} is not still armed")
        if seal.get("aeeDropCount") != 0 or seal.get("aeeDropBound") != 0:
            errors.append(f"sealed record {idx} has non-zero or inconsistent drop accounting")
        if seal.get("assayDropProofModel") not in {"synchronous-probe", "counted-queue-zero"}:
            errors.append(f"sealed record {idx} has no eligible drop-accounting proof model")
        if seal.get("aeeObservedSet") != observed_set(records):
            errors.append(f"sealed record {idx} aeeObservedSet mismatch")
        for attack_id in seal.get("aeeObservedAttacks", []):
            if attack_id not in caught_attacks:
                errors.append(f"sealed record {idx} names attack not supported by caught rows: {attack_id}")
        if seal.get("assayAttackRowAttributionSource") == "substrate-runner" and sorted(seal.get("aeeObservedAttacks", [])) != caught_attacks:
            errors.append(f"sealed record {idx} substrate-runner observed attacks mismatch")
    for row_idx, row in enumerate(rows):
        refs = row.get("observationRefs", [])
        ref_kinds = {payloads[ref].get("aeeKind") for ref in refs if isinstance(ref, int) and 0 <= ref < len(payloads)}
        if row.get("basis") == "substrate" and "arming" not in ref_kinds:
            errors.append(f"substrate row {row_idx} lacks arming coverage")
        if row.get("basis") == "substrate" and "sealed" not in ref_kinds:
            errors.append(f"substrate row {row_idx} lacks sealed coverage")
        if row.get("basis") == "substrate" and row.get("containmentObserved") in caught and "interception" not in ref_kinds:
            errors.append(f"caught substrate row {row_idx} lacks interception coverage")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("case", nargs="?", choices=CASES, help="Named fixture case to check")
    parser.add_argument("--emit", action="store_true", help="Write all named fixtures to scripts/experiments/fixtures/aee-landlock-seal")
    parser.add_argument("--expect-invalid", action="store_true", help="Exit 0 only when validation fails")
    args = parser.parse_args()
    if args.emit:
        emit_fixtures()
        return 0
    if not args.case:
        parser.error("case is required unless --emit is used")
    errors = validate(load_case(args.case))
    if args.expect_invalid:
        if errors:
            print(f"invalid as expected: {args.case}")
            for error in errors:
                print(f"- {error}")
            return 0
        print(f"expected invalid but passed: {args.case}")
        return 1
    if errors:
        print(f"invalid: {args.case}")
        for error in errors:
            print(f"- {error}")
        return 1
    print(f"valid ADR-045 Landlock seal fixture: {args.case}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
