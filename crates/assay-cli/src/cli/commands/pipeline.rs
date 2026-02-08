use super::super::args::{CiArgs, JudgeArgs, RunArgs};
use super::run_output::{
    decide_run_outcome, export_baseline, reason_code_from_anyhow_error,
    reason_code_from_error_message, summary_from_outcome, write_run_json_minimal,
};
use super::runner_builder::{build_runner, ensure_parent_dir};
use crate::exit_codes::{ExitCodeVersion, ReasonCode, RunOutcome};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) struct PipelineInput {
    pub config: PathBuf,
    pub db: PathBuf,
    pub trace_file: Option<PathBuf>,
    pub baseline: Option<PathBuf>,
    pub export_baseline: Option<PathBuf>,
    pub strict: bool,
    pub rerun_failures: u32,
    pub quarantine_mode: String,
    pub embedder: String,
    pub embedding_model: String,
    pub refresh_embeddings: bool,
    pub incremental: bool,
    pub refresh_cache: bool,
    pub no_cache: bool,
    pub judge: JudgeArgs,
    pub replay_strict: bool,
    pub deny_deprecations: bool,
    pub redact_prompts: bool,
    pub exit_codes: ExitCodeVersion,
    pub require_config_exists: bool,
    pub ingest_trace_on_replay_strict: bool,
    pub strict_zero_reruns: bool,
}

impl PipelineInput {
    pub(crate) fn from_run(args: &RunArgs) -> Self {
        Self {
            config: args.config.clone(),
            db: args.db.clone(),
            trace_file: args.trace_file.clone(),
            baseline: args.baseline.clone(),
            export_baseline: args.export_baseline.clone(),
            strict: args.strict,
            rerun_failures: args.rerun_failures,
            quarantine_mode: args.quarantine_mode.clone(),
            embedder: args.embedder.clone(),
            embedding_model: args.embedding_model.clone(),
            refresh_embeddings: args.refresh_embeddings,
            incremental: args.incremental,
            refresh_cache: args.refresh_cache,
            no_cache: args.no_cache,
            judge: args.judge.clone(),
            replay_strict: args.replay_strict,
            deny_deprecations: args.deny_deprecations,
            redact_prompts: args.redact_prompts,
            exit_codes: args.exit_codes,
            require_config_exists: false,
            ingest_trace_on_replay_strict: false,
            strict_zero_reruns: false,
        }
    }

    pub(crate) fn from_ci(args: &CiArgs) -> Self {
        Self {
            config: args.config.clone(),
            db: args.db.clone(),
            trace_file: args.trace_file.clone(),
            baseline: args.baseline.clone(),
            export_baseline: args.export_baseline.clone(),
            strict: args.strict,
            rerun_failures: args.rerun_failures,
            quarantine_mode: args.quarantine_mode.clone(),
            embedder: args.embedder.clone(),
            embedding_model: args.embedding_model.clone(),
            refresh_embeddings: args.refresh_embeddings,
            incremental: args.incremental,
            refresh_cache: args.refresh_cache,
            no_cache: args.no_cache,
            judge: args.judge.clone(),
            replay_strict: args.replay_strict,
            deny_deprecations: args.deny_deprecations,
            redact_prompts: args.redact_prompts,
            exit_codes: args.exit_codes,
            require_config_exists: true,
            ingest_trace_on_replay_strict: true,
            strict_zero_reruns: true,
        }
    }
}

pub(crate) enum PipelineError {
    Classified { reason: ReasonCode, message: String },
    Fatal(anyhow::Error),
}

pub(crate) struct PipelineSuccess {
    pub cfg: assay_core::model::EvalConfig,
    pub artifacts: assay_core::report::RunArtifacts,
    pub outcome: RunOutcome,
}

pub(crate) async fn execute_pipeline(
    input: &PipelineInput,
    legacy_mode: bool,
) -> Result<PipelineSuccess, PipelineError> {
    if let Err(e) = ensure_parent_dir(&input.db) {
        return Err(PipelineError::Classified {
            reason: ReasonCode::ECfgParse,
            message: format!("Failed to create DB dir: {}", e),
        });
    }

    if input.baseline.is_some() && input.export_baseline.is_some() {
        eprintln!("config error: cannot use --baseline and --export-baseline together");
        return Err(PipelineError::Classified {
            reason: ReasonCode::EInvalidArgs,
            message: "Cannot use --baseline and --export-baseline together".to_string(),
        });
    }

    let cfg = if input.require_config_exists && !input.config.exists() {
        return Err(PipelineError::Classified {
            reason: ReasonCode::EMissingConfig,
            message: format!("Config file not found: {}", input.config.display()),
        });
    } else {
        match assay_core::config::load_config(&input.config, legacy_mode, input.deny_deprecations) {
            Ok(c) => c,
            Err(e) => {
                let msg = e.to_string();
                return Err(PipelineError::Classified {
                    reason: reason_code_from_error_message(&msg).unwrap_or(ReasonCode::ECfgParse),
                    message: msg,
                });
            }
        }
    };

    if !cfg.is_legacy() && cfg.has_legacy_usage() {
        let msg = format!(
            "Deprecated policy file usage detected in version {} config. Run 'assay migrate' to inline policies.",
            cfg.version
        );
        if input.deny_deprecations {
            return Err(PipelineError::Classified {
                reason: ReasonCode::ECfgParse,
                message: msg,
            });
        }
        eprintln!("WARN: {}", msg);
    }

    let store = match assay_core::storage::Store::open(&input.db) {
        Ok(s) => s,
        Err(e) => {
            return Err(PipelineError::Classified {
                reason: ReasonCode::ECfgParse,
                message: format!("Failed to open DB: {}", e),
            });
        }
    };

    if input.ingest_trace_on_replay_strict {
        if let Err(e) = store.init_schema() {
            return Err(PipelineError::Classified {
                reason: ReasonCode::ECfgParse,
                message: format!("Failed to init DB schema: {}", e),
            });
        }
        if input.replay_strict {
            if let Some(trace_path) = &input.trace_file {
                match assay_core::trace::ingest::ingest_into_store(&store, trace_path) {
                    Ok(stats) => {
                        eprintln!(
                            "auto-ingest: loaded {} events into {} (from {})",
                            stats.event_count,
                            input.db.display(),
                            trace_path.display()
                        );
                    }
                    Err(e) => {
                        let msg = format!("Failed to ingest trace: {}", e);
                        return Err(PipelineError::Classified {
                            reason: reason_code_from_error_message(&msg)
                                .unwrap_or(ReasonCode::ECfgParse),
                            message: msg,
                        });
                    }
                }
            }
        }
    }

    let reruns = if input.strict_zero_reruns && input.strict {
        0
    } else {
        input.rerun_failures
    };

    let runner = build_runner(
        store,
        &input.trace_file,
        &cfg,
        reruns,
        &input.quarantine_mode,
        &input.embedder,
        &input.embedding_model,
        input.refresh_embeddings,
        input.incremental,
        input.refresh_cache || input.no_cache,
        &input.judge,
        &input.baseline,
        input.config.clone(),
        input.replay_strict,
    )
    .await;

    let runner = match runner {
        Ok(r) => r,
        Err(e) => {
            if let Some(diag) = assay_core::errors::try_map_error(&e) {
                eprintln!("{}", diag);
                return Err(PipelineError::Classified {
                    reason: ReasonCode::ECfgParse,
                    message: diag.to_string(),
                });
            }
            let msg = e.to_string();
            return Err(PipelineError::Classified {
                reason: reason_code_from_anyhow_error(&e).unwrap_or(ReasonCode::ECfgParse),
                message: msg,
            });
        }
    };

    let total = cfg.tests.len();
    if total > 0 {
        eprintln!("Running {} tests...", total);
    }
    let progress = assay_core::report::console::default_progress_sink(total);
    let mut artifacts = runner
        .run_suite(&cfg, progress)
        .await
        .map_err(PipelineError::Fatal)?;

    if input.redact_prompts {
        let policy = assay_core::redaction::RedactionPolicy::new(true);
        for row in &mut artifacts.results {
            policy.redact_judge_metadata(&mut row.details);
        }
    }

    let outcome = decide_run_outcome(&artifacts.results, input.strict, input.exit_codes);

    Ok(PipelineSuccess {
        cfg,
        artifacts,
        outcome,
    })
}

pub(crate) fn write_error_artifacts(
    reason: ReasonCode,
    message: String,
    version: ExitCodeVersion,
    verify_enabled: bool,
    run_json_path: &PathBuf,
) -> anyhow::Result<i32> {
    let mut o = RunOutcome::from_reason(reason, Some(message), None);
    o.exit_code = reason.exit_code_for(version);
    if let Err(e) = write_run_json_minimal(&o, run_json_path) {
        eprintln!("WARNING: failed to write run.json: {}", e);
    }

    let summary_path = run_json_path
        .parent()
        .map(|p| p.join("summary.json"))
        .unwrap_or_else(|| PathBuf::from("summary.json"));
    let summary = summary_from_outcome(&o, verify_enabled).with_seeds(None, None);
    if let Err(e) = assay_core::report::summary::write_summary(&summary, &summary_path) {
        eprintln!("WARNING: failed to write summary.json: {}", e);
    }
    Ok(o.exit_code)
}

pub(crate) fn build_summary_from_artifacts(
    outcome: &RunOutcome,
    verify_enabled: bool,
    artifacts: &assay_core::report::RunArtifacts,
) -> assay_core::report::summary::Summary {
    let mut summary = summary_from_outcome(outcome, verify_enabled);
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
    summary
}

pub(crate) fn print_pipeline_summary(
    artifacts: &assay_core::report::RunArtifacts,
    explain_skip: bool,
    summary: &assay_core::report::summary::Summary,
) {
    assay_core::report::console::print_summary(&artifacts.results, explain_skip);
    assay_core::report::console::print_run_footer(
        Some(&summary.seeds),
        summary.judge_metrics.as_ref(),
    );
}

pub(crate) fn maybe_export_baseline(
    export_path: &Option<PathBuf>,
    config_path: &Path,
    cfg: &assay_core::model::EvalConfig,
    artifacts: &assay_core::report::RunArtifacts,
) {
    if let Some(path) = export_path {
        if let Err(e) = export_baseline(path, config_path, cfg, &artifacts.results) {
            eprintln!("Failed to export baseline: {}", e);
        }
    }
}
