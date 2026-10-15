//! Read-only offline verifier for Incident Package v1 outer archives.
//!
//! Emits an RFC 8785 JCS-compatible `assay.incident.verify.v1` verification report.
//! Returns exit code 0 for `package_verified`, 2 for `package_refused` or `verification_unavailable`.

use std::path::PathBuf;

use anyhow::Result;
use assay_evidence::{
    verify_incident_package_file, IncidentExpectation, IncidentOutcome, IncidentReason,
    IncidentVerifyReport, INCIDENT_VERIFY_SCHEMA_V1, NON_CLAIMS_DEFAULT,
};
use clap::{Args, ValueEnum};

use crate::exit_codes;
use crate::output_write::write_stdout_json;

#[derive(Debug, Args, Clone)]
pub struct VerifyIncidentPackageArgs {
    /// Incident package archive (.tar)
    #[arg(value_name = "PACKAGE")]
    pub package: PathBuf,

    /// Output format
    #[arg(long, value_enum, default_value_t = IncidentPackageFormat::Json)]
    pub format: IncidentPackageFormat,

    /// Optional external ContextInput JSON file
    #[arg(long, value_name = "CONTEXT")]
    pub context: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum, PartialEq, Eq)]
pub enum IncidentPackageFormat {
    #[default]
    Json,
}

pub fn cmd_verify_incident_package(args: VerifyIncidentPackageArgs) -> Result<i32> {
    let context = match super::incident_input::read_context(args.context.as_deref()) {
        Ok(context) => context,
        Err(reason) => {
            let report = super::incident_input::failure(reason);
            let status = write_stdout_json(&serde_json::to_string_pretty(&report)?);
            return Ok(if status == 0 { 2 } else { status });
        }
    };

    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    // Reject FIFOs after opening instead of blocking until a writer appears.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let package_file = match options.open(&args.package) {
        Ok(b) => b,
        Err(_) => {
            let report = IncidentVerifyReport {
                schema: INCIDENT_VERIFY_SCHEMA_V1.to_string(),
                outcome: IncidentOutcome::VerificationUnavailable,
                reason: Some(IncidentReason::IoUnavailable),
                artifact_sha256: None,
                inventory_sha256: None,
                assessment_sha256: None,
                expectation: IncidentExpectation::NotEvaluated,
                attestations: Vec::new(),
                verification_context: None,
                verification_context_sha256: None,
                resolved_results: Vec::new(),
                counts: None,
                non_claims: NON_CLAIMS_DEFAULT.iter().map(|s| s.to_string()).collect(),
            };
            let json = serde_json::to_string_pretty(&report)?;
            let write_status = write_stdout_json(&json);
            if write_status != exit_codes::EXIT_SUCCESS {
                return Ok(write_status);
            }
            return Ok(2);
        }
    };

    let report = verify_incident_package_file(package_file, &context);
    let json = serde_json::to_string_pretty(&report)?;
    let write_status = write_stdout_json(&json);
    if write_status != exit_codes::EXIT_SUCCESS {
        return Ok(write_status);
    }

    let exit_code = if report.outcome == IncidentOutcome::PackageVerified {
        exit_codes::OK
    } else {
        2
    };
    Ok(exit_code)
}
