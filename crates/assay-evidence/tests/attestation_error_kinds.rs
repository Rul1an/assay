//! Machine-readable refusal kinds of the canonical artifact attestation verifier (#2497).
//!
//! Every witness is a real refusal of the existing verifier. The kind must name the first check
//! that fails in the verifier's fixed order, the existing top-level message must not change, and
//! typed causes that were recoverable before must stay recoverable.
use anyhow::Context as _;
use assay_evidence::attestation::{
    attestation_error_kind, sign_statement, statement_for_bundle, verify_attestation_for_bundle,
    verify_attestation_for_bundle_with_extent,
    verify_attestation_for_bundle_with_extent_and_limits,
    verify_attestation_for_bundle_with_limits, AttestationErrorKind, DsseEnvelope, DsseSignature,
    InTotoStatement, EVIDENCE_BUNDLE_PREDICATE_TYPE_V0, EVIDENCE_BUNDLE_PREDICATE_TYPE_V1,
};
use assay_evidence::bundle::{BundleWriter, VerifyLimits};
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use assay_evidence::{ErrorClass, ErrorCode, VerifyError};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signer as _, SigningKey};
use serde_json::{json, Value};

use AttestationErrorKind as K;

const IN_TOTO: &str = "application/vnd.in-toto+json";

fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn other_key() -> SigningKey {
    SigningKey::from_bytes(&[9; 32])
}

fn bundle_with(run_id: &str, event_types: impl IntoIterator<Item = String>) -> Vec<u8> {
    let producer = ProducerMeta {
        name: "error-kinds-test".into(),
        version: "0.0.0".into(),
        git: None,
    };
    let mut out = Vec::new();
    let mut writer = BundleWriter::new(&mut out).with_producer(producer.clone());
    for (seq, type_) in event_types.into_iter().enumerate() {
        writer.add_event(
            EvidenceEvent::new(
                type_,
                "urn:assay:error-kinds",
                run_id,
                seq as u64,
                json!({}),
            )
            .with_producer(&producer)
            .with_time("2026-10-16T00:00:00Z".parse().unwrap()),
        );
    }
    writer.finish().expect("write test bundle");
    out
}

fn bundle(run_id: &str) -> Vec<u8> {
    bundle_with(run_id, ["assay.test.event".to_string()])
}

fn statement(bytes: &[u8]) -> InTotoStatement {
    statement_for_bundle(bytes).expect("producer accepts the test bundle")
}

fn signed(statement: &InTotoStatement) -> DsseEnvelope {
    sign_statement(statement, &key()).expect("sign test statement")
}

/// Sign arbitrary payload bytes, to reach checks a well-formed Statement never reaches.
fn signed_raw(payload: &[u8]) -> DsseEnvelope {
    let pae = assay_common::dsse::build_pae(IN_TOTO, payload);
    let sig = key().sign(&pae);
    DsseEnvelope {
        payload: STANDARD.encode(payload),
        payload_type: IN_TOTO.to_string(),
        signatures: vec![DsseSignature {
            keyid: "error-kinds-test".to_string(),
            sig: STANDARD.encode(sig.to_bytes()),
        }],
    }
}

fn refuse(envelope: &DsseEnvelope, bytes: &[u8]) -> anyhow::Error {
    verify_attestation_for_bundle(envelope, &key().verifying_key(), bytes)
        .expect_err("the verifier must refuse this witness")
}

fn refuse_with_limits(
    envelope: &DsseEnvelope,
    bytes: &[u8],
    limits: VerifyLimits,
) -> anyhow::Error {
    verify_attestation_for_bundle_with_limits(envelope, &key().verifying_key(), bytes, limits)
        .expect_err("the verifier must refuse this witness")
}

fn nested(depth: usize) -> Value {
    (0..depth).fold(json!(0), |inner, _| json!([inner]))
}

fn tiny_bundle_limit() -> VerifyLimits {
    VerifyLimits {
        max_bundle_bytes: 16,
        ..VerifyLimits::default()
    }
}

struct Witness {
    label: &'static str,
    error: anyhow::Error,
    /// The existing top-level message, pinned as it is before classification.
    display: String,
    kind: AttestationErrorKind,
}

/// One witness per kind and per distinct refusal site, in the verifier's check order.
fn witnesses() -> Vec<Witness> {
    let a = bundle("error-kinds-a");
    let b = bundle("error-kinds-b");
    let good = statement(&a);
    let mut out = Vec::new();
    let mut push = |label, error, display: &str, kind| {
        out.push(Witness {
            label,
            error,
            display: display.to_string(),
            kind,
        })
    };

    // 1. payloadType (before any signature work).
    let mut env = signed(&good);
    env.payload_type = "application/vnd.example+json".into();
    push(
        "payloadType",
        refuse(&env, &a),
        "unexpected DSSE payloadType: expected application/vnd.in-toto+json, got application/vnd.example+json",
        K::Unsupported,
    );

    // 2-4. Envelope encoding and signature shape.
    let mut env = signed(&good);
    env.payload = "%%% not base64 %%%".into();
    push(
        "payload base64",
        refuse(&env, &a),
        "decode dsse payload",
        K::Malformed,
    );

    let mut env = signed(&good);
    env.signatures.clear();
    push(
        "no signature",
        refuse(&env, &a),
        "dsse envelope has no signatures",
        K::Malformed,
    );

    let mut env = signed(&good);
    env.signatures[0].sig = "%%%".into();
    push(
        "signature base64",
        refuse(&env, &a),
        "decode signature",
        K::Malformed,
    );

    let mut env = signed(&good);
    env.signatures[0].sig = STANDARD.encode([0u8; 10]);
    push(
        "signature length",
        refuse(&env, &a),
        "signature is not 64 bytes",
        K::Malformed,
    );

    // 5. The Ed25519 check itself.
    let env = sign_statement(&good, &other_key()).unwrap();
    push(
        "wrong key",
        refuse(&env, &a),
        "dsse signature verification failed",
        K::SignatureInvalid,
    );

    // 6. Signed payload bytes.
    push(
        "payload not UTF-8",
        refuse(&signed_raw(&[0xff, 0xfe]), &a),
        "signed payload is not UTF-8",
        K::Malformed,
    );
    push(
        "duplicate member",
        refuse(&signed_raw(br#"{"_type":"a","_type":"b"}"#), &a),
        "signed payload is not unambiguous JSON: it contains a duplicate object member",
        K::Malformed,
    );
    let mut deep = good.clone();
    deep.predicate["nested"] = nested(80);
    push(
        "strict nesting budget",
        refuse(&signed(&deep), &a),
        "signed payload is not unambiguous JSON: it nests deeper than the parser accepts",
        K::LimitExceeded,
    );

    // 7. Statement type.
    let mut st = good.clone();
    st.type_ = "https://example.invalid/Statement/v9".into();
    push(
        "statement type",
        refuse(&signed(&st), &a),
        "unexpected in-toto statement _type: expected https://in-toto.io/Statement/v1, got https://example.invalid/Statement/v9",
        K::Unsupported,
    );

    // 8. Predicate type.
    let mut st = good.clone();
    st.predicate_type = EVIDENCE_BUNDLE_PREDICATE_TYPE_V0.into();
    push(
        "legacy predicate",
        refuse(&signed(&st), &a),
        &format!(
            "attestation uses the pre-ADR-044 predicate {EVIDENCE_BUNDLE_PREDICATE_TYPE_V0}, whose subject digest identifies no artifact; it cannot be matched and must be reissued"
        ),
        K::Unsupported,
    );
    let mut st = good.clone();
    st.predicate_type = "https://example.invalid/predicate/v9".into();
    push(
        "unknown predicate",
        refuse(&signed(&st), &a),
        &format!(
            "unknown predicate type: expected {EVIDENCE_BUNDLE_PREDICATE_TYPE_V1}. An unrecognised major version fails closed rather than being reported as verified"
        ),
        K::Unsupported,
    );

    // 9. Subject shape.
    let mut st = good.clone();
    let extra = st.subject[0].clone();
    st.subject.push(extra);
    push(
        "two subjects",
        refuse(&signed(&st), &a),
        "expected exactly one subject, found 2",
        K::Malformed,
    );

    // 10. Attested extent: shape, and its key-bytes budget.
    let mut st = good.clone();
    st.predicate["extent"] = json!("not an object");
    push(
        "extent shape",
        refuse(&signed(&st), &a),
        "invalid extent: expected object",
        K::Malformed,
    );
    // Two keys under the strict string ceiling whose total exceeds the histogram key-bytes budget.
    let mut wide_keys = serde_json::Map::new();
    wide_keys.insert("a".repeat(600_000), json!(1));
    wide_keys.insert("b".repeat(600_000), json!(1));
    let mut st = good.clone();
    st.predicate["extent"] = json!({
        "retained_events_by_type": Value::Object(wide_keys),
        "observed": {"basis": "not_stated"},
    });
    push(
        "attested extent key-bytes budget",
        refuse(&signed(&st), &a),
        "extent.retained_events_by_type key bytes limit",
        K::LimitExceeded,
    );

    // 11. Predicate schema version.
    let mut st = good.clone();
    st.predicate["schema_version"] = json!(2);
    push(
        "schema version",
        refuse(&signed(&st), &a),
        "predicate schema_version 2 is not 1",
        K::Unsupported,
    );

    // 12. Bundle verification: a budget, a derived-extent budget, and any other refusal.
    push(
        "bundle budget",
        refuse_with_limits(&signed(&good), &a, tiny_bundle_limit()),
        "the attested bundle does not verify",
        K::LimitExceeded,
    );
    let wide = bundle_with(
        "error-kinds-wide",
        (0..10_001).map(|i| format!("assay.test.kind{i}")),
    );
    let mut st = statement(&wide);
    st.predicate["extent"] = json!({
        "retained_events_by_type": {},
        "observed": {"basis": "not_stated"},
    });
    push(
        "derived extent key-count budget",
        refuse(&signed(&st), &wide),
        "the attested bundle does not verify",
        K::LimitExceeded,
    );
    push(
        "bundle rejected",
        refuse(&signed(&good), b"not a bundle"),
        "the attested bundle does not verify",
        K::BundleRejected,
    );

    // 13-14. Artifact identity.
    push(
        "artifact digest",
        refuse(&signed(&good), &b),
        "the subject digest does not match the sha256 of the bundle offered",
        K::ArtifactMismatch,
    );
    // Same valid bundle, correct digest; only the subject name is wrong.
    let mut st = good.clone();
    st.subject[0].name = "not-the-attested-bundle-id".into();
    push(
        "subject name",
        refuse(&signed(&st), &a),
        "the subject name is not the attested bundle's bundle_id",
        K::ArtifactMismatch,
    );

    // 15-16. Signed claims against the artifact.
    let mut st = good.clone();
    st.predicate["run"]["event_count"] = json!(2);
    push(
        "derived predicate field",
        refuse(&signed(&st), &a),
        "predicate field `run.event_count` disagrees with the bundle it is attached to",
        K::PredicateMismatch,
    );
    let mut st = good.clone();
    st.predicate["extent"] = json!({
        "retained_events_by_type": {"assay.test.event": 2},
        "observed": {"basis": "not_stated"},
    });
    push(
        "extent comparison",
        refuse(&signed(&st), &a),
        "predicate field `extent.retained_events_by_type` disagrees with the bundle it is attached to",
        K::PredicateMismatch,
    );

    out
}

/// Positive control: classification must not change any existing top-level message.
#[test]
fn existing_refusal_messages_are_unchanged() {
    for w in witnesses() {
        assert_eq!(w.error.to_string(), w.display, "{}", w.label);
    }
}

#[test]
fn each_refusal_names_the_kind_of_its_failing_check() {
    for w in witnesses() {
        assert_eq!(
            attestation_error_kind(&w.error),
            Some(w.kind),
            "{}: {}",
            w.label,
            w.error
        );
    }
}

/// Two defects at once: the kind is the first failing check in the existing order.
#[test]
fn first_failing_check_decides_mixed_defects() {
    let a = bundle("error-kinds-a");
    let b = bundle("error-kinds-b");
    let good = statement(&a);

    // Bad signature over an unsupported predicate: the signature check runs first.
    let mut st = good.clone();
    st.predicate_type = "https://example.invalid/predicate/v9".into();
    let env = sign_statement(&st, &other_key()).unwrap();
    assert_eq!(
        attestation_error_kind(&refuse(&env, &a)),
        Some(K::SignatureInvalid)
    );

    // Unsupported payloadType over a bad signature: payloadType is checked first.
    let mut env = sign_statement(&good, &other_key()).unwrap();
    env.payload_type = "application/vnd.example+json".into();
    assert_eq!(
        attestation_error_kind(&refuse(&env, &a)),
        Some(K::Unsupported)
    );

    // Malformed extent beside an unsupported schema version: extent is parsed first.
    let mut st = good.clone();
    st.predicate["extent"] = json!("not an object");
    st.predicate["schema_version"] = json!(2);
    assert_eq!(
        attestation_error_kind(&refuse(&signed(&st), &a)),
        Some(K::Malformed)
    );

    // A bundle budget, or any bundle refusal, beside an artifact mismatch: bundle first.
    assert_eq!(
        attestation_error_kind(&refuse_with_limits(&signed(&good), &b, tiny_bundle_limit())),
        Some(K::LimitExceeded)
    );
    assert_eq!(
        attestation_error_kind(&refuse(&signed(&good), b"not a bundle")),
        Some(K::BundleRejected)
    );
}

/// Positive control: a typed bundle cause recoverable today stays recoverable, by the same chain
/// walk the CLI's evidence reason mapping uses.
#[test]
fn bundle_verify_error_stays_recoverable() {
    let a = bundle("error-kinds-a");
    let error = refuse_with_limits(&signed(&statement(&a)), &a, tiny_bundle_limit());
    let verify = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<VerifyError>())
        .expect("the bundle budget refusal carries its VerifyError");
    assert_eq!(verify.class, ErrorClass::Limits);
    assert_eq!(verify.code, ErrorCode::LimitBundleBytes);
}

#[test]
fn caller_context_and_every_wrapper_keep_the_kind() {
    let a = bundle("error-kinds-a");
    let b = bundle("error-kinds-b");
    let good = statement(&a);
    let wrong_key = sign_statement(&good, &other_key()).unwrap();
    let vk = key().verifying_key();
    for (envelope, bytes, kind) in [
        (&wrong_key, &a, K::SignatureInvalid),
        (&signed(&good), &b, K::ArtifactMismatch),
    ] {
        let results = [
            verify_attestation_for_bundle(envelope, &vk, bytes).map(|_| ()),
            verify_attestation_for_bundle_with_limits(
                envelope,
                &vk,
                bytes,
                VerifyLimits::default(),
            )
            .map(|_| ()),
            verify_attestation_for_bundle_with_extent(envelope, &vk, bytes).map(|_| ()),
            verify_attestation_for_bundle_with_extent_and_limits(
                envelope,
                &vk,
                bytes,
                VerifyLimits::default(),
            )
            .map(|_| ()),
        ];
        for result in results {
            let error = result.expect_err("every wrapper refuses");
            assert_eq!(attestation_error_kind(&error), Some(kind));
            let wrapped = Err::<(), _>(error)
                .context("caller-added context")
                .unwrap_err();
            assert_eq!(attestation_error_kind(&wrapped), Some(kind));
        }
    }
}

/// Positive control: no kind for foreign errors, and success is not a refusal.
#[test]
fn unclassified_errors_have_no_kind_and_success_is_unchanged() {
    assert_eq!(
        attestation_error_kind(&anyhow::anyhow!("foreign error")),
        None
    );
    let a = bundle("error-kinds-a");
    verify_attestation_for_bundle(&signed(&statement(&a)), &key().verifying_key(), &a)
        .expect("a valid attestation still verifies");
}

#[test]
fn kinds_carry_no_input_values() {
    let a = bundle("error-kinds-a");
    let mut env = signed(&statement(&a));
    env.payload_type = "ATTACKER_SECRET_2497".into();
    let kind =
        attestation_error_kind(&refuse(&env, &a)).expect("payloadType refusal is classified");
    assert_eq!(kind, K::Unsupported);
    assert!(!format!("{kind:?}").contains("ATTACKER_SECRET_2497"));
}
