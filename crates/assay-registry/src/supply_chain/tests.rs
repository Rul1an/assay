use super::*;
use crate::types::{DsseSignature, TrustedKey};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::ed25519::signature::Signer;
use ed25519_dalek::{SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

const ARTIFACT_DIGEST: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const BUILDER: &str = "https://github.com/example/builder@refs/tags/v1";

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn spki_der(vk: &VerifyingKey) -> Vec<u8> {
    use ed25519_dalek::pkcs8::EncodePublicKey;
    vk.to_public_key_der().unwrap().as_bytes().to_vec()
}

/// Build a trust store with the given verifying key pinned; returns (store, key_id).
fn trust_with(vk: &VerifyingKey) -> (crate::trust::TrustStore, String) {
    let der = spki_der(vk);
    let key_id = format!("sha256:{}", sha256_hex(&der));
    let key = TrustedKey {
        key_id: key_id.clone(),
        algorithm: "Ed25519".to_string(),
        public_key: BASE64.encode(&der),
        description: None,
        added_at: None,
        expires_at: None,
        revoked: false,
    };
    (
        crate::trust::TrustStore::from_pinned_roots(vec![key]).unwrap(),
        key_id,
    )
}

fn statement_json(subject_digest_hex: &str, predicate_type: &str, builder: &str) -> String {
    serde_json::json!({
        "_type": STATEMENT_TYPE_V1,
        "subject": [{ "name": "pack", "digest": { "sha256": subject_digest_hex } }],
        "predicateType": predicate_type,
        "predicate": { "runDetails": { "builder": { "id": builder } } }
    })
    .to_string()
}

/// A signed in-toto/SLSA DSSE envelope over `statement`, signed by `sk`.
fn signed_dsse(sk: &SigningKey, key_id: &str, statement: &str) -> crate::types::DsseEnvelope {
    let payload_bytes = statement.as_bytes().to_vec();
    let payload_b64 = BASE64.encode(&payload_bytes);
    let pae = provenance::build_pae(DSSE_PAYLOAD_TYPE, &payload_bytes);
    let sig = sk.sign(&pae);
    crate::types::DsseEnvelope {
        payload_type: DSSE_PAYLOAD_TYPE.to_string(),
        payload: payload_b64,
        signatures: vec![DsseSignature {
            key_id: key_id.to_string(),
            signature: BASE64.encode(sig.to_bytes()),
        }],
    }
}

fn subject() -> Subject {
    Subject {
        name: "mcp-pack".to_string(),
        version: "1.2.3".to_string(),
        digest: ARTIFACT_DIGEST.to_string(),
    }
}

fn clean_pinning() -> PinningInput {
    PinningInput {
        version_pinned: true,
        digest_pinned: Some(true),
        lockfile_digest: Some(ARTIFACT_DIGEST.to_string()),
        floating_source_ref: false,
        container_ref: Some(ContainerRef::DigestPinned),
    }
}

fn policy(level: u8) -> Policy {
    Policy {
        required_builder_id: Some(BUILDER.to_string()),
        required_slsa_build_level: SlsaLevel(level),
        require_rekor_inclusion: false,
        require_timestamp_freshness: false,
        require_consistency: false,
        require_witnessing: false,
    }
}

#[test]
fn valid_pinned_key_slsa_provenance_is_verified_and_clean() {
    let sk = SigningKey::from_bytes(&[7u8; 32]);
    let (store, key_id) = trust_with(&sk.verifying_key());
    let env = signed_dsse(
        &sk,
        &key_id,
        &statement_json(hex_of(ARTIFACT_DIGEST), SLSA_PROVENANCE_PREDICATE, BUILDER),
    );
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: Some(ARTIFACT_DIGEST.to_string()),
        provenance: ProvenanceInput::Dsse(env),
        pinning: clean_pinning(),
        policy: policy(2),
        trust_store: &store,
    });
    assert_eq!(
        report.checks.provenance.dsse_signature,
        CheckStatus::Verified
    );
    assert_eq!(
        report.checks.provenance.slsa_provenance,
        CheckStatus::Verified
    );
    assert_eq!(
        report.checks.provenance.builder_identity,
        CheckStatus::Verified
    );
    assert_eq!(
        report.checks.integrity.subject_digest_binding,
        CheckStatus::Verified
    );
    assert_eq!(report.verified.slsa_build_level, SlsaLevel(2));
    assert_eq!(report.policy_result, PolicyResult::Pass);
    assert!(is_clean(&report));
}

#[test]
fn missing_provenance_is_not_present_never_clean() {
    let store = crate::trust::TrustStore::new();
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::None,
        pinning: clean_pinning(),
        policy: policy(2),
        trust_store: &store,
    });
    assert_eq!(
        report.checks.provenance.slsa_provenance,
        CheckStatus::NotPresent
    );
    assert_eq!(report.policy_result, PolicyResult::Incomplete);
    assert!(!is_clean(&report));
}

#[test]
fn pep740_npm_are_unsupported_format_never_pass() {
    let store = crate::trust::TrustStore::new();
    for kind in [
        UnsupportedProvenance::Pep740,
        UnsupportedProvenance::NpmProvenance,
    ] {
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::Unsupported(kind),
            pinning: clean_pinning(),
            policy: policy(2),
            trust_store: &store,
        });
        assert_eq!(
            report.checks.provenance.slsa_provenance,
            CheckStatus::UnsupportedFormat
        );
        assert_eq!(
            report.checks.provenance.sigstore_bundle,
            CheckStatus::UnsupportedFormat
        );
        assert!(!is_clean(&report));
        assert_eq!(report.policy_result, PolicyResult::Incomplete);
    }
}

#[test]
fn subject_digest_mismatch_fails() {
    let sk = SigningKey::from_bytes(&[9u8; 32]);
    let (store, key_id) = trust_with(&sk.verifying_key());
    let env = signed_dsse(
        &sk,
        &key_id,
        &statement_json("deadbeef", SLSA_PROVENANCE_PREDICATE, BUILDER),
    );
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::Dsse(env),
        pinning: clean_pinning(),
        policy: policy(1),
        trust_store: &store,
    });
    assert_eq!(
        report.checks.integrity.subject_digest_binding,
        CheckStatus::SubjectDigestMismatch
    );
    assert_eq!(report.policy_result, PolicyResult::Fail);
}

#[test]
fn declared_l3_but_unverifiable_fails() {
    let sk = SigningKey::from_bytes(&[3u8; 32]);
    let (store, key_id) = trust_with(&sk.verifying_key());
    let env = signed_dsse(
        &sk,
        &key_id,
        &statement_json(hex_of(ARTIFACT_DIGEST), SLSA_PROVENANCE_PREDICATE, BUILDER),
    );
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::Dsse(env),
        pinning: clean_pinning(),
        policy: policy(3),
        trust_store: &store,
    });
    assert_eq!(report.verified.slsa_build_level, SlsaLevel(2));
    assert_eq!(
        report.checks.provenance.slsa_provenance,
        CheckStatus::Failed
    );
    assert_eq!(report.policy_result, PolicyResult::Fail);
}

#[test]
fn builder_identity_mismatch() {
    let sk = SigningKey::from_bytes(&[5u8; 32]);
    let (store, key_id) = trust_with(&sk.verifying_key());
    let env = signed_dsse(
        &sk,
        &key_id,
        &statement_json(
            hex_of(ARTIFACT_DIGEST),
            SLSA_PROVENANCE_PREDICATE,
            "https://evil/builder",
        ),
    );
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::Dsse(env),
        pinning: clean_pinning(),
        policy: policy(1),
        trust_store: &store,
    });
    assert_eq!(
        report.checks.provenance.builder_identity,
        CheckStatus::IdentityMismatch
    );
    assert_eq!(report.policy_result, PolicyResult::Fail);
}

#[test]
fn floating_source_ref_is_policy_not_satisfied() {
    let store = crate::trust::TrustStore::new();
    let mut pinning = clean_pinning();
    pinning.floating_source_ref = true;
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::None,
        pinning,
        policy: policy(0),
        trust_store: &store,
    });
    assert_eq!(
        report.checks.pinning.no_floating_source_ref,
        CheckStatus::PolicyNotSatisfied
    );
    assert_eq!(report.policy_result, PolicyResult::Fail);
}

#[test]
fn lockfile_digest_mismatch_fails() {
    let store = crate::trust::TrustStore::new();
    let mut pinning = clean_pinning();
    pinning.lockfile_digest =
        Some("sha256:9999999999999999999999999999999999999999999999999999999999999999".to_string());
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::None,
        pinning,
        policy: policy(0),
        trust_store: &store,
    });
    assert_eq!(
        report.checks.pinning.lockfile_subject_matches_artifact,
        CheckStatus::Failed
    );
    assert_eq!(report.policy_result, PolicyResult::Fail);
}

#[test]
fn trust_root_missing_is_trust_root_unavailable() {
    let sk = SigningKey::from_bytes(&[1u8; 32]);
    let store = crate::trust::TrustStore::new();
    let env = signed_dsse(
        &sk,
        "sha256:notinstore",
        &statement_json(hex_of(ARTIFACT_DIGEST), SLSA_PROVENANCE_PREDICATE, BUILDER),
    );
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::Dsse(env),
        pinning: clean_pinning(),
        policy: policy(1),
        trust_store: &store,
    });
    assert_eq!(
        report.checks.provenance.dsse_signature,
        CheckStatus::TrustRootUnavailable
    );
    assert!(!is_clean(&report));
}

#[test]
fn unsupported_predicate_is_unsupported_format() {
    let sk = SigningKey::from_bytes(&[2u8; 32]);
    let (store, key_id) = trust_with(&sk.verifying_key());
    let env = signed_dsse(
        &sk,
        &key_id,
        &statement_json(
            hex_of(ARTIFACT_DIGEST),
            "https://example/other-predicate/v1",
            BUILDER,
        ),
    );
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::Dsse(env),
        pinning: clean_pinning(),
        policy: policy(1),
        trust_store: &store,
    });
    assert_eq!(
        report.checks.provenance.slsa_provenance,
        CheckStatus::UnsupportedFormat
    );
}

#[test]
fn invalid_signature_is_failed_not_trust_root() {
    let sk = SigningKey::from_bytes(&[8u8; 32]);
    let (store, key_id) = trust_with(&sk.verifying_key());
    let mut env = signed_dsse(
        &sk,
        &key_id,
        &statement_json(hex_of(ARTIFACT_DIGEST), SLSA_PROVENANCE_PREDICATE, BUILDER),
    );
    env.signatures[0].signature = BASE64.encode([0u8; 64]);
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::Dsse(env),
        pinning: clean_pinning(),
        policy: policy(1),
        trust_store: &store,
    });
    assert_eq!(report.checks.provenance.dsse_signature, CheckStatus::Failed);
}

#[test]
fn carrier_is_value_free_and_vsa_mappable() {
    let store = crate::trust::TrustStore::new();
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::None,
        pinning: clean_pinning(),
        policy: policy(0),
        trust_store: &store,
    });
    let v = serde_json::to_value(&report).unwrap();
    assert_eq!(v["schema"], SCHEMA);
    assert!(v["subject"]["digest"].is_string());
    assert_eq!(v["declared"]["required_slsa_build_level"], "L0");
    assert_eq!(v["verified"]["slsa_build_level"], "L0");
    assert!(v["policy_result"].is_string());
    assert!(v["non_claims"].as_array().unwrap().len() >= 4);
}

#[test]
fn sigstore_bundle_parse_failure_is_failed_never_verified() {
    let store = crate::trust::TrustStore::new();
    let report = verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::SigstoreBundle(Box::new(SigstoreBundleInput {
            bundle_json: b"not a bundle".to_vec(),
            fulcio_roots: vec![],
            fulcio_intermediates: vec![],
            rekor_trusted_root_json: b"{}".to_vec(),
            now_unix_secs: 1_750_000_000,
            expected_san: "x".to_string(),
            expected_issuer: "y".to_string(),
        })),
        pinning: clean_pinning(),
        policy: policy(0),
        trust_store: &store,
    });
    let p = &report.checks.provenance;
    assert_eq!(p.sigstore_bundle, CheckStatus::Failed);
    assert_eq!(p.cert_chain, CheckStatus::Failed);
    assert_eq!(p.identity, CheckStatus::Failed);
    assert_eq!(p.dsse_pae, CheckStatus::Failed);
    assert_eq!(p.rekor_inclusion, CheckStatus::Failed);
    assert_eq!(p.timestamp_freshness, CheckStatus::NotChecked);
    assert_eq!(p.consistency, CheckStatus::NotChecked);
    assert_eq!(p.witnessing, CheckStatus::NotChecked);
    assert_eq!(report.policy_result, PolicyResult::Fail);
    assert!(!is_clean(&report));
}
