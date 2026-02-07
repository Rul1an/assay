use anyhow::Context;
use assay_core::agentic::{build_suggestions, AgenticCtx, SuggestedPatch};
use assay_core::config::{load_config, path_resolver::PathResolver};
use assay_core::errors::diagnostic::{codes, Diagnostic};
use assay_core::errors::similarity::closest_prompt;
use dialoguer::{theme::ColorfulTheme, Confirm};
use similar::TextDiff;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cli::args::DoctorArgs;
use crate::cli::helpers::{infer_policy_path, normalize_severity};
use crate::diagnostics;
use crate::diagnostics::format::format_text;

pub async fn run(args: DoctorArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    if args.fix && args.format == "json" {
        eprintln!("doctor --fix currently supports text output only; use --format text");
        return Ok(1);
    }
    if (args.yes || args.dry_run) && !args.fix {
        eprintln!("doctor: --yes/--dry-run require --fix");
        return Ok(1);
    }

    // 1. Unified System Diagnostics
    let report = diagnostics::probe_system();

    // 2. Data/Config Diagnostics via Core
    let (target_path, explicit) = match args.config.clone() {
        Some(p) => (p, true),
        None => (PathBuf::from("eval.yaml"), false),
    };

    let (cfg, cfg_err) = if explicit || target_path.exists() {
        match load_config(&target_path, legacy_mode, false) {
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(e.to_string())),
        }
    } else {
        (None, None)
    };

    if args.format == "json" {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let mut json_out = serde_json::to_value(&report)?;

        if let Some(obj) = json_out.as_object_mut() {
            obj.insert("generated_at".to_string(), serde_json::json!(timestamp));

            if let Some(err) = cfg_err {
                obj.insert(
                    "config_error".to_string(),
                    serde_json::json!({
                        "message": err.to_string(),
                        "code": "E_CFG_PARSE"
                    }),
                );
                println!("{}", serde_json::to_string_pretty(&json_out)?);
                return Ok(1);
            }

            if let Some(c) = &cfg {
                let resolver = PathResolver::new(&target_path);
                let opts = assay_core::doctor::DoctorOptions {
                    config_path: target_path.clone(),
                    trace_file: args.trace_file.clone(),
                    baseline_file: args.baseline.clone(),
                    db_path: args.db.clone(),
                    replay_strict: args.replay_strict,
                };
                let core_report = assay_core::doctor::doctor(c, &opts, &resolver).await?;
                obj.insert(
                    "data_diagnostics".to_string(),
                    serde_json::to_value(&core_report.diagnostics)?,
                );
                obj.insert(
                    "data_suggestions".to_string(),
                    serde_json::to_value(&core_report.suggested_actions)?,
                );
            }
        }

        println!("{}", serde_json::to_string_pretty(&json_out)?);
        return Ok(0);
    }

    // Text Format
    let text_output = format_text(&report);
    println!("{}", text_output);

    if let Some(e) = cfg_err {
        println!("\nConfig Status: FAILED");
        println!("  File:     {}", target_path.display());
        println!("  Error:    {}\n", e);

        if args.fix {
            return try_fix_parse_error(&args, &target_path, &e, legacy_mode);
        }
        return Ok(1);
    }

    if let Some(c) = cfg {
        println!("\nPolicy Check:");
        println!("  Config:   {}", target_path.display());
        println!("  Suite:    {}", c.suite);

        let resolver = PathResolver::new(&target_path);
        let opts = assay_core::doctor::DoctorOptions {
            config_path: target_path.clone(),
            trace_file: args.trace_file.clone(),
            baseline_file: args.baseline.clone(),
            db_path: args.db.clone(),
            replay_strict: args.replay_strict,
        };
        let core_report = assay_core::doctor::doctor(&c, &opts, &resolver).await?;

        if !core_report.diagnostics.is_empty() {
            println!("  Issues:   {}", core_report.diagnostics.len());
            for d in &core_report.diagnostics {
                println!("    - [{}] {}", d.severity, d.message);
            }
        } else {
            println!("  Issues:   None (Clean)");
        }

        if args.fix {
            let fix_result =
                run_doctor_fix(&args, &target_path, &core_report.diagnostics, legacy_mode).await?;
            return Ok(fix_result);
        }

        let has_errors = core_report
            .diagnostics
            .iter()
            .any(|d| normalize_severity(&d.severity) == "error");
        return Ok(if has_errors { 1 } else { 0 });
    }

    println!("\nPolicy Check: SKIPPED (No config found; run inside project or use --config)");
    Ok(0)
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

async fn run_doctor_fix(
    args: &DoctorArgs,
    config_path: &Path,
    diagnostics: &[Diagnostic],
    legacy_mode: bool,
) -> anyhow::Result<i32> {
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
        return Ok(0);
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
        return Ok(0);
    }

    if args.dry_run {
        println!("\nDry run complete. {} fix(es) previewed.", applied);
        return Ok(0);
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

fn apply_patch_to_file(patch: &SuggestedPatch) -> anyhow::Result<()> {
    let path = PathBuf::from(&patch.file);
    assay_core::fix::apply_ops_to_file(&path, &patch.ops)
        .with_context(|| format!("failed to apply patch {}", patch.id))?;
    Ok(())
}

fn preview_patch(patch: &SuggestedPatch) -> anyhow::Result<()> {
    let path = PathBuf::from(&patch.file);
    let before = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let is_json = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    let after = assay_core::fix::apply_ops_to_text(&before, &patch.ops, is_json)
        .with_context(|| format!("failed to preview patch {}", patch.id))?;

    print_unified_diff(&patch.file, &patch.id, &before, &after);
    Ok(())
}

fn create_empty_trace(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    std::fs::write(path, "")
        .with_context(|| format!("failed to create trace file {}", path.display()))
}

fn print_unified_diff(file: &str, patch_id: &str, before: &str, after: &str) {
    println!("--- {} (dry-run) patch={} ---", file, patch_id);

    if before == after {
        println!("(no changes)");
        println!("--- end ---");
        return;
    }

    let diff = TextDiff::from_lines(before, after);
    print!(
        "{}",
        diff.unified_diff().context_radius(3).header(file, file)
    );
    println!("--- end ---");
}

fn try_fix_parse_error(
    args: &DoctorArgs,
    config_path: &Path,
    err: &str,
    legacy_mode: bool,
) -> anyhow::Result<i32> {
    let (unknown, candidates) = parse_unknown_field_error(err)
        .map(|(u, c)| (u.to_string(), c))
        .unwrap_or_else(|| (String::new(), Vec::new()));

    if unknown.is_empty() || candidates.is_empty() {
        println!("No auto-fixable config parse issue detected.");
        return Ok(1);
    }

    let replacement = closest_prompt(&unknown, candidates.iter()).and_then(|m| {
        if m.similarity >= 0.80 {
            Some(m.prompt)
        } else {
            None
        }
    });

    let Some(replacement) = replacement else {
        println!(
            "No safe replacement found for '{}'. Try fixing the key manually.",
            unknown
        );
        return Ok(1);
    };

    let do_apply = if args.yes || args.dry_run {
        true
    } else {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Replace key '{}' with '{}' in {}?",
                unknown,
                replacement,
                config_path.display()
            ))
            .default(false)
            .interact()
            .unwrap_or(false)
    };

    if !do_apply {
        println!("No fixes applied.");
        return Ok(1);
    }

    let before = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let Some(after) = replace_yaml_key(&before, &unknown, &replacement) else {
        println!(
            "Could not find YAML key '{}' to replace in {}.",
            unknown,
            config_path.display()
        );
        return Ok(1);
    };

    if args.dry_run {
        print_unified_diff(
            &config_path.display().to_string(),
            "rename_config_key",
            &before,
            &after,
        );
        println!("Dry run complete. 1 fix(es) previewed.");
        return Ok(0);
    }

    write_text_file(config_path, &after)
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    println!(
        "Applied: replaced '{}' with '{}' in {}",
        unknown,
        replacement,
        config_path.display()
    );

    match load_config(config_path, legacy_mode, false) {
        Ok(_) => {
            println!("Config parses successfully after fix.");
            Ok(0)
        }
        Err(e) => {
            println!("Config still has issues after fix: {}", e);
            Ok(1)
        }
    }
}

fn write_text_file(path: &Path, content: &str) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let parent = path.parent().unwrap_or(Path::new("."));
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("assay-config");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp = parent.join(format!(
            ".{}.assay-tmp-{}-{}",
            name,
            std::process::id(),
            nonce
        ));

        std::fs::write(&tmp, content)
            .with_context(|| format!("failed to write temp file {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| {
            format!(
                "failed to atomically replace {} with {}",
                path.display(),
                tmp.display()
            )
        })?;
        return Ok(());
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

fn parse_unknown_field_error(err: &str) -> Option<(&str, Vec<String>)> {
    let unknown = err
        .split("unknown field `")
        .nth(1)?
        .split('`')
        .next()
        .filter(|s| !s.is_empty())?;

    let expected = err.split("expected one of").nth(1)?;
    let mut candidates = extract_backticked_tokens(expected);
    if candidates.is_empty() {
        let expected = expected.split(" at line").next().unwrap_or(expected);
        candidates = expected
            .split(',')
            .map(|s| s.trim().trim_matches('`').trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    if candidates.is_empty() {
        None
    } else {
        Some((unknown, candidates))
    }
}

fn extract_backticked_tokens(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = input;
    while let Some(start) = rest.find('`') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('`') else {
            break;
        };
        let candidate = rest[..end].trim();
        if !candidate.is_empty() {
            out.push(candidate.to_string());
        }
        rest = &rest[end + 1..];
    }
    out
}

fn replace_yaml_key(content: &str, from: &str, to: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::with_capacity(content.len());

    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(&format!("{}:", from)) {
            let indent = &line[..line.len() - trimmed.len()];
            out.push_str(indent);
            out.push_str(to);
            out.push(':');
            out.push_str(rest);
            out.push('\n');
            changed = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    if changed {
        Some(out)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_unknown_field_error, replace_yaml_key};

    #[test]
    fn parse_unknown_field_extracts_candidates() {
        let err = "unknown field `response_format`, expected one of `format`, `out`, `trace_file`";
        let (unknown, candidates) = parse_unknown_field_error(err).expect("parsed");
        assert_eq!(unknown, "response_format");
        assert!(candidates.iter().any(|c| c == "format"));
    }

    #[test]
    fn parse_unknown_field_ignores_line_column_suffix() {
        let err = "unknown field `response_format`, expected one of `format`, `out`, `trace_file` at line 4 column 3";
        let (unknown, candidates) = parse_unknown_field_error(err).expect("parsed");
        assert_eq!(unknown, "response_format");
        assert!(candidates.iter().any(|c| c == "trace_file"));
        assert!(candidates.iter().all(|c| !c.contains("line")));
    }

    #[test]
    fn replace_yaml_key_rewrites_key() {
        let input = "version: 1\nresponse_format: text\n";
        let out = replace_yaml_key(input, "response_format", "format").expect("replacement");
        assert!(out.contains("format: text"));
        assert!(!out.contains("response_format: text"));
    }
}
