use anyhow::{anyhow, Context};
use dialoguer::{theme::ColorfulTheme, Confirm};
use similar::TextDiff;

use assay_core::agentic::{build_suggestions, AgenticCtx, RiskLevel};
use assay_core::config::{load_config, path_resolver::PathResolver};
use assay_core::validate::{validate, ValidateOptions};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::cli::args::{FixArgs, MaxRisk};
use crate::cli::commands::exit_codes;

pub async fn run(args: FixArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    // 1) Load config (must succeed for P1 fix)
    let cfg = load_config(&args.config, legacy_mode, true)
        .map_err(|e| anyhow!("failed to load config {}: {}", args.config.display(), e))?;

    let resolver = PathResolver::new(&args.config);

    // 2) Validate
    let opts = ValidateOptions {
        trace_file: args.trace_file.clone(),
        baseline_file: args.baseline.clone(),
        replay_strict: args.replay_strict,
    };

    let report = validate(&cfg, &opts, &resolver).await?;

    // 3) Build suggested patches
    let inferred_policy = infer_policy_path(&args.config);
    let (_actions, mut patches) = build_suggestions(
        &report.diagnostics,
        &AgenticCtx {
            policy_path: inferred_policy,
            config_path: Some(args.config.clone()),
        },
    );

    if patches.is_empty() {
        eprintln!("No suggested patches. Nothing to fix.");
        return Ok(exit_codes::OK);
    }

    // 4) Filters: only + max-risk
    let only_set: BTreeSet<String> = args.only.iter().cloned().collect();
    let max_risk = max_risk_to_agentic(args.max_risk);

    patches.retain(|p| {
        let ok_only = only_set.is_empty() || only_set.contains(&p.id);
        let ok_risk = p.risk <= max_risk;
        ok_only && ok_risk
    });

    if patches.is_empty() {
        eprintln!("No patches match the provided filters (--only/--max-risk).");
        return Ok(exit_codes::OK);
    }

    // After filtering, allow listing and exit
    if args.list {
        // Deterministic order by id (and stable output)
        let mut ps = patches;
        ps.sort_by(|a, b| a.id.cmp(&b.id));

        for p in ps {
            // format: <id>\t<risk>\t<file>\t<title>
            println!("{}\t{:?}\t{}\t{}", p.id, p.risk, p.file, p.title);
        }

        return Ok(exit_codes::OK);
    }

    // 5) Group by file (deterministic)
    let mut by_file: BTreeMap<String, Vec<assay_core::agentic::SuggestedPatch>> = BTreeMap::new();
    for p in patches {
        by_file.entry(p.file.clone()).or_default().push(p);
    }
    for (_file, ps) in by_file.iter_mut() {
        ps.sort_by(|a, b| a.id.cmp(&b.id));
    }

    let theme = ColorfulTheme::default();

    let mut applied: Vec<String> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();

    for (file, ps) in by_file {
        let path = PathBuf::from(&file);

        for p in ps {
            let prompt = format!(
                "Apply patch '{}' (id: {}, risk: {:?}) to {}?",
                p.title, p.id, p.risk, file
            );

            let do_apply = if args.yes {
                true
            } else {
                Confirm::with_theme(&theme)
                    .with_prompt(prompt)
                    .default(false)
                    .interact()
                    .unwrap_or(false)
            };

            if !do_apply {
                continue;
            }

            // DRY RUN: print unified diff for this patch
            if args.dry_run {
                let input = std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;

                let is_json = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("json"))
                    .unwrap_or(false);

                let out = assay_core::fix::apply_ops_to_text(&input, &p.ops, is_json)
                    .with_context(|| format!("failed to apply patch {} in memory", p.id))?;

                print_unified_diff(&file, &p.id, &input, &out);
                applied.push(p.id);
                continue;
            }

            match assay_core::fix::apply_ops_to_file(&path, &p.ops) {
                Ok(_) => {
                    eprintln!("Applied: {} -> {}", p.id, file);
                    applied.push(p.id);
                }
                Err(e) => {
                    eprintln!("Failed: {} -> {} ({})", p.id, file, e);
                    failed.push((p.id, e.to_string()));
                }
            }
        }
    }

    if !failed.is_empty() {
        return Ok(exit_codes::CONFIG_ERROR);
    }

    if applied.is_empty() {
        eprintln!("No patches applied.");
        return Ok(exit_codes::OK);
    }

    // 6) Re-run validate after modifications (nice feedback loop)
    // Only if not dry-run
    if args.dry_run {
        return Ok(exit_codes::OK);
    }

    let cfg2 = load_config(&args.config, legacy_mode, true).map_err(|e| {
        anyhow!(
            "after fixes, failed to load config {}: {}",
            args.config.display(),
            e
        )
    })?;

    let report2 = validate(&cfg2, &opts, &resolver).await?;
    let exit2 = decide_exit_like_validate(&report2.diagnostics);

    let error_count = report2
        .diagnostics
        .iter()
        .filter(|d| normalize_severity(d.severity.as_str()) == "error")
        .count();
    let warn_count = report2
        .diagnostics
        .iter()
        .filter(|d| normalize_severity(d.severity.as_str()) == "warn")
        .count();

    eprintln!(
        "Done. Applied {} patch(es). Remaining: {} error(s), {} warning(s).",
        applied.len(),
        error_count,
        warn_count
    );

    Ok(exit2)
}

fn print_unified_diff(file: &str, patch_id: &str, before: &str, after: &str) {
    println!("--- {} (dry-run) patch={} ---", file, patch_id);

    if before == after {
        println!("(no changes)");
        println!("--- end ---");
        return;
    }

    let diff = TextDiff::from_lines(before, after);

    // Very small unified diff
    // (context radius = 3 lines)
    print!(
        "{}",
        diff.unified_diff().context_radius(3).header(file, file)
    );

    println!("--- end ---");
}

fn max_risk_to_agentic(r: MaxRisk) -> RiskLevel {
    match r {
        MaxRisk::Low => RiskLevel::Low,
        MaxRisk::Medium => RiskLevel::Medium,
        MaxRisk::High => RiskLevel::High,
    }
}

// Keep this aligned with validate.rs normalization
fn normalize_severity(s: &str) -> &'static str {
    match s {
        "error" | "ERROR" => "error",
        "warn" | "warning" | "WARN" | "WARNING" => "warn",
        "note" | "info" | "INFO" => "note",
        _ => "note",
    }
}

// Minimal copy of your validate exit heuristic (so fix returns meaningful code)
fn decide_exit_like_validate(diags: &[assay_core::errors::diagnostic::Diagnostic]) -> i32 {
    let has_error = diags
        .iter()
        .any(|d| normalize_severity(d.severity.as_str()) == "error");
    if !has_error {
        return exit_codes::OK;
    }

    let is_config_like = diags.iter().any(|d| {
        normalize_severity(d.severity.as_str()) == "error"
            && (d.code.starts_with("E_CFG_")
                || d.code.starts_with("E_PATH_")
                || d.code.starts_with("E_TRACE_SCHEMA")
                || d.code.starts_with("E_BASE_MISMATCH"))
    });

    if is_config_like {
        exit_codes::CONFIG_ERROR
    } else {
        exit_codes::TEST_FAILED
    }
}

fn infer_policy_path(assay_yaml: &Path) -> Option<PathBuf> {
    let s = std::fs::read_to_string(assay_yaml).ok()?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&s).ok()?;
    let m = doc.as_mapping()?;
    let v = m.get(serde_yaml::Value::String("policy".into()))?;
    let p = v.as_str()?;
    Some(PathBuf::from(p))
}
