#!/usr/bin/env python3
"""Check the fixture-only Assay -> AEE v0.7 spike statement.

This is not a general AEE verifier. It validates the spike invariants that are
useful for Assay: digest integrity, run binding, batch root, covering-kind
constraints, substrate coverage, and the known negative controls.
"""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any

from aee_spike_lib import (
    AEE_PREDICATE_TYPE,
    COVERING_KINDS,
    FIXTURE_KEY_ID,
    PAYLOAD_TYPE,
    VALID_ATTRIBUTION,
    VALID_BASIS,
    VALID_METHOD,
    decode_record,
    digest_json,
    merkle_root,
    read_json,
    sign_payload,
)

ROOT = Path(__file__).resolve().parent
FIXTURES = ROOT / "fixtures" / "aee"

def recompute_run_binding(statement: dict[str, Any]) -> str:
    subject = statement["subject"][0]
    env = statement["predicate"]["observationEnvironment"]
    network_posture = env["networkPosture"]
    preimage = {
        "aeeBindingVersion": "2",
        "catchPolicy": env["catchPolicy"]["digest"]["sha256"],
        "corpus": env["corpus"]["digest"]["sha256"],
        "networkPosture": digest_json(network_posture),
        "observationVocabulary": env["observationVocabulary"]["digest"]["sha256"],
        "runEntropy": env["runEntropy"]["digest"]["sha256"],
        "subject": subject["digest"]["sha256"],
        "substrate": env["substrate"]["digest"]["sha256"],
    }
    return digest_json(preimage)


def expected_result(predicate: dict[str, Any]) -> str:
    vocab = predicate["observationEnvironment"]["observationVocabulary"]
    labels = set(vocab["labels"])
    caught = set(vocab["caught"])
    rows = predicate["attackResults"]
    if any(
        row.get("containmentObserved") in caught
        or row.get("containmentObserved") not in labels
        or row.get("basis") not in VALID_BASIS
        or row.get("method") not in VALID_METHOD
        or row.get("attribution") not in VALID_ATTRIBUTION
        for row in rows
    ):
        return "fail"
    if predicate["coverage"].get("outOfScope") or predicate["coverage"].get("routedElsewhere"):
        return "degraded"
    if any(row.get("basis") != "substrate" or row.get("method") != "intercepted" for row in rows):
        return "pass_indirect"
    return "pass"


def validate(statement: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if statement.get("_type") != "https://in-toto.io/Statement/v1":
        errors.append("statement _type is not in-toto Statement/v1")
    if statement.get("predicateType") != AEE_PREDICATE_TYPE:
        errors.append("predicateType is not AEE v0.7")
    if len(statement.get("subject", [])) != 1:
        errors.append("statement must carry exactly one subject")

    predicate = statement.get("predicate", {})
    env = predicate.get("observationEnvironment", {})
    corpus = env.get("corpus", {})
    vocab = env.get("observationVocabulary", {})
    records = predicate.get("observationRecords", [])
    rows = predicate.get("attackResults", [])

    if corpus.get("digest", {}).get("sha256") != digest_json(corpus.get("manifest")):
        errors.append("corpus digest does not recompute")
    if vocab.get("digest", {}).get("sha256") != digest_json({"caught": vocab.get("caught"), "labels": vocab.get("labels")}):
        errors.append("observation vocabulary digest does not recompute")
    if records and predicate.get("batchRoot") != merkle_root(records):
        errors.append("batchRoot does not recompute")
    if predicate.get("result") != expected_result(predicate):
        errors.append("result does not recompute")

    run_binding = None
    if any(row.get("basis") == "substrate" for row in rows):
        try:
            run_binding = recompute_run_binding(statement)
        except KeyError as exc:
            errors.append(f"run binding cannot be derived: missing {exc}")

    payloads: list[dict[str, Any]] = []
    for idx, record in enumerate(records):
        try:
            payload, payload_bytes = decode_record(record)
            payloads.append(payload)
        except Exception as exc:  # noqa: BLE001 - checker reports all fixture faults.
            errors.append(f"record {idx} payload cannot be decoded: {exc}")
            payloads.append({})
            continue
        if record.get("payloadType") != PAYLOAD_TYPE:
            errors.append(f"record {idx} payloadType is not the fixture +json type")
        signatures = record.get("signatures", [])
        if not signatures:
            errors.append(f"record {idx} has no fixture signature")
        elif signatures[0].get("keyid") != FIXTURE_KEY_ID or signatures[0].get("sig") != sign_payload(record["payloadType"], payload_bytes):
            errors.append(f"record {idx} fixture signature does not verify")
        if run_binding and payload.get("aeeRunBinding") != run_binding:
            errors.append(f"record {idx} aeeRunBinding mismatch")

    seal_indexes = [idx for idx, payload in enumerate(payloads) if payload.get("aeeKind") == "sealed"]
    if any(row.get("basis") == "substrate" for row in rows) and not seal_indexes:
        errors.append("substrate row lacks required sealed coverage")

    for idx, payload in enumerate(payloads):
        kind = payload.get("aeeKind")
        if kind in COVERING_KINDS:
            if kind == "sealed":
                if payload.get("aeeStillArmed") is not True:
                    errors.append(f"sealed record {idx} is not still armed")
                if payload.get("aeeDropCount") != 0 or payload.get("aeeDropBound") != 0:
                    errors.append(f"sealed record {idx} has non-zero or inconsistent drop accounting")
                observed_set = sorted({row.get("containmentObserved") for row in rows if row.get("containmentObserved") in set(vocab.get("caught", []))})
                if payload.get("aeeObservedSet") != observed_set:
                    errors.append(f"sealed record {idx} aeeObservedSet mismatch")
            if payload.get("aeeMethod") not in VALID_METHOD and kind != "arming":
                errors.append(f"record {idx} has invalid aeeMethod")

    referenced_interceptions: set[int] = set()
    for row_idx, row in enumerate(rows):
        refs = row.get("observationRefs", [])
        if row.get("basis") == "substrate" and not refs:
            errors.append(f"substrate row {row_idx} has empty observationRefs")
        for ref in refs:
            if not isinstance(ref, int) or ref < 0 or ref >= len(records):
                errors.append(f"row {row_idx} has out-of-range observationRefs index {ref}")
        ref_payloads = [payloads[ref] for ref in refs if isinstance(ref, int) and 0 <= ref < len(payloads)]
        ref_kinds = {payload.get("aeeKind") for payload in ref_payloads}
        if row.get("basis") == "substrate":
            if row.get("containmentObserved") in set(vocab.get("caught", [])) and row.get("method") == "intercepted" and "interception" not in ref_kinds:
                errors.append(f"caught substrate row {row_idx} lacks interception coverage")
            if "arming" not in ref_kinds:
                errors.append(f"substrate row {row_idx} lacks arming coverage")
            if "sealed" not in ref_kinds:
                errors.append(f"substrate row {row_idx} lacks sealed coverage")
            methods = [payload.get("aeeMethod") for payload in ref_payloads if payload.get("aeeKind") in {"interception", "sealed", "examination"}]
            if row.get("method") == "intercepted" and "reconstructed" in methods:
                errors.append(f"row {row_idx} claims intercepted but covering record is reconstructed")
        if row.get("attribution") == "pinned":
            expected = corpus.get("manifest", {}).get("expectedPayloads", {}).get(row.get("attackId"), [])
            interception_payloads = [payload for payload in ref_payloads if payload.get("aeeKind") == "interception"]
            if not interception_payloads:
                errors.append(f"pinned row {row_idx} lacks interception record")
            elif not any(payload.get("aeePayloadCommitment") in expected for payload in interception_payloads):
                errors.append(f"pinned row {row_idx} payload commitment not in corpus expectedPayloads")
            for ref in refs:
                if 0 <= ref < len(payloads) and payloads[ref].get("aeeKind") == "interception":
                    referenced_interceptions.add(ref)

    for idx, payload in enumerate(payloads):
        if payload.get("aeeKind") == "interception" and idx not in referenced_interceptions:
            errors.append(f"interception record {idx} is not resolved by a caught row")

    invalid_claim = predicate.get("_ext", {}).get("assaySpike", {}).get("invalidClaim")
    if invalid_claim == "no sibling runs existed":
        errors.append("run population overclaim: AEE/Assay spike must not claim no sibling runs existed")
    elif invalid_claim == "artifact-produced observation overclaimed as substrate":
        errors.append("artifact/proxy observation overclaim: substrate basis requires substrate coverage")

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("statement", type=Path, help="AEE spike statement JSON to check")
    parser.add_argument("--expect-invalid", action="store_true", help="Exit 0 only when validation fails")
    args = parser.parse_args()

    errors = validate(read_json(args.statement, base_dir=FIXTURES))
    if args.expect_invalid:
        if errors:
            print(f"invalid as expected: {args.statement}")
            for error in errors:
                print(f"- {error}")
            return 0
        print(f"expected invalid but passed: {args.statement}")
        return 1
    if errors:
        print(f"invalid: {args.statement}")
        for error in errors:
            print(f"- {error}")
        return 1
    print(f"valid fixture statement: {args.statement}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
