use super::super::args::RunArgs;
use super::run_output::{
    decide_run_outcome, export_baseline, reason_code_from_anyhow_error,
    reason_code_from_error_message, summary_from_outcome, write_extended_run_json,
    write_run_json_minimal,
};
use super::runner_builder::{build_runner, ensure_parent_dir};
use crate::exit_codes::ReasonCode;
use std::path::PathBuf;

pub(crate) async fn run(args: RunArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    // determine strictly what version to use? args.exit_codes is available.
    let version = args.exit_codes;
    let run_json_path = PathBuf::from("run.json");

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

    // Argument validation
    if args.baseline.is_some() && args.export_baseline.is_some() {
        eprintln!("config error: cannot use --baseline and --export-baseline together");
        return write_error(
            ReasonCode::EInvalidArgs,
            "Cannot use --baseline and --export-baseline together".into(),
        );
    }

    let cfg =
        match assay_core::config::load_config(&args.config, legacy_mode, args.deny_deprecations) {
            Ok(c) => c,
            Err(e) => {
                let msg = e.to_string();
                let reason = reason_code_from_error_message(&msg).unwrap_or(ReasonCode::ECfgParse);
                return write_error(reason, msg);
            }
        };

    // Check for deprecated legacy usage
    if !cfg.is_legacy() && cfg.has_legacy_usage() {
        let msg = format!(
            "Deprecated policy file usage detected in version {} config. Run 'assay migrate' to inline policies.",
            cfg.version
        );
        if args.deny_deprecations {
            return write_error(ReasonCode::ECfgParse, msg);
        }
        eprintln!("WARN: {}", msg);
    }

    let store = match assay_core::storage::Store::open(&args.db) {
        Ok(s) => s,
        Err(e) => return write_error(ReasonCode::ECfgParse, format!("Failed to open DB: {}", e)),
    };

    let runner = build_runner(
        store,
        &args.trace_file,
        &cfg,
        args.rerun_failures,
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
            let reason = reason_code_from_anyhow_error(&e).unwrap_or(ReasonCode::ECfgParse);
            return write_error(reason, msg);
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

    let outcome = decide_run_outcome(&artifacts.results, args.strict, args.exit_codes);
    // Use extended writer for authoritative reason coding in run.json (no SARIF in run command)
    write_extended_run_json(&artifacts, &outcome, &run_json_path, None)?;

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
    // E7.2: seeds in summary
    summary = summary.with_seeds(artifacts.order_seed, None);
    // E7.3: judge metrics
    if let Some(metrics) =
        assay_core::report::summary::judge_metrics_from_results(&artifacts.results)
    {
        summary = summary.with_judge_metrics(metrics);
    }
    assay_core::report::summary::write_summary(&summary, &summary_path)?;

    assay_core::report::console::print_summary(&artifacts.results, args.explain_skip);
    assay_core::report::console::print_run_footer(
        Some(&summary.seeds),
        summary.judge_metrics.as_ref(),
    );

    // PR11: Export baseline logic
    if let Some(path) = &args.export_baseline {
        if let Err(e) =
            export_baseline(path, &PathBuf::from(&args.config), &cfg, &artifacts.results)
        {
            eprintln!("Failed to export baseline: {}", e);
        }
    }

    Ok(outcome.exit_code)
}
