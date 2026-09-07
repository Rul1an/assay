//! Real CLI contract for full signed-archive verification; no live host or key discovery.
#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use assay_evidence::attestation::{
    sign_statement, statement_for_bundle, DsseEnvelope, InTotoStatement,
};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use bounded_process::{run_bounded, ProcessLimits};
use ed25519_dalek::pkcs8::{spki::der::pem::LineEnding, EncodePublicKey};
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::Write;
use std::process::{Command, Output};
use std::time::Duration;

struct Fixture {
    dir: tempfile::TempDir,
    bytes: Vec<u8>,
    signing: SigningKey,
}

impl Fixture {
    fn new(producer_name: String) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let producer = ProducerMeta {
            name: producer_name,
            version: "test".into(),
            git: None,
        };
        let mut writer = BundleWriter::new(File::create(dir.path().join("bundle.tar.gz")).unwrap())
            .with_producer(producer.clone());
        writer.add_event(
            EvidenceEvent::new(
                "assay.test.event",
                "urn:assay:test",
                "verify_run",
                0,
                json!({}),
            )
            .with_producer(&producer),
        );
        writer.finish().unwrap();
        let bytes = fs::read(dir.path().join("bundle.tar.gz")).unwrap();
        fs::write(dir.path().join("bundle.tar.gz"), &bytes).unwrap();
        let signing = SigningKey::from_bytes(&[7; 32]);
        fs::write(
            dir.path().join("key.pem"),
            signing
                .verifying_key()
                .to_public_key_pem(LineEnding::LF)
                .unwrap(),
        )
        .unwrap();
        let f = Self {
            dir,
            bytes,
            signing,
        };
        f.write_statement(statement_for_bundle(&f.bytes).unwrap());
        f
    }
    fn small() -> Self {
        Self::new("assay-cli-test".into())
    }
    fn write_statement(&self, statement: InTotoStatement) {
        let envelope = sign_statement(&statement, &self.signing).unwrap();
        fs::write(
            self.dir.path().join("attestation.json"),
            serde_json::to_vec(&envelope).unwrap(),
        )
        .unwrap();
    }
    fn run(&self) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
        cmd.current_dir(self.dir.path()).args([
            "evidence",
            "verify-attestation",
            "--bundle",
            "bundle.tar.gz",
            "--attestation",
            "attestation.json",
            "--key",
            "key.pem",
        ]);
        for (name, _) in std::env::vars_os() {
            if name
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("ASSAY_")
            {
                cmd.env_remove(name);
            }
        }
        run_bounded(
            cmd,
            &[],
            ProcessLimits::new(Duration::from_secs(60), 4096, 4096),
            "attestation verification CLI",
        )
        .expect("bounded CLI run")
    }
    fn refuse(&self, context: &str) {
        let output = self.run();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stdout.is_empty(),
            "{context}: refusal emitted success JSON"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(context),
            "wrong refusal: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn matching_archive_returns_one_typed_matched_outcome() {
    let f = Fixture::small();
    let output = f.run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let statement = statement_for_bundle(&f.bytes).unwrap();
    assert_eq!(
        result,
        json!({"schema":"assay.evidence.attestation.verify.v1", "outcome":"attestation_verified", "signature_verified":true,"subject_matched":true,"artifact_sha256":statement.subject[0].digest["sha256"],"predicate_type":statement.predicate_type,"subject_name":statement.subject[0].name})
    );
}

#[test]
fn wrong_signature_key_is_refused_without_success_output() {
    let f = Fixture::small();
    let wrong = SigningKey::from_bytes(&[9; 32]);
    fs::write(
        f.dir.path().join("key.pem"),
        wrong
            .verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .unwrap(),
    )
    .unwrap();
    f.refuse("attestation verification failed");
}

#[test]
fn valid_archive_with_other_container_bytes_is_refused() {
    let f = Fixture::small();
    let mut other = f.bytes.clone();
    other.extend_from_slice(b"trailing");
    // Prove the witness passes ordinary bundle validation before testing artifact matching.
    statement_for_bundle(&other).expect("changed container is still a valid bundle");
    fs::write(f.dir.path().join("bundle.tar.gz"), other).unwrap();
    f.refuse("attestation verification failed");
}

#[test]
fn signed_legacy_unknown_version_and_derived_field_mismatch_are_refused() {
    let f = Fixture::small();
    for variant in ["legacy", "unknown", "schema", "derived"] {
        let mut statement = statement_for_bundle(&f.bytes).unwrap();
        match variant {
            "legacy" => {
                statement.predicate_type = "https://assay.dev/attestation/evidence-bundle/v0".into()
            }
            "unknown" => {
                statement.predicate_type =
                    "https://docs.getassay.dev/attestation/evidence-bundle/v999".into()
            }
            "schema" => statement.predicate["schema_version"] = json!(999),
            _ => statement.predicate["run"]["run_id"] = json!("different-run"),
        }
        f.write_statement(statement);
        f.refuse("attestation verification failed");
    }
}

#[test]
fn malformed_outer_json_is_refused() {
    let f = Fixture::small();
    fs::write(f.dir.path().join("attestation.json"), "{").unwrap();
    f.refuse("invalid DSSE envelope");
}

#[test]
fn trailing_outer_json_is_refused() {
    let f = Fixture::small();
    let path = f.dir.path().join("attestation.json");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(path, format!("{original} false")).unwrap();
    f.refuse("invalid DSSE envelope");
}

#[test]
fn duplicate_known_outer_field_is_refused_even_if_last_value_is_valid() {
    let f = Fixture::small();
    let path = f.dir.path().join("attestation.json");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        original.replacen('{', "{\"payload\":\"duplicate\",", 1),
    )
    .unwrap();
    f.refuse("invalid DSSE envelope");
}

#[test]
fn deeply_nested_unknown_outer_field_is_refused_on_an_otherwise_valid_envelope() {
    let f = Fixture::small();
    let path = f.dir.path().join("attestation.json");
    let original = fs::read_to_string(&path).unwrap();
    let with_depth = |depth| {
        original.replacen(
            '{',
            &format!("{{\"extra\":{}0{},", "[".repeat(depth), "]".repeat(depth)),
            1,
        )
    };
    fs::write(&path, with_depth(2)).unwrap();
    assert!(
        f.run().status.success(),
        "a shallow unknown field is compatible"
    );
    fs::write(path, with_depth(140)).unwrap();
    f.refuse("invalid DSSE envelope");
}

#[test]
fn base64_payload_larger_than_one_mib_keeps_valid_statement_compatibility() {
    let f = Fixture::new("x".repeat(800_000));
    let raw = fs::read(f.dir.path().join("attestation.json")).unwrap();
    let envelope: DsseEnvelope = serde_json::from_slice(&raw).unwrap();
    assert!(
        envelope.payload.len() > 1024 * 1024,
        "witness must cross outer strict-string ceiling"
    );
    let output = f.run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn envelope_ceiling_is_inclusive_and_next_byte_is_refused() {
    let f = Fixture::small();
    let path = f.dir.path().join("attestation.json");
    let mut raw = fs::read(&path).unwrap();
    raw.resize(32 * 1024 * 1024, b' ');
    fs::write(&path, &raw).unwrap();
    assert!(
        f.run().status.success(),
        "valid JSON at the input ceiling must be accepted"
    );
    File::options()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b" ")
        .unwrap();
    f.refuse("read attestation within input limit");
}

#[test]
fn public_key_source_ceiling_refuses_before_pem_decoding() {
    let f = Fixture::small();
    File::create(f.dir.path().join("key.pem"))
        .unwrap()
        .set_len(16 * 1024 + 1)
        .unwrap();
    f.refuse("read public key within input limit");
}

#[test]
fn bundle_source_ceiling_refuses_before_archive_decoding() {
    let f = Fixture::small();
    File::create(f.dir.path().join("bundle.tar.gz"))
        .unwrap()
        .set_len(100 * 1024 * 1024 + 1)
        .unwrap();
    f.refuse("read bundle within input limit");
}

#[test]
fn private_key_is_not_a_public_key_fallback() {
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    let f = Fixture::small();
    fs::write(
        f.dir.path().join("key.pem"),
        f.signing.to_pkcs8_pem(LineEnding::LF).unwrap().as_bytes(),
    )
    .unwrap();
    f.refuse("invalid Ed25519 public PEM");
}

#[test]
fn attacker_values_are_not_echoed_through_library_error_chains() {
    let f = Fixture::small();
    let mut envelope: Value =
        serde_json::from_slice(&fs::read(f.dir.path().join("attestation.json")).unwrap()).unwrap();
    envelope["payloadType"] = json!("ATTACKER_SECRET_2831");
    fs::write(
        f.dir.path().join("attestation.json"),
        serde_json::to_vec(&envelope).unwrap(),
    )
    .unwrap();
    let output = f.run();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("ATTACKER_SECRET_2831"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("attestation verification failed"));
}
