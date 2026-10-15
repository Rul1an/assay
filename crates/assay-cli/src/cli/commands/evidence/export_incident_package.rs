//! Package supplied local bytes; never generate an assessment or overwrite output.

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Debug, Args, Clone)]
pub struct ExportIncidentPackageArgs {
    /// Canonical inventory JSON (JCS plus LF)
    #[arg(long)]
    pub inventory: PathBuf,
    /// Canonical recorded assessment JSON (JCS plus LF)
    #[arg(long)]
    pub assessment: PathBuf,
    /// One local file per unique inventory digest: SHA256=PATH
    #[arg(long = "object", value_name = "SHA256=PATH")]
    pub objects: Vec<String>,
    /// Optional external ContextInput JSON file (at most 16 MiB)
    #[arg(long)]
    pub context: Option<PathBuf>,
    /// New archive path in an existing caller-controlled directory
    #[arg(long)]
    pub out: PathBuf,
}

pub fn cmd_export_incident_package(args: ExportIncidentPackageArgs) -> Result<i32> {
    #[cfg(not(unix))]
    {
        let _ = args;
        eprintln!("export-incident-package currently requires Unix regular-file admission and atomic publication");
        Ok(2)
    }
    #[cfg(unix)]
    {
        use assay_evidence::IncidentOutcome;
        let report = match export(&args) {
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
fn export(
    args: &ExportIncidentPackageArgs,
) -> Result<assay_evidence::IncidentVerifyReport, Box<assay_evidence::IncidentVerifyReport>> {
    use super::incident_input::{failure, read_context, read_file};
    use assay_common::atomic_write::{write_new, WriteNewError};
    use assay_evidence::incident_package::verify::{
        build_verified_incident_package, prepare_incident_export,
    };
    use assay_evidence::{ContextLimits, IncidentReason, IncidentVerifyReport};
    use std::collections::BTreeMap;
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
    let (inventory, _) = read_file(
        &args.inventory,
        context.limits.inventory_bytes,
        tier(
            context.limits.inventory_bytes,
            ContextLimits::HARD.inventory_bytes,
        ),
        true,
    )
    .map_err(fail)?;
    let (assessment, _) = read_file(
        &args.assessment,
        context.limits.assessment_bytes,
        tier(
            context.limits.assessment_bytes,
            ContextLimits::HARD.assessment_bytes,
        ),
        true,
    )
    .map_err(fail)?;
    let expected = prepare_incident_export(&inventory, &assessment, &context).map_err(fail)?;
    let mut paths = BTreeMap::new();
    for argument in &args.objects {
        let Some((digest, path)) = argument.split_once('=') else {
            return Err(fail(IncidentReason::InputShape));
        };
        if path.is_empty() || paths.insert(digest, Path::new(path)).is_some() {
            return Err(fail(IncidentReason::InputShape));
        }
    }
    // Missing, surplus and duplicate bindings refuse before any object is opened.
    if paths.len() != expected.len()
        || expected
            .iter()
            .any(|object| !paths.contains_key(object.sha256.as_str()))
    {
        return Err(fail(IncidentReason::Digest));
    }
    let mut staged = Vec::new();
    for object in expected {
        let (bytes, digest) = read_file(
            paths[object.sha256.as_str()],
            object.bytes,
            IncidentReason::Digest,
            true,
        )
        .map_err(fail)?;
        if bytes.len() as u64 != object.bytes || digest != object.sha256 {
            return Err(fail(IncidentReason::Digest));
        }
        staged.push(bytes);
    }
    let objects: Vec<&[u8]> = staged.iter().map(Vec::as_slice).collect();
    let (bytes, report) =
        build_verified_incident_package(&inventory, &assessment, &objects, &context)?;
    let Some(name) = args.out.file_name().and_then(|name| name.to_str()) else {
        return Err(fail(IncidentReason::AtomicOutput));
    };
    let parent = args
        .out
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    write_new(parent, name, &bytes).map_err(|error| {
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
