#!/usr/bin/env python3
"""ADR-045 Landlock run-end seal fixture/checker harness.

This is the fixture/checker slice for issue #1998, hardened in #2006: fixtures
and checker semantics before producer code. It deliberately does not add a
production AEE exporter or production signing primitive.

Three outcomes stay distinguishable, because ADR-043's rule that integrity never
upgrades meaning only exists if a consumer can tell them apart:

  malformed                        not structurally valid
  structurally-valid-not-credited  signature verifies, key is untrusted, out of
                                   scope, wrong role, or outside its window
  credited                         structurally valid and trusted for this scope

Fixture policy (#2006 item 1): the positive fixture is a full on-disk artifact,
because it is what a producer gets checked against and the bytes it commits to
are the signing surface. Negative controls are marker-only, because each is
defined by the single field it breaks and the marker already names it.
"""

from __future__ import annotations

import argparse
import base64
import copy
import hashlib
import hmac
import json
import re
from datetime import datetime
from pathlib import Path
from typing import Any, NamedTuple

AEE_PREDICATE_TYPE = "https://in-toto.io/attestation/adversarial-execution-evidence/v0.7"
AEE_VERSION = "0.7"
PAYLOAD_TYPE = "application/vnd.assay.aee-landlock-seal.fixture.v0+json"
STRUCTURAL_KEY = "assay-test-observation-key-landlock-v0"
FIXTURE_KEY = "assay-aee-spike-fixture-key-v0"
FIXTURE_KEY_PREFIX = "assay-aee-spike-fixture-key"
UNTRUSTED_KEY = "assay-test-observation-key-landlock-unenrolled-v0"
SECRET = b"assay-aee-landlock-seal-fixture-key-not-production"
ROOT = Path(__file__).resolve().parent
# No override. An earlier version read the output root from an environment variable so the drift
# check could emit elsewhere; CodeQL flagged it as an arbitrary-write primitive and was right, since
# the value reached `write_text` in a script that runs from a pre-push hook. Validating it still left
# tainted data flowing into a path -- a containment comparison is not a barrier the dataflow
# recognises, and CodeQL flagged the validator too. So the capability is gone rather than guarded:
# the drift check copies this directory to a scratch tree and runs the copy, where `ROOT` resolves
# under the copy by itself.
FIXTURE_ROOT = ROOT / "fixtures" / "aee-landlock-seal"
NEGATIVE_ROOT = FIXTURE_ROOT / "negative-controls"

SUBSTRATE_NAME = "assay-landlock-fixture-substrate"
COLLECTION_PATH = "landlock-tcp-connect"
OBSERVATION_ROLE = "substrate-observation"
SOURCE_SCHEMA = "assay.enforcement_health.v1"

# The one bounded enforcement scope this slice implements, per the #2001 field
# contract. It is both what the fixture builds and what the checker requires,
# from one constant: a seal that names a different scope is one this checker has
# no rules for, and crediting it would report coverage of a boundary nothing
# here observed.
SEAL_SCOPE = "tcp_connect_landlock_port"

# The remaining payload-only constants from the #2001 field contract. They were declared in the
# sketch and unimplemented here (#2060), found by mutating one field at a time and watching the
# checker credit the result.
SEAL_METHOD = "intercepted"
ATTRIBUTION_SOURCES = ("assembly-plane", "substrate-runner")

# `assayNonClaims` MUST include at least these. A producer that drops one is claiming more than the
# seal supports, which is the failure the field exists to prevent, so a subset is a rejection rather
# than a warning.
SEAL_DIGEST_FIELDS = ("aeeRunBinding", "aeePostureDigest", "aeeObservedSet")

MINIMUM_NON_CLAIMS = (
    "does not prove complete run population",
    "does not prove agent safety",
    "does not prove provider side effects",
    "does not prove independent substrate operation",
)

# Every field the contract marks required. `_test_every_required_field_is_checked` removes each in
# turn and requires the checker to reject, which is how "required field present" is enforced here:
# each field is covered by the rule that owns it, and this list is what proves none was forgotten.
# A generic presence rule would fire alongside those and stop every control isolating its reason.
REQUIRED_SEAL_FIELDS = (
    "aeeKind",
    "aeeVersion",
    "aeeRunBinding",
    "aeeMethod",
    "aeePostureDigest",
    "aeeStillArmed",
    "aeeDropCount",
    "aeeDropBound",
    "assayDropProofModel",
    "aeeObservedSet",
    "aeeObservedAttacks",
    "assayCollectionPath",
    "assaySealedAt",
    "assaySourceSchema",
    "assaySealScope",
    "assayAttackRowAttributionSource",
    "assayNonClaims",
)

# Fixed instants. A validity window needs something to be checked against, and a
# wall clock would make the drift check flaky, which is how a gate gets disabled.
SEALED_AT = "2026-08-05T00:00:00Z"
KEY_VALID_FROM = "2026-01-01T00:00:00Z"
KEY_VALID_UNTIL = "2027-01-01T00:00:00Z"
KEY_EXPIRED_UNTIL = "2026-08-04T00:00:00Z"

SIGNED_KINDS = {"arming", "interception", "examination", "sealed"}

PHASE_MALFORMED = "malformed"
PHASE_NOT_CREDITED = "not-credited"

OUTCOME_MALFORMED = "malformed"
OUTCOME_NOT_CREDITED = "structurally-valid-not-credited"
OUTCOME_CREDITED = "credited"


class Finding(NamedTuple):
    """A rejection with a stable reason code.

    The code is what a negative control asserts. A control that only asserts a
    non-zero exit reports coverage it does not have, because it cannot tell
    "rejected for the reason I built" from "rejected because I broke the JSON".
    """

    code: str
    phase: str
    message: str


POSITIVE_CASE = "valid-landlock-seal"

# Each negative control names the one reason code it exists to produce. The
# meta-test disables exactly that code and asserts the control then passes; if it
# still fails, the control was failing for some other reason.
NEGATIVE_CONTROLS: dict[str, str] = {
    "missing-seal": "substrate-row-missing-sealed-coverage",
    "bad-run-binding": "run-binding-mismatch",
    "not-still-armed": "seal-not-still-armed",
    "bad-drop-accounting": "drop-accounting-nonzero",
    "uncounted-channel-without-eligible-seal": "drop-proof-model-ineligible",
    "bad-observed-set": "observed-set-mismatch",
    "unsupported-observed-attack": "observed-attack-unsupported",
    "substrate-runner-observed-attacks-mismatch": "substrate-runner-observed-attacks-mismatch",
    "fixture-key-production-scope": "fixture-key-in-production-path",
    "untrusted-signing-key": "untrusted-signing-key",
    "wrong-key-role": "wrong-key-role",
    "key-scope-collection-path-mismatch": "key-scope-collection-path-mismatch",
    "key-scope-substrate-mismatch": "key-scope-substrate-mismatch",
    "key-outside-validity-window": "key-outside-validity-window",
    "unsupported-envelope-shape": "unsupported-envelope-shape",
    "posture-digest-is-run-binding-input": "posture-digest-mismatch",
    "seal-scope-absent": "seal-scope-missing",
    "seal-scope-other-boundary": "seal-scope-mismatch",
    "unsupported-aee-version": "payload-aee-version-unsupported",
    "unsupported-seal-method": "payload-method-unsupported",
    "unknown-attribution-source": "payload-attribution-source-unknown",
    "incomplete-non-claims": "payload-non-claims-incomplete",
    "seal-collection-path-other": "payload-collection-path-mismatch",
    "digest-field-not-a-digest": "payload-digest-shape-invalid",
    "row-missing-arming-coverage": "substrate-row-missing-arming-coverage",
    "row-missing-interception-coverage": "substrate-row-missing-interception-coverage",
    "payload-not-an-object": "payload-not-object",
    "run-binding-underivable": "run-binding-underivable",
    "two-signatures-on-one-record": "signature-count",
    "corrupt-signature": "signature-invalid",
    "seal-instant-not-rfc3339": "seal-instant-invalid",
    "empty-source-schema": "payload-source-schema-invalid",
    "observed-attacks-not-a-list": "payload-observed-attacks-invalid",
}

CASES = [POSITIVE_CASE, *NEGATIVE_CONTROLS]


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def digest_json(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def pae(payload_type: str, payload: bytes) -> bytes:
    pt = payload_type.encode("utf-8")
    return b"DSSEv1 " + str(len(pt)).encode("ascii") + b" " + pt + b" " + str(len(payload)).encode("ascii") + b" " + payload


def sign(payload: dict[str, Any], keyid: str, payload_type: str = PAYLOAD_TYPE) -> str:
    return base64.b64encode(hmac.new(SECRET + keyid.encode("utf-8"), pae(payload_type, canonical_bytes(payload)), hashlib.sha256).digest()).decode("ascii")


def record(payload: dict[str, Any], seq: int, keyid: str = STRUCTURAL_KEY, payload_type: str = PAYLOAD_TYPE) -> dict[str, Any]:
    return {
        "payload": payload,
        "payloadType": payload_type,
        "seq": seq,
        "signatures": [{"keyid": keyid, "sig": sign(payload, keyid, payload_type)}],
    }


def leaf_hash(rec: dict[str, Any]) -> str:
    return hashlib.sha256(b"\x00" + pae(rec["payloadType"], canonical_bytes(rec["payload"]))).hexdigest()


def observed_set(records: list[dict[str, Any]]) -> str:
    # A non-object payload contributes no leaf rather than raising. `validate` already reports it as
    # `payload-not-object` and then keeps going, so a raise here made that rule unreachable: the
    # checker crashed before it could return the finding it had just recorded. Two representations
    # of one record -- `validate`'s normalized `payloads` and the raw `records` this walks -- is what
    # let them disagree, and the guard is on this side because the emitter calls it too.
    leaves = sorted(
        {
            leaf_hash(rec)
            for rec in records
            if isinstance(rec.get("payload"), dict)
            and rec["payload"].get("aeeKind") in {"interception", "examination"}
        }
    )
    return hashlib.sha256(canonical_bytes(leaves)).hexdigest()


def is_sha256_hex(value: Any) -> bool:
    """Lowercase SHA-256 hex, as the field contract requires. Uppercase is a different string and
    therefore a different digest to every consumer that compares bytes."""
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def parse_instant(value: Any) -> datetime | None:
    if not isinstance(value, str):
        return None
    try:
        return datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ")
    except ValueError:
        return None


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


def trusted_scope(keyid: str, **overrides: Any) -> dict[str, Any]:
    scope = {
        "keyid": keyid,
        "role": OBSERVATION_ROLE,
        "collectionPaths": [COLLECTION_PATH],
        "substrate": SUBSTRATE_NAME,
        "validFrom": KEY_VALID_FROM,
        "validUntil": KEY_VALID_UNTIL,
    }
    scope.update(overrides)
    return scope


def base_statement() -> dict[str, Any]:
    subject = {"name": "assay-aee-landlock-seal-fixture-subject", "digest": {"sha256": digest_json({"artifact": "landlock-seal", "v": 1})}}
    substrate_descriptor = {"name": SUBSTRATE_NAME, "collectionPaths": [COLLECTION_PATH]}
    corpus_manifest = {"classes": {"NET": ["NET-CONNECT-BLOCK-001"]}, "expectedPayloads": {"NET-CONNECT-BLOCK-001": [digest_json({"probe": "denied-connect", "port": 443})]}}
    catch_policy = {"name": "deny-default-landlock-probe", "scope": SEAL_SCOPE, "allowedPorts": []}
    network_posture = {"mode": "deny-default", "mechanism": "landlock", "scope": SEAL_SCOPE}
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
            "_ext": {"assayLandlockSeal": {"productionPath": False, "trustedKeyScopes": [trusted_scope(STRUCTURAL_KEY)]}},
        },
    }
    rb = run_binding(stmt)
    env = stmt["predicate"]["observationEnvironment"]
    interception = {"aeeKind": "interception", "aeeVersion": AEE_VERSION, "aeeRunBinding": rb, "aeeMethod": "intercepted", "aeePayloadCommitment": corpus_manifest["expectedPayloads"]["NET-CONNECT-BLOCK-001"][0], "assayCollectionPath": COLLECTION_PATH, "assaySourceSchema": "assay.enforcement_health.v1.probe"}
    arming = {"aeeKind": "arming", "aeeVersion": AEE_VERSION, "aeeRunBinding": rb, "aeePostureDigest": env["networkPosture"]["digest"]["sha256"], "assayCollectionPath": COLLECTION_PATH}
    records = [record(interception, 1), record(arming, 2)]
    sealed = {"aeeKind": "sealed", "aeeVersion": AEE_VERSION, "aeeRunBinding": rb, "aeeMethod": "intercepted", "aeePostureDigest": env["networkPosture"]["digest"]["sha256"], "aeeStillArmed": True, "aeeDropCount": 0, "aeeDropBound": 0, "assayDropProofModel": "synchronous-probe", "aeeObservedSet": observed_set(records), "aeeObservedAttacks": [], "assayObservedLabels": ["connect_blocked"], "assayCollectionPath": COLLECTION_PATH, "assaySealedAt": SEALED_AT, "assaySourceSchema": SOURCE_SCHEMA, "assaySealScope": SEAL_SCOPE, "assayAttackRowAttributionSource": "assembly-plane", "assayNonClaims": ["does not prove complete run population", "does not prove agent safety", "does not prove provider side effects", "does not prove independent substrate operation"]}
    records.append(record(sealed, 3))
    stmt["predicate"]["observationRecords"] = records
    return stmt


def replace_payload(statement: dict[str, Any], idx: int, payload: dict[str, Any], keyid: str = STRUCTURAL_KEY, payload_type: str = PAYLOAD_TYPE) -> None:
    statement["predicate"]["observationRecords"][idx] = record(payload, idx + 1, keyid, payload_type)


def set_scopes(statement: dict[str, Any], scopes: list[dict[str, Any]]) -> None:
    statement["predicate"]["_ext"]["assayLandlockSeal"]["trustedKeyScopes"] = scopes


def case_statement(name: str) -> dict[str, Any]:
    stmt = base_statement()
    pred = stmt["predicate"]
    seal = copy.deepcopy(pred["observationRecords"][2]["payload"])
    if name == POSITIVE_CASE:
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
        set_scopes(stmt, [trusted_scope(FIXTURE_KEY)])
        for idx, rec in enumerate(list(pred["observationRecords"])):
            replace_payload(stmt, idx, rec["payload"], FIXTURE_KEY)
    elif name == "untrusted-signing-key":
        # Signature verifies. The key is simply not in the consumer trust set, so
        # no scope is found and every scope-dependent check stays silent.
        for idx, rec in enumerate(list(pred["observationRecords"])):
            replace_payload(stmt, idx, rec["payload"], UNTRUSTED_KEY)
    elif name == "wrong-key-role":
        set_scopes(stmt, [trusted_scope(STRUCTURAL_KEY, role="policy-decision")])
    elif name == "key-scope-collection-path-mismatch":
        set_scopes(stmt, [trusted_scope(STRUCTURAL_KEY, collectionPaths=["landlock-udp-send"])])
    elif name == "key-scope-substrate-mismatch":
        set_scopes(stmt, [trusted_scope(STRUCTURAL_KEY, substrate="assay-some-other-substrate")])
    elif name == "key-outside-validity-window":
        set_scopes(stmt, [trusted_scope(STRUCTURAL_KEY, validUntil=KEY_EXPIRED_UNTIL)])
    elif name == "posture-digest-is-run-binding-input":
        # ADR-045 line 476 requires this control by name. `aeePostureDigest` must equal the carried
        # `networkPosture.digest.sha256`; the run-binding input is the digest of the whole carried
        # object, which -- because the digest member is inserted after that member is computed -- is
        # a strictly larger object and a different value. Two plausible readings, one correct, and
        # the ADR names the confusion rather than leaving it to be discovered.
        seal["aeePostureDigest"] = digest_json(pred["observationEnvironment"]["networkPosture"])
        replace_payload(stmt, 2, seal)
    elif name == "seal-scope-absent":
        # The member simply is not there. Separated from the wrong-value control
        # because an absent scope and a wrong one fail differently: one producer
        # never wrote the field, the other wrote a boundary it did not observe.
        del seal["assaySealScope"]
        replace_payload(stmt, 2, seal)
    elif name == "seal-scope-other-boundary":
        # A well-formed scope name for an enforcement boundary this slice has no
        # rules for. Everything else in the statement still describes a Landlock
        # TCP-connect observation, which is the point: the seal claims a reach
        # the observation records do not cover.
        seal["assaySealScope"] = "filesystem_write_all"
        replace_payload(stmt, 2, seal)
    elif name == "unsupported-aee-version":
        # A seal from a later slice, read by a checker with this slice's rules. Accepting it would
        # mean applying 0.7 rules to a payload that does not claim to be 0.7.
        seal["aeeVersion"] = "0.8"
        replace_payload(stmt, 2, seal)
    elif name == "unsupported-seal-method":
        # `inferred` is a method this slice has no rules for. The seal would still carry every other
        # field, which is what makes it worth refusing by name.
        seal["aeeMethod"] = "inferred"
        replace_payload(stmt, 2, seal)
    elif name == "unknown-attribution-source":
        # Neither assembly-plane nor substrate-runner. The substrate-runner equality rule is keyed on
        # that exact string, so an unknown value slips past it and attributes nothing.
        seal["assayAttackRowAttributionSource"] = "hearsay"
        replace_payload(stmt, 2, seal)
    elif name == "incomplete-non-claims":
        # One non-claim dropped. The seal then reads as making a claim it never supported.
        seal["assayNonClaims"] = [c for c in seal["assayNonClaims"] if c != MINIMUM_NON_CLAIMS[3]]
        replace_payload(stmt, 2, seal)
    elif name == "seal-collection-path-other":
        # The key scope is widened to trust both paths, so `key-scope-collection-path-mismatch`
        # stays silent and this control isolates the payload rule. Without that, the two would fire
        # together and neither would be shown to work on its own -- which is precisely why the
        # trust-scope rule catching this today is not the same as a payload rule existing.
        set_scopes(stmt, [trusted_scope(STRUCTURAL_KEY, collectionPaths=[COLLECTION_PATH, "landlock-udp-send"])])
        seal["assayCollectionPath"] = "landlock-udp-send"
        replace_payload(stmt, 2, seal)
    elif name == "digest-field-not-a-digest":
        # Uppercase hex of the right length. Well-formed enough to survive a careless check and not
        # a SHA-256 digest, since every consumer that compares bytes sees a different string.
        seal["aeePostureDigest"] = seal["aeePostureDigest"].upper()
        replace_payload(stmt, 2, seal)
    elif name == "row-missing-arming-coverage":
        # The arming record is still emitted and still signed; the row simply stops referencing it.
        # That is the "defective unreferenced covering-kind record" #1998 requires be rejected: the
        # evidence exists and the row does not claim it, so nothing binds the row to armed state.
        pred["attackResults"][0]["observationRefs"] = [0, 2]
    elif name == "row-missing-interception-coverage":
        pred["attackResults"][0]["observationRefs"] = [1, 2]
    elif name == "payload-not-an-object":
        # A scalar where a payload object belongs, on a *fourth* record that no attack row
        # references. Corrupting one of the three carried records would also break the observed set
        # and the row's coverage, so the control would prove three rules at once and none of them on
        # its own. An unreferenced malformed record is also the realistic shape: a producer emitting
        # something the assembler did not expect.
        #
        # The signature is recomputed over the empty object the checker substitutes for a non-object
        # payload, so this isolates the shape rejection rather than also tripping
        # `signature-invalid`.
        pred["observationRecords"].append({
            "payload": "not-an-object",
            "payloadType": PAYLOAD_TYPE,
            "seq": 4,
            "signatures": [{"keyid": STRUCTURAL_KEY, "sig": sign({}, STRUCTURAL_KEY)}],
        })
    elif name == "run-binding-underivable":
        # The derivation input is gone, so the binding cannot be computed at all. The checker must
        # say that rather than report a mismatch against a value it never derived.
        del stmt["subject"][0]["digest"]
    elif name == "two-signatures-on-one-record":
        rec = pred["observationRecords"][0]
        rec["signatures"] = [rec["signatures"][0], dict(rec["signatures"][0])]
    elif name == "corrupt-signature":
        # Valid base64 of the right length, signed over nothing. The envelope is well-formed and the
        # signature does not verify, which is a different finding from a malformed envelope.
        pred["observationRecords"][0]["signatures"][0]["sig"] = base64.b64encode(b"\x00" * 32).decode("ascii")
    elif name == "seal-instant-not-rfc3339":
        seal["assaySealedAt"] = "yesterday"
        replace_payload(stmt, 2, seal)
    elif name == "empty-source-schema":
        seal["assaySourceSchema"] = ""
        replace_payload(stmt, 2, seal)
    elif name == "observed-attacks-not-a-list":
        seal["aeeObservedAttacks"] = "NET-CONNECT-BLOCK-001"
        replace_payload(stmt, 2, seal)
    elif name == "unsupported-envelope-shape":
        # Self-consistent envelope, signed over its own declared payload type.
        # Nothing here is malformed; the checker simply does not implement this
        # shape, and must say so rather than pass it through.
        replace_payload(stmt, 2, seal, STRUCTURAL_KEY, "application/vnd.assay.aee-landlock-seal.unimplemented.v9+json")
    else:
        raise ValueError(f"unknown fixture case: {name}")
    return stmt


# Every fixture path this harness will ever touch, built once from the case list
# and looked up by name. Nothing joins a caller-supplied string onto a directory.
#
# `argparse(choices=CASES)` already constrains the CLI, but `fixture_path` and
# `load_case` are library-shaped: a second caller would not come through argparse,
# and a path boundary that holds only because of a framework detail stops holding
# the moment someone adds one.
FIXTURE_PATHS: dict[str, Path] = {
    name: (FIXTURE_ROOT if name == POSITIVE_CASE else NEGATIVE_ROOT) / f"{name}.json" for name in CASES
}


def fixture_path(name: str) -> Path:
    try:
        return FIXTURE_PATHS[name]
    except KeyError:
        raise SystemExit(f"unknown fixture case: {name}") from None


def marker_body(name: str) -> dict[str, Any]:
    return {
        "_type": "https://in-toto.io/Statement/v1",
        "case": name,
        "note": "Marker fixture for a negative control. The control is defined by the single field it breaks, which this name states; the body is generated by aee_landlock_seal_fixture.py so emitter and checker cannot drift.",
        "predicateType": AEE_PREDICATE_TYPE,
        "rejectsWith": NEGATIVE_CONTROLS[name],
    }


PARITY_PATH = FIXTURE_ROOT / "derivation-parity.json"


def emit_parity_vectors() -> None:
    """Emit the derivation inputs and their expected digests, for a second implementation.

    The producer in `crates/assay-cli/src/aee_seal.rs` derives `aeeRunBinding` and `aeeObservedSet`
    itself. Two implementations of one derivation drift, and a Rust test holding a transcribed
    literal cannot notice when this side moves. So the values live in one committed artifact that
    this emitter writes and the Rust side reads: a change here shows up as a diff, and a divergence
    shows up as a failing test rather than as two seals nobody can reconcile.
    """
    stmt = base_statement()
    env = stmt["predicate"]["observationEnvironment"]
    records = [r for r in stmt["predicate"]["observationRecords"] if r["payload"].get("aeeKind") in {"interception", "examination"}]
    body = {
        "note": "Generated by aee_landlock_seal_fixture.py --emit-parity. Do not hand-edit.",
        "environment": {
            "subject": stmt["subject"][0]["digest"]["sha256"],
            "substrate": env["substrate"]["digest"]["sha256"],
            "corpus": env["corpus"]["digest"]["sha256"],
            "catchPolicy": env["catchPolicy"]["digest"]["sha256"],
            "observationVocabulary": env["observationVocabulary"]["digest"]["sha256"],
            "runEntropy": env["runEntropy"]["digest"]["sha256"],
            "networkPosture": env["networkPosture"],
        },
        "records": [{"payload": r["payload"], "payloadType": r["payloadType"]} for r in records],
        # Two leaves whose emission order is deliberately not their sorted order. With a single
        # leaf, sorting is a no-op and a second implementation that omits it still matches -- which
        # is exactly the divergence the spec's "sorted ascending by UTF-16 code unit" exists to
        # prevent. This vector is what makes the sort observable.
        "orderingRecords": [
            {"payload": {"aeeKind": "interception", "aeeVersion": AEE_VERSION, "assayOrderProbe": "zzz"}, "payloadType": PAYLOAD_TYPE},
            {"payload": {"aeeKind": "examination", "aeeVersion": AEE_VERSION, "assayOrderProbe": "aaa"}, "payloadType": PAYLOAD_TYPE},
        ],
        "expected": {
            "runBinding": run_binding(stmt),
            "networkPostureDigest": env["networkPosture"]["digest"]["sha256"],
            "observedSet": observed_set(stmt["predicate"]["observationRecords"]),
            "orderingObservedSet": observed_set([
                {"payload": {"aeeKind": "interception", "aeeVersion": AEE_VERSION, "assayOrderProbe": "zzz"}, "payloadType": PAYLOAD_TYPE},
                {"payload": {"aeeKind": "examination", "aeeVersion": AEE_VERSION, "assayOrderProbe": "aaa"}, "payloadType": PAYLOAD_TYPE},
            ]),
        },
    }
    PARITY_PATH.parent.mkdir(parents=True, exist_ok=True)
    PARITY_PATH.write_text(json.dumps(body, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {PARITY_PATH}")


def emit_fixtures() -> None:
    """Write fixtures per the #2006 policy: full body positive, markers negative."""
    for name in CASES:
        path = fixture_path(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        body = case_statement(name) if name == POSITIVE_CASE else marker_body(name)
        path.write_text(json.dumps(body, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(f"wrote {path}")


def load_case(name: str) -> dict[str, Any]:
    """Load the positive fixture from disk; generate the negative controls.

    The positive fixture is what a producer gets checked against, so the bytes on
    disk are the artifact under review and are read back rather than regenerated.
    Negative controls are generated from their marker so a control and the check
    it targets cannot drift apart in a hand-edited file.
    """
    if name != POSITIVE_CASE:
        return case_statement(name)
    path = fixture_path(name)
    if not path.is_file():
        raise SystemExit(f"missing positive fixture {path}; run --emit to materialize it")
    body = json.loads(path.read_text(encoding="utf-8"))
    if "predicate" not in body:
        raise SystemExit(f"positive fixture {path} is a marker, not a full artifact; run --emit")
    return body


def validate(statement: dict[str, Any], disabled: frozenset[str] = frozenset()) -> list[Finding]:
    findings: list[Finding] = []

    def add(code: str, phase: str, message: str) -> None:
        if code not in disabled:
            findings.append(Finding(code, phase, message))

    pred = statement.get("predicate", {})
    env = pred.get("observationEnvironment", {})
    rows = pred.get("attackResults", [])
    records = pred.get("observationRecords", [])
    ext = pred.get("_ext", {}).get("assayLandlockSeal", {})
    production_path = ext.get("productionPath") is True
    scopes = {scope.get("keyid"): scope for scope in ext.get("trustedKeyScopes", []) if isinstance(scope, dict)}
    substrate_name = env.get("substrate", {}).get("name")

    try:
        rb = run_binding(statement)
    except Exception as exc:  # noqa: BLE001
        add("run-binding-underivable", PHASE_MALFORMED, f"run binding cannot be derived: {exc}")
        rb = None

    payloads: list[dict[str, Any]] = []
    credit_eligible: list[int] = []
    for idx, rec in enumerate(records):
        payload = rec.get("payload")
        if not isinstance(payload, dict):
            add("payload-not-object", PHASE_MALFORMED, f"record {idx} payload is not a JSON object")
            payload = {}
        payloads.append(payload)

        payload_type = rec.get("payloadType")
        if payload_type != PAYLOAD_TYPE and "unsupported-envelope-shape" not in disabled:
            # Reject as unsupported, never skip. A "we did not check this" path
            # that returns success is how an unverified record reads as verified.
            add("unsupported-envelope-shape", PHASE_MALFORMED, f"record {idx} payload type {payload_type!r} is not implemented by this checker")
            continue

        sigs = rec.get("signatures", [])
        if len(sigs) != 1:
            add("signature-count", PHASE_MALFORMED, f"record {idx} must carry exactly one fixture signature")
            continue
        keyid = sigs[0].get("keyid", "")
        if sigs[0].get("sig") != sign(payload, keyid, payload_type if isinstance(payload_type, str) else PAYLOAD_TYPE):
            add("signature-invalid", PHASE_MALFORMED, f"record {idx} fixture signature does not verify")
            continue
        if rb and payload.get("aeeKind") in SIGNED_KINDS and payload.get("aeeRunBinding") != rb:
            add("run-binding-mismatch", PHASE_MALFORMED, f"record {idx} aeeRunBinding mismatch")

        # Credited-evidence phase. The signature is an envelope fact; whether the
        # record counts as attested substrate evidence is a separate question.
        if production_path and keyid.startswith(FIXTURE_KEY_PREFIX):
            add("fixture-key-in-production-path", PHASE_NOT_CREDITED, f"record {idx} uses fixture key in production path")
        scope = scopes.get(keyid)
        if scope is None:
            add("untrusted-signing-key", PHASE_NOT_CREDITED, f"record {idx} key {keyid!r} is not in the consumer trust set")
            continue
        if scope.get("role") != OBSERVATION_ROLE:
            add("wrong-key-role", PHASE_NOT_CREDITED, f"record {idx} key role {scope.get('role')!r} is not {OBSERVATION_ROLE!r}")
        path = payload.get("assayCollectionPath")
        if path is not None and path not in scope.get("collectionPaths", []):
            add("key-scope-collection-path-mismatch", PHASE_NOT_CREDITED, f"record {idx} collection path {path!r} is outside the key's trusted scope")
        if scope.get("substrate") != substrate_name:
            add("key-scope-substrate-mismatch", PHASE_NOT_CREDITED, f"record {idx} key scope substrate {scope.get('substrate')!r} does not match statement substrate {substrate_name!r}")
        credit_eligible.append(idx)

    seals = [idx for idx, payload in enumerate(payloads) if payload.get("aeeKind") == "sealed"]

    # Validity window, checked against the run-end instant the seal commits to. A
    # checker with no window silently keeps crediting a retired key.
    sealed_at = parse_instant(payloads[seals[0]].get("assaySealedAt")) if seals else None
    if seals and sealed_at is None:
        add("seal-instant-invalid", PHASE_MALFORMED, f"sealed record {seals[0]} assaySealedAt is not an RFC 3339 UTC instant")
    for idx in credit_eligible:
        scope = scopes[records[idx]["signatures"][0]["keyid"]]
        valid_from = parse_instant(scope.get("validFrom"))
        valid_until = parse_instant(scope.get("validUntil"))
        if sealed_at is None or valid_from is None or valid_until is None:
            continue
        if not (valid_from <= sealed_at <= valid_until):
            add("key-outside-validity-window", PHASE_NOT_CREDITED, f"record {idx} seal instant {payloads[seals[0]].get('assaySealedAt')} is outside the key window {scope.get('validFrom')}..{scope.get('validUntil')}")

    if any(row.get("basis") == "substrate" for row in rows) and not seals:
        add("substrate-row-missing-sealed-coverage", PHASE_MALFORMED, "substrate row lacks required sealed coverage")
    caught = set(env.get("observationVocabulary", {}).get("caught", []))
    caught_attacks = sorted({row.get("attackId") for row in rows if row.get("containmentObserved") in caught})
    for idx in seals:
        seal = payloads[idx]
        # Payload-only rules (#2006 item 4).
        source_schema = seal.get("assaySourceSchema")
        if not isinstance(source_schema, str) or not source_schema:
            add("payload-source-schema-invalid", PHASE_MALFORMED, f"sealed record {idx} assaySourceSchema must be a non-empty string")
        observed_attacks = seal.get("aeeObservedAttacks")
        if not isinstance(observed_attacks, list) or not all(isinstance(item, str) for item in observed_attacks):
            add("payload-observed-attacks-invalid", PHASE_MALFORMED, f"sealed record {idx} aeeObservedAttacks must be an array of strings")
            observed_attacks = []

        # The scope is what tells a consumer which bounded enforcement boundary
        # the seal speaks for, so an unchecked one lets the strongest record in
        # the statement claim a boundary nothing observed (#2014). The two
        # branches are mutually exclusive so each isolates under `--meta-test`.
        # Digest shape, checked before the equality rules that consume these fields. A value that is
        # not a digest at all and a digest that does not match are different producer errors, and
        # only the first is decidable without the assembled statement -- which is the line the
        # sketch's payload-only phase draws. The equality rules below skip a field that failed here,
        # so each control isolates one reason.
        malformed_digests = [f for f in SEAL_DIGEST_FIELDS if not is_sha256_hex(seal.get(f))]
        if malformed_digests:
            add("payload-digest-shape-invalid", PHASE_MALFORMED, f"sealed record {idx} has non-SHA-256-hex digest fields: {malformed_digests}")

        # The rest of the #2001 payload-only contract (#2060). Each rule owns one field and skips a
        # field it does not own, so every control isolates the reason it was built for.
        aee_version = seal.get("aeeVersion")
        if aee_version != AEE_VERSION:
            add("payload-aee-version-unsupported", PHASE_MALFORMED, f"sealed record {idx} aeeVersion {aee_version!r} is not {AEE_VERSION!r}, the version this checker implements")
        method = seal.get("aeeMethod")
        if method != SEAL_METHOD:
            add("payload-method-unsupported", PHASE_MALFORMED, f"sealed record {idx} aeeMethod {method!r} is not {SEAL_METHOD!r}, the only method this slice observes")
        attribution = seal.get("assayAttackRowAttributionSource")
        if attribution not in ATTRIBUTION_SOURCES:
            add("payload-attribution-source-unknown", PHASE_MALFORMED, f"sealed record {idx} assayAttackRowAttributionSource {attribution!r} is not one of {ATTRIBUTION_SOURCES}")
        non_claims = seal.get("assayNonClaims")
        if not isinstance(non_claims, list) or not set(MINIMUM_NON_CLAIMS).issubset(non_claims):
            missing = [c for c in MINIMUM_NON_CLAIMS if not isinstance(non_claims, list) or c not in non_claims]
            add("payload-non-claims-incomplete", PHASE_MALFORMED, f"sealed record {idx} assayNonClaims omits the payload-local minimum: {missing}")
        collection_path = seal.get("assayCollectionPath")
        if collection_path != COLLECTION_PATH:
            add("payload-collection-path-mismatch", PHASE_MALFORMED, f"sealed record {idx} assayCollectionPath {collection_path!r} is not {COLLECTION_PATH!r}, the path this slice collects on")

        seal_scope = seal.get("assaySealScope")
        if not isinstance(seal_scope, str) or not seal_scope:
            add("seal-scope-missing", PHASE_MALFORMED, f"sealed record {idx} assaySealScope must be a non-empty string naming the sealed enforcement scope")
        elif seal_scope != SEAL_SCOPE:
            add("seal-scope-mismatch", PHASE_MALFORMED, f"sealed record {idx} assaySealScope {seal_scope!r} is not the scope this checker implements ({SEAL_SCOPE!r})")

        if "aeePostureDigest" not in malformed_digests and seal.get("aeePostureDigest") != env.get("networkPosture", {}).get("digest", {}).get("sha256"):
            add("posture-digest-mismatch", PHASE_MALFORMED, f"sealed record {idx} aeePostureDigest mismatch")
        if seal.get("aeeStillArmed") is not True:
            add("seal-not-still-armed", PHASE_MALFORMED, f"sealed record {idx} is not still armed")
        if seal.get("aeeDropCount") != 0 or seal.get("aeeDropBound") != 0:
            add("drop-accounting-nonzero", PHASE_MALFORMED, f"sealed record {idx} has non-zero or inconsistent drop accounting")
        if seal.get("assayDropProofModel") not in {"synchronous-probe", "counted-queue-zero"}:
            add("drop-proof-model-ineligible", PHASE_MALFORMED, f"sealed record {idx} has no eligible drop-accounting proof model")
        if "aeeObservedSet" not in malformed_digests and seal.get("aeeObservedSet") != observed_set(records):
            add("observed-set-mismatch", PHASE_MALFORMED, f"sealed record {idx} aeeObservedSet mismatch")
        for attack_id in observed_attacks:
            if attack_id not in caught_attacks:
                add("observed-attack-unsupported", PHASE_MALFORMED, f"sealed record {idx} names attack not supported by caught rows: {attack_id}")
        if seal.get("assayAttackRowAttributionSource") == "substrate-runner" and sorted(observed_attacks) != caught_attacks:
            add("substrate-runner-observed-attacks-mismatch", PHASE_MALFORMED, f"sealed record {idx} substrate-runner observed attacks mismatch")

    for row_idx, row in enumerate(rows):
        refs = row.get("observationRefs", [])
        ref_kinds = {payloads[ref].get("aeeKind") for ref in refs if isinstance(ref, int) and 0 <= ref < len(payloads)}
        if row.get("basis") == "substrate" and "arming" not in ref_kinds:
            add("substrate-row-missing-arming-coverage", PHASE_MALFORMED, f"substrate row {row_idx} lacks arming coverage")
        if row.get("basis") == "substrate" and "sealed" not in ref_kinds:
            add("substrate-row-missing-sealed-coverage", PHASE_MALFORMED, f"substrate row {row_idx} lacks sealed coverage")
        if row.get("basis") == "substrate" and row.get("containmentObserved") in caught and "interception" not in ref_kinds:
            add("substrate-row-missing-interception-coverage", PHASE_MALFORMED, f"caught substrate row {row_idx} lacks interception coverage")
    return findings


def outcome_of(findings: list[Finding]) -> str:
    if any(finding.phase == PHASE_MALFORMED for finding in findings):
        return OUTCOME_MALFORMED
    if any(finding.phase == PHASE_NOT_CREDITED for finding in findings):
        return OUTCOME_NOT_CREDITED
    return OUTCOME_CREDITED


def report(case: str, findings: list[Finding]) -> str:
    outcome = outcome_of(findings)
    lines = [f"{outcome}: {case}"]
    lines.extend(f"- [{finding.code}] {finding.message}" for finding in findings)
    return "\n".join(lines)


def run_rule_coverage_test() -> int:
    """Every reason code the checker can produce has a negative control that produces it.

    `--meta-test` proves each control fails for its own reason. It says nothing about a rule with no
    control at all, and there were nine: a rule can exist, be believed, and never be shown to fire.
    One of them was `substrate-row-missing-arming-coverage`, which #1998 requires the checker reject
    by name -- so the acceptance criterion was met in code and unproven in the suite.

    Writing the missing controls also found `payload-not-object` to be unreachable: the checker
    recorded the finding and then crashed in `observed_set` before returning it.

    Parsed from the source rather than from a hand-kept list, for the same reason the gating map is:
    a list beside the rules is one more thing to drift.
    """
    src = Path(__file__).read_text(encoding="utf-8")
    rules = set(re.findall(r'add\("([a-z0-9-]+)"', src))
    covered = set(NEGATIVE_CONTROLS.values())

    uncovered = sorted(rules - covered)
    for code in uncovered:
        print(f"FAIL {code}: the checker can produce this and no negative control does")
    unused = sorted(covered - rules)
    for code in unused:
        print(f"FAIL {code}: a control targets this and no rule produces it")

    if uncovered or unused:
        print(f"\n{len(uncovered) + len(unused)} rule/control mismatch(es)")
        return 1
    print(f"\nall {len(rules)} reason codes have a negative control")
    return 0


def run_required_field_test() -> int:
    """Every required field, removed one at a time, must be rejected by some rule.

    #2060 asked for a "required field present" rule. There is deliberately not one. Nearly every
    required field already has a rule that owns it, and a generic presence rule would fire alongside
    those -- so no control could isolate its own reason any more, which is the property `--meta-test`
    exists to hold.

    What was actually missing was proof that the set of owned fields covers the contract. That is
    this: remove each required field from the positive fixture and require the checker to reject.
    Adding a field to `REQUIRED_SEAL_FIELDS` without a rule that catches its absence fails here.
    """
    base = load_case(POSITIVE_CASE)
    failures = 0
    for field in REQUIRED_SEAL_FIELDS:
        stmt = json.loads(json.dumps(base))
        seal = json.loads(json.dumps(stmt["predicate"]["observationRecords"][2]["payload"]))
        if field not in seal:
            print(f"FAIL {field}: not present in the positive fixture, so the contract and the fixture disagree")
            failures += 1
            continue
        del seal[field]
        replace_payload(stmt, 2, seal)
        findings = validate(stmt)
        if outcome_of(findings) == OUTCOME_CREDITED:
            print(f"FAIL {field}: removed, and the seal was still credited")
            failures += 1
        else:
            print(f"ok   {field}: absence is rejected [{findings[0].code}]")

    # The other half: the fixture must not carry a field the contract does not require, or the list
    # above stops being a statement about the contract and becomes a description of the fixture.
    seal = base["predicate"]["observationRecords"][2]["payload"]
    optional = {"assayObservedLabels"}
    unlisted = sorted(set(seal) - set(REQUIRED_SEAL_FIELDS) - optional)
    if unlisted:
        print(f"FAIL fixture carries fields that are neither required nor known-optional: {unlisted}")
        failures += 1

    if failures:
        print(f"\n{failures} required-field check(s) failed")
        return 1
    print(f"\nall {len(REQUIRED_SEAL_FIELDS)} required fields are rejected when absent")
    return 0


def run_meta_test() -> int:
    """Assert every negative control fails for the reason it was built for.

    For each control, disable exactly the code it targets and require the case to
    become credited. A control that still fails was failing for another reason,
    and reported coverage that did not exist.
    """
    failures = 0
    for name, code in NEGATIVE_CONTROLS.items():
        findings = validate(load_case(name))
        codes = {finding.code for finding in findings}
        if code not in codes:
            print(f"FAIL {name}: expected reason {code!r}, got {sorted(codes) or 'no findings'}")
            failures += 1
            continue
        residual = validate(load_case(name), disabled=frozenset({code}))
        if residual:
            print(f"FAIL {name}: with {code!r} disabled it still fails: {sorted({f.code for f in residual})}")
            failures += 1
            continue
        print(f"ok {name}: rejects with {code}, and only that")
    if failures:
        print(f"{failures} negative control(s) do not isolate their reason")
        return 1
    print(f"all {len(NEGATIVE_CONTROLS)} negative controls isolate their reason")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("case", nargs="?", choices=CASES, help="Named fixture case to check")
    parser.add_argument("--emit", action="store_true", help="Write all named fixtures to scripts/experiments/fixtures/aee-landlock-seal")
    parser.add_argument("--expect-invalid", action="store_true", help="Exit 0 only when validation rejects")
    parser.add_argument("--expect-reason", help="Require this reason code among the findings")
    parser.add_argument("--disable-check", action="append", default=[], metavar="CODE", help="Harness-only: suppress a reason code (used by --meta-test)")
    parser.add_argument("--meta-test", action="store_true", help="Assert each negative control fails for its own reason and no other")
    parser.add_argument("--required-field-test", action="store_true", help="Assert every required payload field is rejected when absent")
    parser.add_argument("--rule-coverage-test", action="store_true", help="Assert every reason code the checker can produce has a negative control")
    args = parser.parse_args()

    if args.emit:
        emit_fixtures()
        emit_parity_vectors()
        return 0
    if args.meta_test:
        return run_meta_test()
    if args.required_field_test:
        return run_required_field_test()
    if args.rule_coverage_test:
        return run_rule_coverage_test()
    if not args.case:
        parser.error("case is required unless --emit or one of the --*-test flags is used")

    findings = validate(load_case(args.case), disabled=frozenset(args.disable_check))
    print(report(args.case, findings))

    if args.expect_reason and args.expect_reason not in {finding.code for finding in findings}:
        print(f"expected reason {args.expect_reason!r} not among findings")
        return 1
    if args.expect_invalid:
        if findings:
            return 0
        print(f"expected rejection but {args.case} was credited")
        return 1
    return 1 if findings else 0


if __name__ == "__main__":
    raise SystemExit(main())
