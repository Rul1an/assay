use std::path::{Path, PathBuf};

use assay_core::agentic::{build_suggestions, AgenticCtx, SuggestedPatch};
use assay_core::config::{load_config_with_cause, path_resolver::PathResolver, LoadOptions};
use assay_core::errors::diagnostic::{codes, Diagnostic};
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::cli::args::DoctorArgs;
use crate::cli::helpers::{
    decide_exit, decide_repair_failure_exit, infer_policy_path, normalize_severity,
};

use super::implementation::config_failure;
use super::patching::{apply_patch_to_file, create_empty_trace, preview_patch};

#[cfg(test)]
mod tests;

/// Whether a diagnostic is one this module can offer to create a trace file for.
///
/// Asked in two places — the gate that decides whether to look for a target at all, and the scan
/// that finds which path to create — so it is one function rather than one predicate written twice.
/// The two sites are not interchangeable: the scan returns `--trace-file` before it looks at any
/// code, so without the gate a tree with no missing-trace diagnostic would still be offered a
/// trace-creation fix.
fn is_missing_trace(diagnostic: &Diagnostic) -> bool {
    diagnostic.code == codes::E_TRACE_MISS || diagnostic.code == codes::E_PATH_NOT_FOUND
}

#[derive(Debug, Clone)]
enum DoctorFixOp {
    Patch(SuggestedPatch),
    CreateTrace { path: PathBuf },
}

impl DoctorFixOp {
    fn id(&self) -> String {
        match self {
            DoctorFixOp::Patch(p) => p.id.clone(),
            DoctorFixOp::CreateTrace { path } => format!("create_trace:{}", path.display()),
        }
    }

    fn title(&self) -> String {
        match self {
            DoctorFixOp::Patch(p) => p.title.clone(),
            DoctorFixOp::CreateTrace { path } => {
                format!("Create missing trace file '{}'.", path.display())
            }
        }
    }
}

pub(super) async fn run_doctor_fix(
    args: &DoctorArgs,
    config_path: &Path,
    diagnostics: &[Diagnostic],
    legacy_mode: bool,
) -> anyhow::Result<i32> {
    // The class for these diagnostics, decided where every other doctor path decides it. This used
    // to count error-severity diagnostics here and return a literal `1`, which agreed with the rest
    // of doctor only while doctor also returned `1`. Once both output channels moved to
    // `decide_exit` — which reads the ADR-046 class table, so a config-class code is `2` — the hand
    // computation became a second answer to a question the shared function owns, and the exit class
    // started depending on whether `--fix` was passed. Same rule, one function, three readers.
    let unfixed_exit = decide_exit(diagnostics);

    let inferred_policy = infer_policy_path(config_path);
    let (_actions, mut patches) = build_suggestions(
        diagnostics,
        &AgenticCtx {
            policy_path: inferred_policy,
            config_path: Some(config_path.to_path_buf()),
        },
    );

    patches.sort_by(|a, b| a.id.cmp(&b.id));

    let mut ops: Vec<DoctorFixOp> = patches.into_iter().map(DoctorFixOp::Patch).collect();

    if diagnostics.iter().any(is_missing_trace) {
        if let Some(trace_path) = trace_fix_target(args, diagnostics) {
            if !trace_path.exists() {
                ops.push(DoctorFixOp::CreateTrace { path: trace_path });
            }
        }
    }

    ops.sort_by_key(|op| op.id());

    if ops.is_empty() {
        println!("\nNo auto-fixable diagnostics found.");
        return Ok(unfixed_exit);
    }

    println!("\nAuto-fix candidates:");
    for op in &ops {
        println!("  - {}", op.title());
    }

    let theme = ColorfulTheme::default();
    let mut applied = 0usize;
    let mut failed = 0usize;

    for op in &ops {
        let should_apply = if args.yes || args.dry_run {
            true
        } else {
            Confirm::with_theme(&theme)
                .with_prompt(format!("Apply fix '{}'?", op.title()))
                .default(false)
                .interact()
                .unwrap_or(false)
        };

        if !should_apply {
            continue;
        }

        match op {
            DoctorFixOp::Patch(patch) => {
                if args.dry_run {
                    preview_patch(patch)?;
                    applied += 1;
                    continue;
                }

                match apply_patch_to_file(patch) {
                    Ok(_) => {
                        eprintln!("Applied: {}", patch.id);
                        applied += 1;
                    }
                    Err(err) => {
                        eprintln!("Failed: {} ({})", patch.id, err);
                        failed += 1;
                    }
                }
            }
            DoctorFixOp::CreateTrace { path } => {
                if args.dry_run {
                    println!("[dry-run] would create trace file: {}", path.display());
                    applied += 1;
                    continue;
                }

                match create_empty_trace(path) {
                    Ok(_) => {
                        eprintln!("Applied: created trace file {}", path.display());
                        applied += 1;
                    }
                    Err(err) => {
                        eprintln!("Failed: could not create {} ({})", path.display(), err);
                        failed += 1;
                    }
                }
            }
        }
    }

    if failed > 0 {
        return Ok(decide_repair_failure_exit());
    }

    if applied == 0 {
        println!("No fixes applied.");
        return Ok(unfixed_exit);
    }

    if args.dry_run {
        println!("\nDry run complete. {} fix(es) previewed.", applied);
        return Ok(unfixed_exit);
    }

    let cfg = match load_config_with_cause(
        config_path,
        LoadOptions {
            legacy_mode,
            ..Default::default()
        },
    ) {
        Ok(c) => c,
        Err(err) => {
            // An unloadable config is one condition with one class, decided where the non-`--fix`
            // path decides it. This return held a literal `1` while the same config read one
            // function earlier exits `2`, so whether an unloadable config was a config fault
            // depended on how far the command had got before it noticed.
            eprintln!("Re-validation skipped: config still invalid ({})", err);
            return Ok(config_failure(config_path, &err).exit_code);
        }
    };

    let resolver = PathResolver::new(config_path);
    let opts = assay_core::doctor::DoctorOptions {
        config_path: config_path.to_path_buf(),
        trace_file: args.trace_file.clone(),
        baseline_file: args.baseline.clone(),
        db_path: args.db.clone(),
        replay_strict: args.replay_strict,
    };

    let report = assay_core::doctor::doctor(&cfg, &opts, &resolver).await?;
    let remaining_errors = report
        .diagnostics
        .iter()
        .filter(|d| normalize_severity(&d.severity) == "error")
        .count();

    println!(
        "\nApplied {} fix(es). Remaining: {} error(s).",
        applied, remaining_errors
    );

    // The re-validated tree gets its class from the same function as the unrepaired one, so a
    // repair that resolves nothing reports what `doctor` alone would have reported for what is left.
    Ok(decide_exit(&report.diagnostics))
}

fn trace_fix_target(args: &DoctorArgs, diagnostics: &[Diagnostic]) -> Option<PathBuf> {
    if let Some(p) = &args.trace_file {
        return Some(p.clone());
    }

    for d in diagnostics {
        if !is_missing_trace(d) {
            continue;
        }

        if let Some(path) = d.context.get("trace_file").and_then(|v| v.as_str()) {
            if !path.trim().is_empty() {
                return Some(PathBuf::from(path));
            }
        }

        if let Some(path) = d.context.get("path").and_then(|v| v.as_str()) {
            if !path.trim().is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }

    None
}
