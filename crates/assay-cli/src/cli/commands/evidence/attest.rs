//! `assay evidence attest` — sign a bundle's manifest as an in-toto/DSSE attestation.
//!
//! Wraps `assay_evidence::attestation` (ADR-039): opens and verifies an evidence
//! bundle, builds an in-toto v1 Statement over its integrity root, and signs it
//! as a DSSE envelope with an Ed25519 key (PKCS#8 PEM, as produced by
//! `assay mcp tool keygen`). The anchor (transparency log / timestamp) stays
//! external. Attestation binds who-said-it and the bundle content; it does not
//! upgrade observed support.

use anyhow::{Context, Result};
use assay_evidence::attestation::{predicate_from_verified, sign_statement, statement_from_bundle};
use clap::Args;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::SigningKey;
use std::path::PathBuf;

#[derive(Debug, Args, Clone)]
pub struct AttestArgs {
    /// Path to the evidence bundle (.tar.gz) to attest.
    #[arg(long)]
    pub bundle: PathBuf,
    /// Path to the Ed25519 private key (PKCS#8 PEM; see `assay mcp tool keygen`).
    #[arg(long)]
    pub key: PathBuf,
    /// Optional JSON file used as the attestation predicate (default: a minimal summary).
    #[arg(long)]
    pub predicate: Option<PathBuf>,
    /// Write the DSSE envelope here (default: stdout).
    #[arg(long)]
    pub out: Option<PathBuf>,
}

pub fn cmd_attest(args: AttestArgs) -> Result<i32> {
    run(args)?;
    Ok(0)
}

fn run(args: AttestArgs) -> Result<()> {
    // 1. Read the bundle bytes, then verify them. ADR-044 makes the subject the digest of these
    //    exact bytes, so the attestation is built from what a consumer will hold rather than from
    //    a manifest that cannot identify it.
    let bundle_bytes = std::fs::read(&args.bundle)
        .with_context(|| format!("read bundle {}", args.bundle.display()))?;
    let verified =
        assay_evidence::bundle::verify_bundle(bundle_bytes.as_slice()).context("verify bundle")?;

    // 2. Load the Ed25519 signing key (PKCS#8 PEM).
    let pem = std::fs::read_to_string(&args.key)
        .with_context(|| format!("read key {}", args.key.display()))?;
    let key = SigningKey::from_pkcs8_pem(&pem).context("parse Ed25519 PKCS#8 PEM key")?;

    // 3. Predicate v1, derived from the verified bundle. No longer caller-supplied: every field
    //    must be derivable from the artifact so a consumer can cross-check it rather than trust it,
    //    and a predicate the caller writes by hand cannot offer that. `--predicate` is refused
    //    rather than silently ignored.
    if args.predicate.is_some() {
        anyhow::bail!(
            "--predicate is not accepted for evidence-bundle/v1: every field is derived from the \
             bundle so a consumer can cross-check it against the artifact (ADR-044)"
        );
    }
    let predicate = predicate_from_verified(&verified)?;

    // 4. Build + sign the in-toto statement over the bundle bytes.
    let statement = statement_from_bundle(&bundle_bytes, &predicate)?;
    let envelope = sign_statement(&statement, &key).context("sign in-toto statement")?;
    let json = serde_json::to_string_pretty(&envelope).context("serialize DSSE envelope")?;

    // 5. Write the DSSE envelope.
    match &args.out {
        Some(p) => {
            std::fs::write(p, format!("{json}\n"))
                .with_context(|| format!("write {}", p.display()))?;
            eprintln!("Attestation: {}", p.display());
        }
        None => println!("{json}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_evidence::attestation::{verify_envelope, DsseEnvelope};
    use assay_evidence::bundle::BundleWriter;
    use assay_evidence::types::{EvidenceEvent, ProducerMeta};
    use ed25519_dalek::pkcs8::{spki::der::pem::LineEnding, EncodePrivateKey};
    use std::fs::File;

    #[test]
    fn attest_produces_a_verifiable_envelope() {
        let dir = std::env::temp_dir().join(format!("assay-attest-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bundle_path = dir.join("bundle.tar.gz");
        let key_path = dir.join("private_key.pem");
        let out_path = dir.join("attestation.json");

        // Write a small bundle.
        let producer = ProducerMeta {
            name: "assay-cli".into(),
            version: "test".into(),
            git: None,
        };
        let file = File::create(&bundle_path).unwrap();
        let mut writer = BundleWriter::new(file).with_producer(producer.clone());
        writer.add_event(
            EvidenceEvent::new(
                "assay.test.event",
                "urn:assay:test",
                "attest_run",
                0,
                serde_json::json!({}),
            )
            .with_producer(&producer),
        );
        writer.finish().unwrap();

        // Write a key.
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        std::fs::write(
            &key_path,
            signing.to_pkcs8_pem(LineEnding::LF).unwrap().as_bytes(),
        )
        .unwrap();

        // Attest.
        let bundle_path_for_verify = bundle_path.clone();
        run(AttestArgs {
            bundle: bundle_path,
            key: key_path,
            predicate: None,
            out: Some(out_path.clone()),
        })
        .expect("attest");

        // The produced envelope verifies under the signer's public key.
        let raw = std::fs::read_to_string(&out_path).unwrap();
        let envelope: DsseEnvelope = serde_json::from_str(&raw).unwrap();
        // Signature alone is a state, not a verdict: it says who signed, not which artifact.
        let signed = verify_envelope(&envelope, &signing.verifying_key()).expect("verify");
        assert_eq!(signed.statement.type_, "https://in-toto.io/Statement/v1");

        // The verdict needs the bytes. This is the check that did not exist before ADR-044 --
        // `subject` was read nowhere, so a forged bundle survived because nothing was matched.
        let bytes = std::fs::read(&bundle_path_for_verify).expect("read bundle");
        let attested = assay_evidence::attestation::verify_attestation_for_bundle(
            &envelope,
            &signing.verifying_key(),
            &bytes,
        )
        .expect("attestation must verify against the bundle it describes");
        assert_eq!(attested.predicate.run.event_count, 1);

        // A different artifact must not match, even with a valid signature over a real statement.
        let mut other = bytes.clone();
        other.extend_from_slice(b"trailing");
        let err = assay_evidence::attestation::verify_attestation_for_bundle(
            &envelope,
            &signing.verifying_key(),
            &other,
        )
        .expect_err("a different artifact must not match");
        assert!(
            err.to_string().contains("does not match"),
            "expected a subject-digest mismatch, got: {err}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
