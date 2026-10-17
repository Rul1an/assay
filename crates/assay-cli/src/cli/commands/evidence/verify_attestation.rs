//! Full artifact-attestation verification with bounded local input and explicit outcome.
use crate::exit_codes::EXIT_SUCCESS;
use crate::output_write::{map_write_result, write_stdout_json};
use anyhow::{anyhow, Result};
use assay_common::limits::{LimitExceeded, LimitKind, LimitReader};
use assay_evidence::attestation::{
    attestation_error_kind, verify_attestation_for_bundle_with_extent_and_limits,
    AttestationErrorKind, DsseEnvelope, EvidenceExtent,
};
use assay_evidence::VerifyLimits;
use clap::Args;
use ed25519_dalek::pkcs8::DecodePublicKey;
use ed25519_dalek::VerifyingKey;
use serde::Serialize;
use std::fs::File;
use std::io::{self, Read, Write};
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
    /// Emit command refusals as JSON after CLI argument parsing.
    #[arg(long)]
    pub errors_json: bool,
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
    extent_stated: bool,
    extent: Option<EvidenceExtent>,
}

// Only value-free classification and static diagnostics cross the CLI boundary.
struct Refusal {
    kind: &'static str,
    message: &'static str,
}

#[derive(Serialize)]
struct AttestationRefusalReport {
    schema: &'static str,
    outcome: &'static str,
    error_kind: &'static str,
}

fn verification_kind(error: &anyhow::Error) -> &'static str {
    kind_name(attestation_error_kind(error))
}

fn kind_name(kind: Option<AttestationErrorKind>) -> &'static str {
    match kind {
        Some(AttestationErrorKind::Malformed) => "malformed",
        Some(AttestationErrorKind::Unsupported) => "unsupported",
        Some(AttestationErrorKind::SignatureInvalid) => "signature_invalid",
        Some(AttestationErrorKind::LimitExceeded) => "limit_exceeded",
        Some(AttestationErrorKind::BundleRejected) => "bundle_rejected",
        Some(AttestationErrorKind::ArtifactMismatch) => "artifact_mismatch",
        Some(AttestationErrorKind::PredicateMismatch) => "predicate_mismatch",
        // The library enum is non-exhaustive. Untagged/future errors remain refusals.
        _ => "unclassified",
    }
}

fn read_kind(error: &io::Error) -> &'static str {
    if LimitExceeded::from_io(error).is_some() {
        "limit_exceeded"
    } else {
        "input_unreadable"
    }
}

fn emit_refusal(refusal: Refusal) -> i32 {
    let report = AttestationRefusalReport {
        schema: "assay.evidence.attestation.refusal.v1",
        outcome: "attestation_refused",
        error_kind: refusal.kind,
    };
    let json = match serde_json::to_string(&report) {
        Ok(json) => json,
        Err(_) => {
            return map_write_result(
                "stdout",
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "serialize attestation refusal report",
                )),
            )
        }
    };
    let code = write_stdout_json(&json);
    if code != EXIT_SUCCESS {
        return code;
    }
    // Do not return Err to main's eprintln path: closed stderr must not panic
    // after the requested machine document has been delivered.
    let _ = writeln!(io::stderr().lock(), "fatal: {}", refusal.message);
    2
}

/// Classify before erasing causes that may contain paths or attacker-controlled values.
fn read_bounded(
    path: &Path,
    limit: u64,
    context: &'static str,
) -> std::result::Result<Vec<u8>, Refusal> {
    let file = File::open(path).map_err(|_| Refusal {
        kind: "input_unreadable",
        message: context,
    })?;
    let mut bytes = Vec::new();
    LimitReader::new(file, limit, LimitKind::SourceBytes)
        .read_to_end(&mut bytes)
        .map_err(|error| Refusal {
            kind: read_kind(&error),
            message: context,
        })?;
    Ok(bytes)
}

fn verify(
    args: &VerifyAttestationArgs,
) -> std::result::Result<AttestationVerificationReport, Refusal> {
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
        serde_json::from_slice::<serde_json::Value>(&encoded).map_err(|_| Refusal {
            kind: "malformed",
            message: "invalid DSSE envelope",
        })?,
    );
    let envelope: DsseEnvelope = serde_json::from_slice(&encoded).map_err(|_| Refusal {
        kind: "malformed",
        message: "invalid DSSE envelope",
    })?;
    let pem = std::str::from_utf8(&pem).map_err(|_| Refusal {
        kind: "key_invalid",
        message: "invalid Ed25519 public PEM",
    })?;
    let key = VerifyingKey::from_public_key_pem(pem).map_err(|_| Refusal {
        kind: "key_invalid",
        message: "invalid Ed25519 public PEM",
    })?;

    // This is the one canonical full verification path. A checked signature alone is
    // artifact-unmatched and must never become this command's success report.
    let checked =
        verify_attestation_for_bundle_with_extent_and_limits(&envelope, &key, &bundle, limits)
            .map_err(|error| Refusal {
                kind: verification_kind(&error),
                message: "attestation verification failed",
            })?;
    let (verified, extent) = checked.into_parts();
    Ok(AttestationVerificationReport {
        schema: "assay.evidence.attestation.verify.v1",
        outcome: "attestation_verified",
        signature_verified: true,
        subject_matched: true,
        artifact_sha256: verified.artifact_sha256,
        predicate_type: verified.statement.predicate_type,
        subject_name: verified.statement.subject[0].name.clone(),
        extent_stated: extent.is_some(),
        extent,
    })
}

pub fn cmd_verify_attestation(args: VerifyAttestationArgs) -> Result<i32> {
    let report = match verify(&args) {
        Ok(report) => report,
        Err(refusal) if args.errors_json => return Ok(emit_refusal(refusal)),
        Err(refusal) => return Err(anyhow!(refusal.message)),
    };
    let json = serde_json::to_string(&report)
        .map_err(|_| anyhow!("serialize attestation verification report"))?;
    Ok(write_stdout_json(&json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_verification_errors_and_text_only_limits_are_not_misclassified() {
        assert_eq!(
            verification_kind(&anyhow!("signature verification failed")),
            "unclassified"
        );
        assert_eq!(
            read_kind(&io::Error::other("exceeded source bytes limit")),
            "input_unreadable"
        );
        assert_eq!(
            read_kind(&io::Error::other(LimitExceeded {
                kind: LimitKind::SourceBytes,
                limit: 1
            })),
            "limit_exceeded"
        );
    }

    #[test]
    fn library_kinds_map_to_the_wire_vocabulary() {
        for (kind, expected) in [
            (AttestationErrorKind::Malformed, "malformed"),
            (AttestationErrorKind::Unsupported, "unsupported"),
            (AttestationErrorKind::SignatureInvalid, "signature_invalid"),
            (AttestationErrorKind::LimitExceeded, "limit_exceeded"),
            (AttestationErrorKind::BundleRejected, "bundle_rejected"),
            (AttestationErrorKind::ArtifactMismatch, "artifact_mismatch"),
            (
                AttestationErrorKind::PredicateMismatch,
                "predicate_mismatch",
            ),
        ] {
            assert_eq!(kind_name(Some(kind)), expected);
        }
    }
}
