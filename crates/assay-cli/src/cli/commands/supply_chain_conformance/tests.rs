use std::path::{Path, PathBuf};

use assay_registry::supply_chain::{SupplyChainConformance, SCHEMA};
use serde_json::{json, Value};

use super::descriptor::{build_carrier, EmitErr, DSSE_IN_TOTO_PAYLOAD_TYPE, INPUT_SCHEMA};
use super::map_write_result;
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_INFRA_ERROR, EXIT_SUCCESS};

fn descriptor(provenance: Value) -> String {
    json!({
        "schema": INPUT_SCHEMA,
        "subject": { "name": "demo", "version": "1.0.0", "digest": "sha256:aa" },
        "provenance": provenance,
        "pinning": { "version_pinned": true, "digest_pinned": true },
        "policy": { "required_slsa_build_level": 0 }
    })
    .to_string()
}

fn bc(raw: &str) -> Result<SupplyChainConformance, EmitErr> {
    // none/unsupported never consult base_dir; "." is a harmless placeholder for those cases.
    build_carrier(raw, Path::new("."))
}

fn carrier_value(raw: &str) -> Value {
    serde_json::to_value(bc(raw).expect("carrier")).expect("value")
}

#[test]
fn none_provenance_emits_a_valid_carrier() {
    let v = carrier_value(&descriptor(json!({ "kind": "none" })));
    assert_eq!(v["schema"], SCHEMA);
    // provenance is absent, not verified as trusted.
    assert_eq!(v["checks"]["provenance"]["slsa_provenance"], "not_present");
}

#[test]
fn unsupported_provenance_records_unsupported_format() {
    let v = carrier_value(&descriptor(
        json!({ "kind": "unsupported", "format": "pep740" }),
    ));
    assert_eq!(v["schema"], SCHEMA);
    assert_eq!(
        v["checks"]["provenance"]["slsa_provenance"],
        "unsupported_format"
    );
}

#[test]
fn expected_artifact_digest_mismatch_fails_the_integrity_check() {
    // expected_artifact_digest != subject.digest -> integrity.artifact_digest = failed (real check).
    let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
    d["expected_artifact_digest"] = json!("sha256:bb"); // subject.digest is sha256:aa
    let v = carrier_value(&d.to_string());
    assert_eq!(v["checks"]["integrity"]["artifact_digest"], "failed");
}

#[test]
fn matching_expected_artifact_digest_verifies_the_integrity_check() {
    let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
    d["expected_artifact_digest"] = json!("sha256:aa"); // == subject.digest
    let v = carrier_value(&d.to_string());
    assert_eq!(v["checks"]["integrity"]["artifact_digest"], "verified");
}

#[test]
fn wrong_input_schema_is_rejected() {
    let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
    d["schema"] = json!("assay.supply_chain_conformance.v0"); // the carrier schema, not the input schema
    assert_eq!(bc(&d.to_string()).err().unwrap().code, EXIT_CONFIG_ERROR);
}

#[test]
fn unknown_top_level_field_is_rejected() {
    let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
    d["trust_me_bro"] = json!(true);
    assert!(bc(&d.to_string()).is_err());
}

#[test]
fn unknown_provenance_kind_is_rejected() {
    assert!(bc(&descriptor(json!({ "kind": "weird" }))).is_err());
}

#[test]
fn unsupported_without_format_is_rejected() {
    assert!(bc(&descriptor(json!({ "kind": "unsupported" }))).is_err());
}

#[test]
fn unknown_unsupported_format_is_rejected() {
    assert!(bc(&descriptor(
        json!({ "kind": "unsupported", "format": "made_up" })
    ))
    .is_err());
}

#[test]
fn dsse_without_required_fields_is_rejected() {
    // dsse is now wired; omitting its required fields is a config error, never a silent None.
    let e = bc(&descriptor(json!({ "kind": "dsse" }))).err().unwrap();
    assert_eq!(e.code, EXIT_CONFIG_ERROR);
    assert!(e.msg.contains("is required for kind \"dsse\""));
}

#[test]
fn deferred_sigstore_bundle_is_rejected_not_ignored() {
    assert!(bc(&descriptor(json!({ "kind": "sigstore_bundle" }))).is_err());
}

#[test]
fn write_failure_maps_to_infra_error_for_any_target() {
    use std::io::{Error, ErrorKind};

    // A write failure is EXIT_INFRA_ERROR uniformly - stdout (broken pipe) and file alike.
    assert_eq!(
        map_write_result(
            "stdout",
            Err(Error::new(ErrorKind::BrokenPipe, "pipe closed"))
        ),
        EXIT_INFRA_ERROR
    );
    assert_eq!(
        map_write_result(
            "/tmp/x.json",
            Err(Error::new(ErrorKind::PermissionDenied, "nope"))
        ),
        EXIT_INFRA_ERROR
    );
    assert_eq!(map_write_result("stdout", Ok(())), EXIT_SUCCESS);
}

#[test]
fn unknown_container_ref_is_rejected() {
    let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
    d["pinning"]["container_ref"] = json!("floating");
    assert!(bc(&d.to_string()).is_err());
}

// ---- DSSE pinned-key path -------------------------------------------------------------------
// Fixtures are generated deterministically (fixed seed) in a tempdir per test; no committed crypto
// blobs. The crypto/PAE/in-toto/policy live in `assay_registry` and are exercised through it here.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::ed25519::signature::Signer;
use ed25519_dalek::pkcs8::EncodePublicKey;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::fs;
use tempfile::{tempdir, TempDir};

const DSSE_ART_HEX: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const DSSE_ART: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const DSSE_BUILDER: &str = "https://github.com/example/builder@refs/tags/v1";

/// DSSE PAE, byte-identical to the verifier's `build_pae` (the bytes the signature is computed over).
fn pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"DSSEv1 ");
    v.extend_from_slice(payload_type.len().to_string().as_bytes());
    v.push(b' ');
    v.extend_from_slice(payload_type.as_bytes());
    v.push(b' ');
    v.extend_from_slice(payload.len().to_string().as_bytes());
    v.push(b' ');
    v.extend_from_slice(payload);
    v
}

fn key_id_of(spki_der: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(spki_der);
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// A deterministic Ed25519 key from `seed`; returns (signer, key_id, TrustedKey JSON).
fn fixture_key(seed: u8) -> (SigningKey, String, String) {
    let sk = SigningKey::from_bytes(&[seed; 32]);
    let der = sk
        .verifying_key()
        .to_public_key_der()
        .unwrap()
        .as_bytes()
        .to_vec();
    let key_id = key_id_of(&der);
    let trusted = json!({
        "key_id": key_id, "algorithm": "Ed25519", "public_key": B64.encode(&der)
    })
    .to_string();
    (sk, key_id, trusted)
}

fn statement(subject_hex: &str, predicate_type: &str, builder: &str) -> String {
    json!({
        "_type": "https://in-toto.io/Statement/v1",
        "subject": [{ "name": "art", "digest": { "sha256": subject_hex } }],
        "predicateType": predicate_type,
        "predicate": { "runDetails": { "builder": { "id": builder } } }
    })
    .to_string()
}

fn envelope(sk: &SigningKey, key_id: &str, statement: &str) -> String {
    let payload = statement.as_bytes();
    let sig = sk.sign(&pae(DSSE_IN_TOTO_PAYLOAD_TYPE, payload));
    json!({
        "payloadType": DSSE_IN_TOTO_PAYLOAD_TYPE,
        "payload": B64.encode(payload),
        "signatures": [{ "keyid": key_id, "sig": B64.encode(sig.to_bytes()) }]
    })
    .to_string()
}

/// Write envelope + key into a fresh tempdir and return (descriptor_raw, dir). The descriptor binds
/// `expected_artifact_digest = DSSE_ART` and a policy requiring the matching builder at L1.
fn dsse_case(envelope_json: &str, trusted_json: &str, extra_policy: Value) -> (String, TempDir) {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("env.dsse.json"), envelope_json).unwrap();
    fs::write(dir.path().join("key.trustedkey.json"), trusted_json).unwrap();
    let mut policy = json!({ "required_builder_id": DSSE_BUILDER, "required_slsa_build_level": 1 });
    if let Value::Object(extra) = extra_policy {
        for (k, val) in extra {
            policy[k] = val;
        }
    }
    let desc = json!({
        "schema": INPUT_SCHEMA,
        "subject": { "name": "art", "version": "1.0.0", "digest": DSSE_ART },
        "expected_artifact_digest": DSSE_ART,
        "provenance": {
            "kind": "dsse",
            "payload_type": DSSE_IN_TOTO_PAYLOAD_TYPE,
            "envelope_path": "env.dsse.json",
            "trusted_key_path": "key.trustedkey.json"
        },
        "pinning": { "version_pinned": true, "digest_pinned": true, "lockfile_digest": DSSE_ART },
        "policy": policy
    })
    .to_string();
    (desc, dir)
}

#[test]
fn dsse_pinned_key_happy_path_emits_a_pass_carrier() {
    let (sk, key_id, trusted) = fixture_key(7);
    let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
    let (desc, dir) = dsse_case(&envelope(&sk, &key_id, &stmt), &trusted, json!({}));
    let v = serde_json::to_value(build_carrier(&desc, dir.path()).expect("carrier")).unwrap();
    assert_eq!(v["checks"]["provenance"]["dsse_signature"], "verified");
    assert_eq!(
        v["checks"]["integrity"]["subject_digest_binding"],
        "verified"
    );
    assert_eq!(v["policy_result"], "pass");
    // pinned-key DSSE is NOT the Sigstore leaf path, and never claims transparency-log evidence.
    assert_ne!(v["checks"]["provenance"]["dsse_pae"], "verified");
    assert_ne!(v["checks"]["provenance"]["rekor_inclusion"], "verified");
}

#[test]
fn dsse_wrong_pinned_key_is_not_clean() {
    // Envelope signed by key A; trust store pins a DIFFERENT key B -> keyid A not found.
    let (sk, signed_key_id, _) = fixture_key(7);
    let (_, _, other_trusted) = fixture_key(9);
    let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
    let (desc, dir) = dsse_case(
        &envelope(&sk, &signed_key_id, &stmt),
        &other_trusted,
        json!({}),
    );
    let v = serde_json::to_value(build_carrier(&desc, dir.path()).expect("carrier")).unwrap();
    assert_ne!(v["policy_result"], "pass");
    assert_eq!(
        v["checks"]["provenance"]["dsse_signature"],
        "trust_root_unavailable"
    );
}

#[test]
fn dsse_tampered_payload_fails_the_signature() {
    let (sk, key_id, trusted) = fixture_key(7);
    let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
    let mut env: Value = serde_json::from_str(&envelope(&sk, &key_id, &stmt)).unwrap();
    // Swap the payload AFTER signing: the signature no longer matches the PAE over the new payload.
    let tampered = statement(
        DSSE_ART_HEX,
        "https://slsa.dev/provenance/v1",
        "https://evil/builder",
    );
    env["payload"] = json!(B64.encode(tampered.as_bytes()));
    let (desc, dir) = dsse_case(&env.to_string(), &trusted, json!({}));
    let v = serde_json::to_value(build_carrier(&desc, dir.path()).expect("carrier")).unwrap();
    assert_ne!(v["policy_result"], "pass");
    assert_eq!(v["checks"]["provenance"]["dsse_signature"], "failed");
}

#[test]
fn dsse_subject_digest_mismatch_is_reported() {
    let (sk, key_id, trusted) = fixture_key(7);
    let other_hex = "2222222222222222222222222222222222222222222222222222222222222222";
    let stmt = statement(other_hex, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
    let (desc, dir) = dsse_case(&envelope(&sk, &key_id, &stmt), &trusted, json!({}));
    let v = serde_json::to_value(build_carrier(&desc, dir.path()).expect("carrier")).unwrap();
    assert_ne!(v["policy_result"], "pass");
    assert_eq!(
        v["checks"]["integrity"]["subject_digest_binding"],
        "subject_digest_mismatch"
    );
}

#[test]
fn dsse_require_rekor_inclusion_is_incomplete_never_pass() {
    // A DSSE-only carrier cannot satisfy a Rekor requirement: incomplete, never a magic pass.
    let (sk, key_id, trusted) = fixture_key(7);
    let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
    let (desc, dir) = dsse_case(
        &envelope(&sk, &key_id, &stmt),
        &trusted,
        json!({ "require_rekor_inclusion": true }),
    );
    let v = serde_json::to_value(build_carrier(&desc, dir.path()).expect("carrier")).unwrap();
    assert_eq!(v["policy_result"], "incomplete");
}

#[test]
fn dsse_payload_type_must_be_in_toto() {
    let (_sk, _id, trusted) = fixture_key(7);
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("env.dsse.json"), "{}").unwrap();
    fs::write(dir.path().join("key.trustedkey.json"), &trusted).unwrap();
    let desc = json!({
        "schema": INPUT_SCHEMA,
        "subject": { "name": "art", "version": "1.0.0", "digest": DSSE_ART },
        "provenance": { "kind": "dsse", "payload_type": "application/x-wrong",
            "envelope_path": "env.dsse.json", "trusted_key_path": "key.trustedkey.json" },
        "pinning": { "version_pinned": true },
        "policy": { "required_slsa_build_level": 0 }
    })
    .to_string();
    let e = build_carrier(&desc, dir.path()).err().unwrap();
    assert_eq!(e.code, EXIT_CONFIG_ERROR);
    assert!(e.msg.contains("payload_type must be"));
}

#[test]
fn dsse_path_safety_rejects_absolute_traversal_and_url() {
    let (_sk, _id, trusted) = fixture_key(7);
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("key.trustedkey.json"), &trusted).unwrap();
    for bad in ["/etc/passwd", "../escape.json", "https://evil/env.json"] {
        let desc = json!({
            "schema": INPUT_SCHEMA,
            "subject": { "name": "art", "version": "1.0.0", "digest": DSSE_ART },
            "provenance": { "kind": "dsse", "payload_type": DSSE_IN_TOTO_PAYLOAD_TYPE,
                "envelope_path": bad, "trusted_key_path": "key.trustedkey.json" },
            "pinning": { "version_pinned": true },
            "policy": { "required_slsa_build_level": 0 }
        })
        .to_string();
        assert_eq!(
            build_carrier(&desc, dir.path()).err().unwrap().code,
            EXIT_CONFIG_ERROR,
            "path {bad:?} must be rejected"
        );
    }
}

#[test]
fn dsse_trustedkey_with_wrong_key_id_is_rejected() {
    // TrustedKey.key_id != sha256(SPKI-DER): prepare_pinned_key rejects -> config error (non-zero).
    let (sk, key_id, _) = fixture_key(7);
    let der = sk
        .verifying_key()
        .to_public_key_der()
        .unwrap()
        .as_bytes()
        .to_vec();
    let bad_trusted = json!({
        "key_id": "sha256:deadbeef", "algorithm": "Ed25519", "public_key": B64.encode(&der)
    })
    .to_string();
    let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
    let (desc, dir) = dsse_case(&envelope(&sk, &key_id, &stmt), &bad_trusted, json!({}));
    let e = build_carrier(&desc, dir.path()).err().unwrap();
    assert_eq!(e.code, EXIT_CONFIG_ERROR);
    assert!(e.msg.contains("invalid pinned key"));
}

#[test]
fn dsse_fields_on_non_dsse_kind_are_rejected() {
    let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
    d["provenance"]["envelope_path"] = json!("env.json");
    assert!(bc(&d.to_string()).is_err());
}

fn committed_example_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/supply_chain_conformance_dsse")
}

#[test]
fn regen_committed_dsse_example() {
    // Gated: `REGEN_DSSE_EXAMPLE=1 cargo test ... regen_committed_dsse_example` (re)writes the
    // committed example. Deterministic (fixed seed), so the committed bytes are reproducible.
    if std::env::var_os("REGEN_DSSE_EXAMPLE").is_none() {
        return;
    }
    let dir = committed_example_dir();
    fs::create_dir_all(&dir).unwrap();
    let (sk, key_id, trusted) = fixture_key(7);
    let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
    // Trailing newlines so the regen output is stable under the end-of-file-fixer pre-commit hook.
    fs::write(
        dir.join("slsa-provenance.statement.json"),
        format!("{stmt}\n"),
    )
    .unwrap();
    fs::write(
        dir.join("slsa-provenance.dsse.json"),
        format!("{}\n", envelope(&sk, &key_id, &stmt)),
    )
    .unwrap();
    fs::write(
        dir.join("pinned-ed25519.trustedkey.json"),
        format!("{trusted}\n"),
    )
    .unwrap();
    let desc = json!({
        "schema": INPUT_SCHEMA,
        "subject": { "name": "example-artifact", "version": "1.0.0", "digest": DSSE_ART },
        "expected_artifact_digest": DSSE_ART,
        "provenance": {
            "kind": "dsse",
            "payload_type": DSSE_IN_TOTO_PAYLOAD_TYPE,
            "envelope_path": "slsa-provenance.dsse.json",
            "trusted_key_path": "pinned-ed25519.trustedkey.json"
        },
        "pinning": { "version_pinned": true, "digest_pinned": true, "lockfile_digest": DSSE_ART },
        "policy": { "required_builder_id": DSSE_BUILDER, "required_slsa_build_level": 1 }
    });
    fs::write(
        dir.join("input.dsse.example.json"),
        serde_json::to_string_pretty(&desc).unwrap() + "\n",
    )
    .unwrap();
}

#[test]
fn committed_dsse_example_emits_a_pass_carrier() {
    // Drift guard: the committed example must still produce a pass carrier with a verified pinned-key
    // signature. If this fails after a deliberate change, re-run `regen_committed_dsse_example`.
    let dir = committed_example_dir();
    let raw = fs::read_to_string(dir.join("input.dsse.example.json"))
        .expect("committed example descriptor");
    let v = serde_json::to_value(build_carrier(&raw, &dir).expect("carrier")).unwrap();
    assert_eq!(v["policy_result"], "pass");
    assert_eq!(v["checks"]["provenance"]["dsse_signature"], "verified");
    assert_eq!(
        v["checks"]["integrity"]["subject_digest_binding"],
        "verified"
    );
}
