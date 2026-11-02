//! Derive an incident package from wrap-session outputs; never invent grades.

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

pub(crate) const LONG_ABOUT: &str = "\
Derive inventory.json and assessment.json from the saved outputs of one wrap \
session, then write a verified incident package. The decision log is required; \
other files are admitted only when SPEC-Incident-Package-v1 names their format.

This command is not a count of all calls, not completeness of capture, not \
proof of what the server did, and an allow is not proof a human accepted.

Surface grades stay as the verifier derives them. The optional request-receipts \
sidecar is retained context only: no new joins or completeness claim. Its original \
bytes, including potentially sensitive questions and answers, are copied unchanged. \
Producer capture may stop without an in-file marker. A producer-valid escaped \
record can exceed the incident verifier record limit (at most 1 MiB) and refuse \
the package. Selection does not prove the sidecar and decision log share a session. The wrap audit log is not admitted. Existing export-incident-package behaviour \
is unchanged: this command derives the documents that command already packages.";

#[derive(Debug, Args, Clone)]
#[command(about = "Derive and export an incident package from wrap-session outputs (Unix)", long_about = LONG_ABOUT)]
pub struct PrepareIncidentPackageArgs {
    /// Decision log written by `assay mcp wrap --decision-log` (proxy-decision-ndjson)
    #[arg(long)]
    pub decision_log: PathBuf,
    /// Copy the whole receipts sidecar unchanged, including potentially sensitive user data
    #[arg(long)]
    pub request_receipts: Option<PathBuf>,
    /// Additional admitted SPEC §3 input: FORMAT=PATH
    #[arg(long = "input", value_name = "FORMAT=PATH")]
    pub inputs: Vec<String>,
    /// Optional external ContextInput JSON file (at most 16 MiB)
    #[arg(long)]
    pub context: Option<PathBuf>,
    /// New archive path in an existing caller-controlled directory
    #[arg(long)]
    pub out: PathBuf,
}

pub fn cmd_prepare_incident_package(args: PrepareIncidentPackageArgs) -> Result<i32> {
    #[cfg(not(unix))]
    {
        let _ = args;
        eprintln!("prepare-incident-package currently requires Unix regular-file admission and atomic publication");
        Ok(2)
    }
    #[cfg(unix)]
    {
        use assay_evidence::IncidentOutcome;
        let report = match prepare(&args) {
            Ok(report) => report,
            Err(report) => *report,
        };
        let status = crate::output_write::write_stdout_json(
            report.to_canonical_string()?.trim_end_matches('\n'),
        );
        Ok(if status != 0 {
            status
        } else if report.outcome == IncidentOutcome::PackageVerified {
            0
        } else {
            2
        })
    }
}

#[cfg(unix)]
fn prepare(
    args: &PrepareIncidentPackageArgs,
) -> Result<assay_evidence::IncidentVerifyReport, Box<assay_evidence::IncidentVerifyReport>> {
    use super::incident_input::{failure, read_context, read_file};
    use assay_common::atomic_write::{write_new, WriteNewError};
    use assay_evidence::{
        prepare_verified_incident_package, ContextLimits, IncidentPrepareSource, IncidentReason,
        IncidentVerifyReport,
    };
    use std::path::Path;

    let fail = |reason| Box::new(failure(reason));
    let context = read_context(args.context.as_deref()).map_err(fail)?;
    let tier = |selected, hard| {
        if selected < hard {
            IncidentReason::ResourceSelected
        } else {
            IncidentReason::ResourceHard
        }
    };
    let mut sources = Vec::new();
    let (bytes, _) = read_file(
        &args.decision_log,
        context.limits.transcript_bytes,
        tier(
            context.limits.transcript_bytes,
            ContextLimits::HARD.transcript_bytes,
        ),
        true,
    )
    .map_err(fail)?;
    sources.push(IncidentPrepareSource {
        id: "wrap-decision-log".to_string(),
        format: "proxy-decision-ndjson".to_string(),
        bytes,
    });
    if let Some(path) = &args.request_receipts {
        let (bytes, _) = read_file(
            path,
            context.limits.transcript_bytes,
            tier(
                context.limits.transcript_bytes,
                ContextLimits::HARD.transcript_bytes,
            ),
            true,
        )
        .map_err(fail)?;
        sources.push(IncidentPrepareSource {
            id: "wrap-request-receipts".to_string(),
            format: "assay-request-receipts-ndjson-v0".to_string(),
            bytes,
        });
    }
    for (index, argument) in args.inputs.iter().enumerate() {
        let Some((format, path)) = argument.split_once('=') else {
            return Err(fail(IncidentReason::InputShape));
        };
        if format.is_empty() || path.is_empty() {
            return Err(fail(IncidentReason::InputShape));
        }
        let (bytes, _) = read_file(
            Path::new(path),
            ContextLimits::HARD.object_bytes_total,
            IncidentReason::ResourceHard,
            true,
        )
        .map_err(fail)?;
        sources.push(IncidentPrepareSource {
            id: format!("input-{index}"),
            format: format.to_string(),
            bytes,
        });
    }
    let (package_bytes, report) = prepare_verified_incident_package(&sources, &context)?;
    let Some(name) = args.out.file_name().and_then(|name| name.to_str()) else {
        return Err(fail(IncidentReason::AtomicOutput));
    };
    let parent = args
        .out
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    write_new(parent, name, &package_bytes).map_err(|error| {
        let reason = match error {
            WriteNewError::AlreadyExists { .. } | WriteNewError::InvalidName { .. } => {
                IncidentReason::AtomicOutput
            }
            WriteNewError::Publish { source, .. }
                if matches!(source.raw_os_error(), Some(libc::ENOTSUP | libc::EXDEV)) =>
            {
                IncidentReason::AtomicOutput
            }
            WriteNewError::TempCleanup { .. } | WriteNewError::SyncDir { .. } => {
                eprintln!("incident package publication did not complete; destination may exist");
                IncidentReason::IoUnavailable
            }
            _ => IncidentReason::IoUnavailable,
        };
        fail(reason)
    })?;
    Ok::<IncidentVerifyReport, Box<IncidentVerifyReport>>(report)
}
