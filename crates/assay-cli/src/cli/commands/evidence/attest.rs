//! `assay evidence attest` — sign a bundle's manifest as an in-toto/DSSE attestation.
//!
//! Wraps `assay_evidence::attestation` (ADR-039): opens and verifies an evidence
//! bundle, builds an in-toto v1 Statement over its integrity root, and signs it
//! as a DSSE envelope with an Ed25519 key (PKCS#8 PEM, as produced by
//! `assay mcp tool keygen`). The anchor (transparency log / timestamp) stays
//! external. Attestation binds who-said-it and the bundle content; it does not
//! upgrade observed support.

use anyhow::{Context, Result};
use assay_evidence::attestation::{sign_statement, statement_from_manifest};
use assay_evidence::bundle::BundleReader;
use clap::Args;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::SigningKey;
use std::fs::File;
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
    // 1. Open + verify the bundle, take its manifest.
    let file = File::open(&args.bundle)
        .with_context(|| format!("open bundle {}", args.bundle.display()))?;
    let reader = BundleReader::open(file).context("verify/open bundle")?;
    let manifest = reader.manifest().clone();

    // 2. Load the Ed25519 signing key (PKCS#8 PEM).
    let pem = std::fs::read_to_string(&args.key)
        .with_context(|| format!("read key {}", args.key.display()))?;
    let key = SigningKey::from_pkcs8_pem(&pem).context("parse Ed25519 PKCS#8 PEM key")?;

    // 3. Predicate: from file, or a minimal default summarizing the bundle.
    let predicate = match &args.predicate {
        Some(p) => {
            let raw = std::fs::read_to_string(p)
                .with_context(|| format!("read predicate {}", p.display()))?;
            serde_json::from_str(&raw).context("parse predicate JSON")?
        }
        None => serde_json::json!({
            "run_id": manifest.run_id,
            "event_count": manifest.event_count,
        }),
    };

    // 4. Build + sign the in-toto statement.
    let statement = statement_from_manifest(&manifest, predicate);
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
        let statement = verify_envelope(&envelope, &signing.verifying_key()).expect("verify");
        assert_eq!(statement.type_, "https://in-toto.io/Statement/v1");

        std::fs::remove_dir_all(&dir).ok();
    }
}
