use super::super::args::CiArgs;
use super::run_output::{
    decide_run_outcome, export_baseline, summary_from_outcome, write_extended_run_json,
    write_run_json_minimal,
};
use super::runner_builder::{build_runner, ensure_parent_dir};
use crate::exit_codes::ReasonCode;
use std::path::PathBuf;

pub(crate) async fn run(args: CiArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    let version = args.exit_codes;
    let run_json_path = PathBuf::from("run.json");

    if args.deny_deprecations {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }

    // Helper to write error run.json + summary.json and return specific exit code
    let write_error = |reason: ReasonCode, msg: String| -> anyhow::Result<i32> {
        let mut o = crate::exit_codes::RunOutcome::from_reason(reason, Some(msg), None);
        o.exit_code = reason.exit_code_for(version);
        if let Err(e) = write_run_json_minimal(&o, &run_json_path) {
            eprintln!("WARNING: failed to write run.json: {}", e);
        }
        let summary_path = run_json_path
            .parent()
            .map(|p| p.join("summary.json"))
            .unwrap_or_else(|| PathBuf::from("summary.json"));
        let summary = summary_from_outcome(&o, !args.no_verify).with_seeds(None, None);
        if let Err(e) = assay_core::report::summary::write_summary(&summary, &summary_path) {
            eprintln!("WARNING: failed to write summary.json: {}", e);
        }
        Ok(o.exit_code)
    };

    if let Err(e) = ensure_parent_dir(&args.db) {
        return write_error(
            ReasonCode::ECfgParse,
            format!("Failed to create DB dir: {}", e),
        );
    }

    // Argument Validation
    if args.baseline.is_some() && args.export_baseline.is_some() {
        eprintln!("config error: cannot use --baseline and --export-baseline together");
        return write_error(
            ReasonCode::EInvalidArgs,
            "Cannot use --baseline and --export-baseline together".into(),
        );
    }

    // Shared Store for Auto-Ingest
    let store = match assay_core::storage::Store::open(&args.db) {
        Ok(s) => s,
        Err(e) => return write_error(ReasonCode::ECfgParse, format!("Failed to open DB: {}", e)),
    };
    if let Err(e) = store.init_schema() {
        return write_error(
            ReasonCode::ECfgParse,
            format!("Failed to init DB schema: {}", e),
        );
    }

    // In Strict Replay mode, we MUST ingest the trace into the DB
    if args.replay_strict {
        if let Some(trace_path) = &args.trace_file {
            match assay_core::trace::ingest::ingest_into_store(&store, trace_path) {
                Ok(stats) => {
                    eprintln!(
                        "auto-ingest: loaded {} events into {} (from {})",
                        stats.event_count,
                        args.db.display(),
                        trace_path.display()
                    );
                }
                Err(e) => {
                    let msg = format!("Failed to ingest trace: {}", e);
                    if msg.contains("No such file")
                        || msg.contains("not found")
                        || msg.contains("failed to ingest trace")
                    {
                        return write_error(ReasonCode::ETraceNotFound, msg);
                    }
                    return write_error(ReasonCode::ECfgParse, msg);
                }
            }
        }
    }

    let cfg = if args.config.exists() {
        match assay_core::config::load_config(&args.config, legacy_mode, false) {
            Ok(c) => c,
            Err(e) => return write_error(ReasonCode::ECfgParse, e.to_string()),
        }
    } else {
        return write_error(
            ReasonCode::EMissingConfig,
            format!("Config file not found: {}", args.config.display()),
        );
    };

    // Observability: Log config version
    if cfg.version > 0 {
        eprintln!("Loaded config version: {}", cfg.version);
        if cfg.has_legacy_usage() {
            eprintln!("WARN: Deprecated policy file usage detected. Run 'assay migrate'.");
        }
    }
    // Strict mode implies no reruns by default policy (fail fast/accurate)
    let reruns = if args.strict { 0 } else { args.rerun_failures };

    let runner = build_runner(
        store,
        &args.trace_file,
        &cfg,
        reruns,
        &args.quarantine_mode,
        &args.embedder,
        &args.embedding_model,
        args.refresh_embeddings,
        args.incremental,
        args.refresh_cache || args.no_cache,
        &args.judge,
        &args.baseline,
        PathBuf::from(&args.config),
        args.replay_strict,
    )
    .await;

    let runner = match runner {
        Ok(r) => r,
        Err(e) => {
            if let Some(diag) = assay_core::errors::try_map_error(&e) {
                eprintln!("{}", diag);
                return write_error(ReasonCode::ECfgParse, diag.to_string());
            }
            let msg = e.to_string();
            if msg.contains("config error") {
                return write_error(ReasonCode::ECfgParse, msg.clone());
            }
            let msg_lower = msg.to_lowercase();
            if msg_lower.contains("trace")
                && (msg_lower.contains("not found")
                    || msg_lower.contains("no such file")
                    || msg_lower.contains("failed to load trace")
                    || msg_lower.contains("failed to ingest trace"))
            {
                return write_error(ReasonCode::ETraceNotFound, msg);
            }
            return write_error(ReasonCode::ECfgParse, msg);
        }
    };

    let total = cfg.tests.len();
    if total > 0 {
        eprintln!("Running {} tests...", total);
    }
    let progress = assay_core::report::console::default_progress_sink(total);
    let mut artifacts = runner.run_suite(&cfg, progress).await?;

    if args.redact_prompts {
        let policy = assay_core::redaction::RedactionPolicy::new(true);
        for row in &mut artifacts.results {
            policy.redact_judge_metadata(&mut row.details);
        }
    }

    // Determine outcome first (safety against report write failures)
    let mut outcome = decide_run_outcome(&artifacts.results, args.strict, args.exit_codes);

    // Write output formats (best effort); SARIF outcome needed for run.json/summary sarif.omitted
    if let Err(e) = (|| -> anyhow::Result<()> {
        if let Some(parent) = args.junit.parent() {
            std::fs::create_dir_all(parent)?;
        }
        assay_core::report::junit::write_junit(&cfg.suite, &artifacts.results, &args.junit)?;
        Ok(())
    })() {
        let msg = format!("Failed to write JUnit report: {}", e);
        eprintln!("WARNING: {}", msg);
        outcome.warnings.push(msg);
    }

    let sarif_outcome = (|| -> anyhow::Result<assay_core::report::sarif::SarifWriteOutcome> {
        if let Some(parent) = args.sarif.parent() {
            std::fs::create_dir_all(parent)?;
        }
        assay_core::report::sarif::write_sarif("assay", &artifacts.results, &args.sarif)
    })();
    let sarif_omitted = match &sarif_outcome {
        Ok(o) => o.omitted_count,
        Err(e) => {
            outcome
                .warnings
                .push(format!("Failed to write SARIF report: {}", e));
            0
        }
    };

    write_extended_run_json(&artifacts, &outcome, &run_json_path, Some(sarif_omitted))?;

    let summary_path = run_json_path
        .parent()
        .map(|p| p.join("summary.json"))
        .unwrap_or_else(|| PathBuf::from("summary.json"));
    let mut summary = summary_from_outcome(&outcome, !args.no_verify);
    let passed = artifacts
        .results
        .iter()
        .filter(|r| r.status.is_passing())
        .count();
    let failed = artifacts
        .results
        .iter()
        .filter(|r| r.status.is_blocking())
        .count();
    summary = summary.with_results(passed, failed, artifacts.results.len());
    summary = summary.with_seeds(artifacts.order_seed, None);
    if let Some(metrics) =
        assay_core::report::summary::judge_metrics_from_results(&artifacts.results)
    {
        summary = summary.with_judge_metrics(metrics);
    }
    summary = summary.with_sarif_omitted(sarif_omitted);
    assay_core::report::summary::write_summary(&summary, &summary_path)?;

    assay_core::report::console::print_summary(&artifacts.results, args.explain_skip);
    assay_core::report::console::print_run_footer(
        Some(&summary.seeds),
        summary.judge_metrics.as_ref(),
    );

    let otel_cfg = assay_core::otel::OTelConfig {
        jsonl_path: args.otel_jsonl.clone(),
        redact_prompts: args.redact_prompts,
    };
    let _ = assay_core::otel::export_jsonl(&otel_cfg, &cfg.suite, &artifacts.results);

    // PR11: Export baseline logic
    if let Some(path) = &args.export_baseline {
        if let Err(e) =
            export_baseline(path, &PathBuf::from(&args.config), &cfg, &artifacts.results)
        {
            eprintln!("Failed to export baseline: {}", e);
        }
    }

    // Write PR comment markdown if requested
    if let Some(comment_path) = &args.pr_comment {
        let md = format_pr_comment(&outcome, &summary, &cfg.suite);
        if let Some(parent) = comment_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(comment_path, &md)?;
        eprintln!("Wrote PR comment to {}", comment_path.display());
    }

    Ok(outcome.exit_code)
}

/// Format a PR comment body as markdown for GitHub Actions.
fn format_pr_comment(
    outcome: &crate::exit_codes::RunOutcome,
    summary: &assay_core::report::summary::Summary,
    suite: &str,
) -> String {
    let mut md = String::new();
    let suite_display = escape_markdown_table_cell(suite);

    // Marker for find/update pattern
    md.push_str("<!-- assay-governance-report -->\n");
    md.push_str("## Assay Governance Report\n\n");

    // Status line
    let status = if outcome.exit_code == 0 {
        "**Status:** :white_check_mark: Pass"
    } else {
        "**Status:** :x: Fail"
    };
    md.push_str(status);
    md.push('\n');
    md.push('\n');

    // Results table
    md.push_str("| Metric | Value |\n");
    md.push_str("|--------|-------|\n");
    md.push_str(&format!("| Suite | {} |\n", suite_display));

    if let Some(results) = &summary.results {
        let total = results.total;
        md.push_str(&format!(
            "| Tests | {}/{} passed |\n",
            results.passed, total
        ));
        if results.failed > 0 {
            md.push_str(&format!("| Failed | {} |\n", results.failed));
        }
    }

    md.push_str(&format!("| Exit | {} |\n", outcome.exit_code));

    if !outcome.reason_code.is_empty() {
        md.push_str(&format!("| Reason | `{}` |\n", outcome.reason_code));
    }

    // Warnings
    if !outcome.warnings.is_empty() {
        md.push('\n');
        md.push_str("<details>\n<summary>Warnings</summary>\n\n");
        for w in &outcome.warnings {
            md.push_str(&format!("- {}\n", escape_markdown_text(w)));
        }
        md.push_str("\n</details>\n");
    }

    // Conversion hint
    md.push('\n');
    md.push_str(
        "> For EU AI Act compliance scanning: `assay evidence lint --pack eu-ai-act-baseline`\n",
    );

    // Footer
    md.push('\n');
    md.push_str("---\n");
    md.push_str(&format!(
        "Generated by Assay v{}\n",
        env!("CARGO_PKG_VERSION")
    ));

    md
}

fn escape_markdown_table_cell(input: &str) -> String {
    escape_markdown_text(input).replace('|', "\\|")
}

fn escape_markdown_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\r' | '\n' => out.push(' '),
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '<' | '>' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}
