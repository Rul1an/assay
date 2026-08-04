#!/usr/bin/env python3
"""Emit a fixture-only Assay -> AEE v0.7 spike statement.

This script intentionally lives outside production crates. It tests the AEE
field shape against Assay evidence carriers and makes the missing production
primitive explicit: Assay can synthesize this fixture seal, but current Assay
carriers do not emit an AEE-compatible substrate-signed sealed record.
"""

from __future__ import annotations

import argparse
import copy
from typing import Any

from aee_spike_lib import (
    AEE_PR_HEAD,
    AEE_SPEC_SHA256,
    AEE_PREDICATE_TYPE,
    AEE_VERSION,
    decode_payload,
    digest_json,
    merkle_root,
    observation_record,
    read_source_fixture,
    run_binding_from_statement,
    run_binding_input_from_statement,
    write_statement_fixture,
)

VARIANT_NAMES = [
    "missing-seal",
    "defective-unreferenced-seal",
    "artifact-labelled-substrate",
    "reconstructed-priced-intercepted",
    "run-population-overclaim",
]


def build_statement() -> dict[str, Any]:
    corpus_manifest = read_source_fixture("corpus-manifest")
    substrate_descriptor = read_source_fixture("substrate-descriptor")
    catch_policy = read_source_fixture("catch-policy")
    proxy_observation = read_source_fixture("proxy-deny-observation")
    enforcement_health = read_source_fixture("enforcement-health-v1-active-probe")

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
        },
    }
    run_binding_input = run_binding_input_from_statement(statement)
    run_binding = run_binding_from_statement(statement)

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

    statement["predicate"].update(
        {
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
        }
    )
    return statement


def write_variant(statement: dict[str, Any], variant: str) -> None:
    mutated = copy.deepcopy(statement)
    predicate = mutated["predicate"]
    if variant == "missing-seal":
        # Original records are [proxy, arming, sealed, network]. Drop only the
        # seal and reindex the network interception so this negative control
        # isolates the missing-seal failure instead of also creating an
        # out-of-range reference.
        predicate["observationRecords"] = [
            predicate["observationRecords"][0],
            predicate["observationRecords"][1],
            predicate["observationRecords"][3],
        ]
        predicate["attackResults"][0]["observationRefs"] = [0, 1]
        predicate["attackResults"][1]["observationRefs"] = [2, 1]
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
        # The valid fixture row is already substrate-backed. To exercise the
        # intended overclaim boundary, strip its substrate coverage and mark the
        # producer-side claim being attempted: an artifact/proxy-produced
        # observation being priced as substrate evidence.
        predicate["attackResults"][0]["basis"] = "substrate"
        predicate["attackResults"][0]["method"] = "intercepted"
        predicate["attackResults"][0]["observationRefs"] = []
        predicate["_ext"]["assaySpike"]["invalidClaim"] = "artifact-produced observation overclaimed as substrate"
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
    write_statement_fixture(variant, mutated)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--variants", action="store_true", help="Also emit intentionally invalid negative-control statements")
    args = parser.parse_args()

    statement = build_statement()
    valid_path = write_statement_fixture("valid", statement)

    if args.variants:
        for name in VARIANT_NAMES:
            write_variant(statement, name)

    print(f"wrote {valid_path}")
    if args.variants:
        print("wrote negative controls under scripts/experiments/fixtures/aee/negative-controls")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
