//! Full artifact-attestation verification with bounded local input and explicit outcome.
use crate::output_write::write_stdout_json;
use anyhow::{anyhow, Result};
use assay_common::limits::{LimitKind, LimitReader};
use assay_evidence::attestation::{verify_attestation_for_bundle_with_limits, DsseEnvelope};
use assay_evidence::VerifyLimits;
use clap::Args;
use ed25519_dalek::pkcs8::DecodePublicKey;
use ed25519_dalek::VerifyingKey;
use serde::Serialize;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_ENVELOPE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PUBLIC_KEY_BYTES: u64 = 16 * 1024;

#[derive(Debug, Args, Clone)]
pub struct VerifyAttestationArgs {
    /// Completed evidence archive (.tar.gz) to match.
    #[arg(long)]
    pub bundle: PathBuf,
    /// DSSE envelope containing the signed v1 Statement.
    #[arg(long)]
    pub attestation: PathBuf,
    /// Explicit Ed25519 SPKI public PEM; no key discovery or private-key fallback.
    #[arg(long)]
    pub key: PathBuf,
}

#[derive(Serialize)]
struct AttestationVerificationReport {
    schema: &'static str,
    outcome: &'static str,
    signature_verified: bool,
    subject_matched: bool,
    artifact_sha256: String,
    predicate_type: String,
    subject_name: String,
}

/// Every input uses the same stream ceiling. Error context is static: underlying file,
/// parser and verification errors can contain paths or attacker-controlled values.
fn read_bounded(path: &Path, limit: u64, context: &'static str) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|_| anyhow!(context))?;
    let mut bytes = Vec::new();
    LimitReader::new(file, limit, LimitKind::SourceBytes)
        .read_to_end(&mut bytes)
        .map_err(|_| anyhow!(context))?;
    Ok(bytes)
}

pub fn cmd_verify_attestation(args: VerifyAttestationArgs) -> Result<i32> {
    let limits = VerifyLimits::default();
    let bundle = read_bounded(
        &args.bundle,
        limits.max_bundle_bytes,
        "read bundle within input limit",
    )?;
    let encoded = read_bounded(
        &args.attestation,
        MAX_ENVELOPE_BYTES,
        "read attestation within input limit",
    )?;
    let pem = read_bounded(
        &args.key,
        MAX_PUBLIC_KEY_BYTES,
        "read public key within input limit",
    )?;

    // Typed deserialization skips unknown members without applying its recursion counter
    // to their values. Check syntax/depth first, drop that allocation, then parse ORIGINAL
    // bytes so duplicate known fields cannot disappear through Value's object map.
    drop(
        serde_json::from_slice::<serde_json::Value>(&encoded)
            .map_err(|_| anyhow!("invalid DSSE envelope"))?,
    );
    let envelope: DsseEnvelope =
        serde_json::from_slice(&encoded).map_err(|_| anyhow!("invalid DSSE envelope"))?;
    let pem = std::str::from_utf8(&pem).map_err(|_| anyhow!("invalid Ed25519 public PEM"))?;
    let key = VerifyingKey::from_public_key_pem(pem)
        .map_err(|_| anyhow!("invalid Ed25519 public PEM"))?;

    // This is the one canonical full verification path. A checked signature alone is
    // artifact-unmatched and must never become this command's success report.
    let verified = verify_attestation_for_bundle_with_limits(&envelope, &key, &bundle, limits)
        .map_err(|_| anyhow!("attestation verification failed"))?;
    let report = AttestationVerificationReport {
        schema: "assay.evidence.attestation.verify.v1",
        outcome: "attestation_verified",
        signature_verified: true,
        subject_matched: true,
        artifact_sha256: verified.artifact_sha256,
        predicate_type: verified.statement.predicate_type,
        subject_name: verified.statement.subject[0].name.clone(),
    };
    let json = serde_json::to_string(&report)
        .map_err(|_| anyhow!("serialize attestation verification report"))?;
    Ok(write_stdout_json(&json))
}
