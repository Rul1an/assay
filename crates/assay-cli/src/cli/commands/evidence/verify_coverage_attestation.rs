//! First production caller of `assay_evidence::verify_cap1_document`.
//!
//! Admits one local file under the CAP-1 ceiling, runs the normative verifier, and writes one
//! closed JSON document. Conformance is internal consistency of that document.

use crate::exit_codes::{ReasonCode, EXIT_SUCCESS};
use crate::output_write::write_stdout_json;
use anyhow::{anyhow, Result};
use assay_common::limits::{LimitExceeded, LimitKind, LimitReader};
use assay_evidence::{
    verify_cap1_document, Cap1AdmissionLimits, Cap1Document, Cap1Refusal, Cap1Stage,
    CAP1_SCHEMA_SHA256, CAP1_SCHEMA_SOURCE,
};
use clap::Args;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const SCHEMA: &str = "assay.evidence.coverage_attestation.verify.v1";
const NON_CLAIMS: [&str; 3] = [
    "no_activity_completeness",
    "no_provider_outcome",
    "no_automatic_trust",
];

#[derive(Debug, Args, Clone)]
pub struct VerifyCoverageAttestationArgs {
    /// Local CAP-1 JSON document. Stdin is not accepted.
    #[arg(long)]
    pub document: PathBuf,
    /// Selected byte budget. Effective ceiling is min(N, HARD_MAX_BYTES) and never rises.
    #[arg(long)]
    pub max_bytes: Option<u64>,
}

enum ReadOutcome {
    Complete(Vec<u8>),
    Oversized { effective: usize },
    Unavailable,
}

fn effective_limit(selected: Option<u64>) -> usize {
    let hard = Cap1AdmissionLimits::HARD_MAX_BYTES as u64;
    selected.unwrap_or(hard).min(hard) as usize
}

fn read_bounded(path: &Path, limit: usize) -> ReadOutcome {
    let Ok(file) = File::open(path) else {
        return ReadOutcome::Unavailable;
    };
    let mut bytes = Vec::new();
    match LimitReader::new(file, limit as u64, LimitKind::SourceBytes).read_to_end(&mut bytes) {
        Ok(_) => ReadOutcome::Complete(bytes),
        Err(err) if LimitExceeded::from_io(&err).is_some() => {
            ReadOutcome::Oversized { effective: limit }
        }
        Err(_) => ReadOutcome::Unavailable,
    }
}

#[derive(Serialize)]
struct SchemaPin {
    source: &'static str,
    sha256: &'static str,
}

#[derive(Serialize)]
struct DocumentView {
    profile: String,
    strata: usize,
    integrity_complete: bool,
    absence_assertions: usize,
}

#[derive(Serialize)]
struct RefusalView {
    stage: &'static str,
    rule: Option<&'static str>,
    at: Option<String>,
    detail: String,
}

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    outcome: &'static str,
    reason: Option<&'static str>,
    input_sha256: Option<String>,
    input_bytes: Option<usize>,
    schema_pin: SchemaPin,
    refusal: Option<RefusalView>,
    document: Option<DocumentView>,
    claim_gate: &'static str,
    non_claims: [&'static str; 3],
}

fn pin() -> SchemaPin {
    SchemaPin {
        source: CAP1_SCHEMA_SOURCE,
        sha256: CAP1_SCHEMA_SHA256,
    }
}

fn blank() -> Report {
    Report {
        schema: SCHEMA,
        outcome: "verification_unavailable",
        reason: Some("io_unavailable"),
        input_sha256: None,
        input_bytes: None,
        schema_pin: pin(),
        refusal: None,
        document: None,
        claim_gate: "not_evaluated",
        non_claims: NON_CLAIMS,
    }
}

fn stage_name(stage: Cap1Stage) -> &'static str {
    match stage {
        Cap1Stage::Admission => "admission",
        Cap1Stage::Schema => "schema",
        Cap1Stage::Rules => "rules",
    }
}

fn reason_token(refusal: &Cap1Refusal, effective: usize) -> &'static str {
    match refusal {
        Cap1Refusal::Oversized { .. } if effective < Cap1AdmissionLimits::HARD_MAX_BYTES => {
            "resource_selected"
        }
        Cap1Refusal::Oversized { .. } => "resource_hard",
        Cap1Refusal::Syntax(_) => "input_shape",
        Cap1Refusal::Schema { .. } => "cap1_shape",
        Cap1Refusal::Rule { .. } => "cap1_rules",
    }
}

fn project_refusal(refusal: &Cap1Refusal) -> RefusalView {
    let at = match refusal {
        Cap1Refusal::Rule { at, .. } => at.clone(),
        Cap1Refusal::Schema { instance_path, .. } if !instance_path.is_empty() => {
            Some(instance_path.clone())
        }
        _ => None,
    };
    RefusalView {
        stage: stage_name(refusal.stage()),
        rule: refusal.rule().map(|rule| rule.as_str()),
        at,
        detail: refusal.to_string(),
    }
}

fn document_view(doc: &Cap1Document) -> DocumentView {
    DocumentView {
        profile: doc.profile.clone(),
        strata: doc.strata.len(),
        integrity_complete: doc.integrity.complete,
        absence_assertions: doc.absence_assertions.as_ref().map_or(0, Vec::len),
    }
}

fn emit(report: &Report, command_exit: i32) -> Result<i32> {
    let json = serde_json::to_string(report)
        .map_err(|_| anyhow!("serialize coverage attestation verification report"))?;
    let write = write_stdout_json(&json);
    if write != EXIT_SUCCESS {
        return Ok(write);
    }
    Ok(command_exit)
}

fn emit_refusal(refusal: &Cap1Refusal, effective: usize, complete: Option<&[u8]>) -> Result<i32> {
    let _ = writeln!(io::stderr().lock(), "{refusal}");
    let mut report = blank();
    report.outcome = "cap1_refused";
    report.reason = Some(reason_token(refusal, effective));
    if !matches!(refusal, Cap1Refusal::Oversized { .. }) {
        if let Some(bytes) = complete {
            report.input_sha256 = Some(hex::encode(Sha256::digest(bytes)));
            report.input_bytes = Some(bytes.len());
        }
    }
    report.refusal = Some(project_refusal(refusal));
    emit(&report, ReasonCode::EEvidenceContract.exit_code())
}

pub fn cmd_verify_coverage_attestation(args: VerifyCoverageAttestationArgs) -> Result<i32> {
    let effective = effective_limit(args.max_bytes);
    let limits = Cap1AdmissionLimits {
        max_bytes: effective,
    };
    match read_bounded(&args.document, effective) {
        ReadOutcome::Unavailable => emit(&blank(), ReasonCode::EEvidenceUnreadable.exit_code()),
        ReadOutcome::Oversized { effective } => emit_refusal(
            &Cap1Refusal::Oversized {
                max_bytes: effective,
            },
            effective,
            None,
        ),
        ReadOutcome::Complete(bytes) => match verify_cap1_document(&bytes, &limits) {
            Ok(doc) => {
                let mut report = blank();
                report.outcome = "cap1_conforms";
                report.reason = None;
                report.input_sha256 = Some(hex::encode(Sha256::digest(&bytes)));
                report.input_bytes = Some(bytes.len());
                report.document = Some(document_view(&doc));
                emit(&report, EXIT_SUCCESS)
            }
            Err(refusal) => emit_refusal(&refusal, effective, Some(&bytes)),
        },
    }
}
