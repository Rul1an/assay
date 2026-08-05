"""Shared helpers for the fixture-only Assay -> AEE v0.7 spike.

This module is intentionally narrow. It is not a general AEE verifier or a
production signing library; it keeps the experiment's deterministic JSON,
fixture signature, run-binding, and RFC6962-style Merkle rules in one place so
emitter and checker cannot drift.
"""

from __future__ import annotations

import base64
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

VALID_RESULTS = ["fail", "degraded", "pass_indirect", "pass"]
VALID_BASIS = {"substrate", "artifact"}
VALID_METHOD = {"intercepted", "reconstructed"}
VALID_ATTRIBUTION = {"pinned", "paired"}
COVERING_KINDS = {"interception", "arming", "sealed", "examination"}

EXPERIMENT_ROOT = Path(__file__).resolve().parent
FIXTURES = EXPERIMENT_ROOT / "fixtures" / "aee"
NEGATIVE_CONTROLS = FIXTURES / "negative-controls"

SOURCE_FIXTURE_PATHS = {
    "catch-policy": FIXTURES / "catch-policy.json",
    "corpus-manifest": FIXTURES / "corpus-manifest.json",
    "enforcement-health-v1-active-probe": FIXTURES / "enforcement-health-v1-active-probe.json",
    "proxy-deny-observation": FIXTURES / "proxy-deny-observation.json",
    "substrate-descriptor": FIXTURES / "substrate-descriptor.json",
}

STATEMENT_PATHS = {
    "valid": FIXTURES / "statement-valid.json",
    "artifact-labelled-substrate": NEGATIVE_CONTROLS / "statement-artifact-labelled-substrate.json",
    "defective-unreferenced-seal": NEGATIVE_CONTROLS / "statement-defective-unreferenced-seal.json",
    "missing-seal": NEGATIVE_CONTROLS / "statement-missing-seal.json",
    "reconstructed-priced-intercepted": NEGATIVE_CONTROLS / "statement-reconstructed-priced-intercepted.json",
    "run-population-overclaim": NEGATIVE_CONTROLS / "statement-run-population-overclaim.json",
}


def canonical_bytes(value: Any) -> bytes:
    """Return deterministic JSON bytes close to the JCS shape used by AEE.

    The fixture uses ASCII-only member names/values and safe integers. This is
    not a general RFC 8785 implementation; the experiment keeps this single
    helper as the shared rule for all local digests and fixture signatures.
    """

    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def digest_json(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def read_source_fixture(name: str) -> Any:
    return _read_known_json(SOURCE_FIXTURE_PATHS[name])


def read_statement_fixture(name: str) -> Any:
    return _read_known_json(STATEMENT_PATHS[name])


def write_statement_fixture(name: str, value: Any) -> Path:
    path = STATEMENT_PATHS[name]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


def _read_known_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def dsse_pae(payload_type: str, payload: bytes) -> bytes:
    payload_type_bytes = payload_type.encode("utf-8")
    return b"DSSEv1 " + str(len(payload_type_bytes)).encode("ascii") + b" " + payload_type_bytes + b" " + str(len(payload)).encode("ascii") + b" " + payload


def sign_payload(payload_type: str, payload: bytes) -> str:
    """Return a deterministic fixture-only signature.

    This is intentionally HMAC over DSSE PAE so the fixture is self-contained in
    Python's standard library. It is not a production DSSE signature algorithm.
    """

    return base64.b64encode(hmac.new(FIXTURE_KEY, dsse_pae(payload_type, payload), hashlib.sha256).digest()).decode("ascii")


def observation_record(payload: dict[str, Any], seq: int) -> dict[str, Any]:
    payload_bytes = canonical_bytes(payload)
    return {
        "payload": base64.b64encode(payload_bytes).decode("ascii"),
        "payloadType": PAYLOAD_TYPE,
        "signatures": [{"keyid": FIXTURE_KEY_ID, "sig": sign_payload(PAYLOAD_TYPE, payload_bytes)}],
        "seq": seq,
    }


def decode_record(record: dict[str, Any]) -> tuple[dict[str, Any], bytes]:
    payload_bytes = base64.b64decode(record["payload"])
    payload = json.loads(payload_bytes.decode("utf-8"))
    return payload, payload_bytes


def decode_payload(record: dict[str, Any]) -> dict[str, Any]:
    payload, _ = decode_record(record)
    return payload


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


def run_binding_input_from_statement(statement: dict[str, Any]) -> dict[str, str]:
    subject = statement["subject"][0]
    env = statement["predicate"]["observationEnvironment"]
    network_posture = env["networkPosture"]
    return {
        "aeeBindingVersion": "2",
        "catchPolicy": env["catchPolicy"]["digest"]["sha256"],
        "corpus": env["corpus"]["digest"]["sha256"],
        "networkPosture": digest_json(network_posture),
        "observationVocabulary": env["observationVocabulary"]["digest"]["sha256"],
        "runEntropy": env["runEntropy"]["digest"]["sha256"],
        "subject": subject["digest"]["sha256"],
        "substrate": env["substrate"]["digest"]["sha256"],
    }


def run_binding_from_statement(statement: dict[str, Any]) -> str:
    return digest_json(run_binding_input_from_statement(statement))
