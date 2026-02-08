use super::super::args::{CiArgs, JudgeArgs, RunArgs};
use super::pipeline_error::{elapsed_ms, PipelineError};
use super::run_output::decide_run_outcome;
use super::runner_builder::{build_runner, ensure_parent_dir};
use crate::exit_codes::{ExitCodeVersion, RunOutcome};
use std::path::PathBuf;
use std::time::Instant;

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

pub(crate) struct PipelineSuccess {
    pub cfg: assay_core::model::EvalConfig,
    pub artifacts: assay_core::report::RunArtifacts,
    pub outcome: RunOutcome,
    pub timings: PipelineTimings,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PipelineTimings {
    pub total_ms: u64,
    pub config_load_ms: Option<u64>,
    pub ingest_ms: Option<u64>,
    pub runner_build_ms: Option<u64>,
    pub run_suite_ms: Option<u64>,
}

pub(crate) async fn execute_pipeline(
    input: &PipelineInput,
    legacy_mode: bool,
) -> Result<PipelineSuccess, PipelineError> {
    let pipeline_start = Instant::now();
    let mut timings = PipelineTimings::default();

    if let Err(e) = ensure_parent_dir(&input.db) {
        return Err(PipelineError::cfg_parse(
            input.db.display().to_string(),
            format!("Failed to create DB dir: {}", e),
        ));
    }

    if input.baseline.is_some() && input.export_baseline.is_some() {
        eprintln!("config error: cannot use --baseline and --export-baseline together");
        return Err(PipelineError::invalid_args(
            "Cannot use --baseline and --export-baseline together",
        ));
    }

    let cfg = if input.require_config_exists && !input.config.exists() {
        return Err(PipelineError::missing_cfg(
            input.config.display().to_string(),
            "config path does not exist",
        ));
    } else {
        let config_start = Instant::now();
        match assay_core::config::load_config(&input.config, legacy_mode, input.deny_deprecations) {
            Ok(c) => {
                timings.config_load_ms = Some(elapsed_ms(config_start));
                c
            }
            Err(e) => {
                let msg = e.to_string();
                return Err(if !input.config.exists() {
                    PipelineError::missing_cfg(input.config.display().to_string(), msg)
                } else {
                    PipelineError::cfg_parse(input.config.display().to_string(), msg)
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
            return Err(PipelineError::cfg_parse(
                input.config.display().to_string(),
                msg,
            ));
        }
        eprintln!("WARN: {}", msg);
    }

    let store = match assay_core::storage::Store::open(&input.db) {
        Ok(s) => s,
        Err(e) => {
            return Err(PipelineError::cfg_parse(
                input.db.display().to_string(),
                format!("Failed to open DB: {}", e),
            ));
        }
    };

    if input.ingest_trace_on_replay_strict {
        if let Err(e) = store.init_schema() {
            return Err(PipelineError::cfg_parse(
                input.db.display().to_string(),
                format!("Failed to init DB schema: {}", e),
            ));
        }
        if input.replay_strict {
            if let Some(trace_path) = &input.trace_file {
                let ingest_start = Instant::now();
                match assay_core::trace::ingest::ingest_into_store(&store, trace_path) {
                    Ok(stats) => {
                        timings.ingest_ms = Some(elapsed_ms(ingest_start));
                        eprintln!(
                            "auto-ingest: loaded {} events into {} (from {})",
                            stats.event_count,
                            input.db.display(),
                            trace_path.display()
                        );
                    }
                    Err(e) => {
                        let msg = format!("Failed to ingest trace: {}", e);
                        return Err(if trace_path.exists() {
                            PipelineError::cfg_parse(trace_path.display().to_string(), msg)
                        } else {
                            PipelineError::from_run_error(
                                assay_core::errors::RunError::trace_not_found(
                                    trace_path.display().to_string(),
                                    "trace path does not exist",
                                ),
                            )
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

    let runner_build_start = Instant::now();
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
    timings.runner_build_ms = Some(elapsed_ms(runner_build_start));

    let runner = match runner {
        Ok(r) => r,
        Err(e) => {
            if let Some(diag) = assay_core::errors::try_map_error(&e) {
                eprintln!("{}", diag);
                return Err(PipelineError::cfg_parse(
                    input.config.display().to_string(),
                    diag.to_string(),
                ));
            }
            return Err(PipelineError::from_run_error(
                assay_core::errors::RunError::from_anyhow(&e),
            ));
        }
    };

    let total = cfg.tests.len();
    if total > 0 {
        eprintln!("Running {} tests...", total);
    }
    let progress = assay_core::report::console::default_progress_sink(total);
    let run_suite_start = Instant::now();
    let mut artifacts = runner
        .run_suite(&cfg, progress)
        .await
        .map_err(PipelineError::Fatal)?;
    timings.run_suite_ms = Some(elapsed_ms(run_suite_start));

    if input.redact_prompts {
        let policy = assay_core::redaction::RedactionPolicy::new(true);
        for row in &mut artifacts.results {
            policy.redact_judge_metadata(&mut row.details);
        }
    }

    let outcome = decide_run_outcome(&artifacts.results, input.strict, input.exit_codes);
    timings.total_ms = elapsed_ms(pipeline_start);

    Ok(PipelineSuccess {
        cfg,
        artifacts,
        outcome,
        timings,
    })
}
