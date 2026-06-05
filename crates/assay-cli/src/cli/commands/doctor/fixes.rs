use std::path::{Path, PathBuf};

use assay_core::agentic::{build_suggestions, AgenticCtx, SuggestedPatch};
use assay_core::config::{load_config, path_resolver::PathResolver};
use assay_core::errors::diagnostic::{codes, Diagnostic};
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::cli::args::DoctorArgs;
use crate::cli::helpers::{infer_policy_path, normalize_severity};

use super::patching::{apply_patch_to_file, create_empty_trace, preview_patch};

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
    let initial_errors = diagnostics
        .iter()
        .filter(|d| normalize_severity(&d.severity) == "error")
        .count();

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

    if diagnostics
        .iter()
        .any(|d| d.code == codes::E_TRACE_MISS || d.code == codes::E_PATH_NOT_FOUND)
    {
        if let Some(trace_path) = trace_fix_target(args, diagnostics) {
            if !trace_path.exists() {
                ops.push(DoctorFixOp::CreateTrace { path: trace_path });
            }
        }
    }

    ops.sort_by_key(|op| op.id());

    if ops.is_empty() {
        println!("\nNo auto-fixable diagnostics found.");
        return Ok(if initial_errors == 0 { 0 } else { 1 });
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
        return Ok(1);
    }

    if applied == 0 {
        println!("No fixes applied.");
        return Ok(if initial_errors == 0 { 0 } else { 1 });
    }

    if args.dry_run {
        println!("\nDry run complete. {} fix(es) previewed.", applied);
        return Ok(if initial_errors == 0 { 0 } else { 1 });
    }

    let cfg = match load_config(config_path, legacy_mode, false) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Re-validation skipped: config still invalid ({})", err);
            return Ok(1);
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

    Ok(if remaining_errors == 0 { 0 } else { 1 })
}

fn trace_fix_target(args: &DoctorArgs, diagnostics: &[Diagnostic]) -> Option<PathBuf> {
    if let Some(p) = &args.trace_file {
        return Some(p.clone());
    }

    for d in diagnostics {
        if d.code != codes::E_TRACE_MISS && d.code != codes::E_PATH_NOT_FOUND {
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
