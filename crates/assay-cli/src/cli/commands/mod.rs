use super::args::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;

pub mod baseline;
pub mod calibrate;
pub mod trace;

pub mod config_path;
pub mod coverage;
pub mod demo;
pub mod discover;
pub mod doctor;
pub mod events;
pub mod evidence;
pub mod explain;
pub mod fix;
pub mod generate;
pub mod heuristics;
pub mod import;
pub mod init;
pub mod init_ci;
pub mod kill;
pub mod mcp;
pub mod migrate;
pub mod monitor;
pub mod policy;
pub mod profile;
#[cfg(test)]
mod profile_simulation_test;
pub mod profile_types;
pub mod record;
pub mod sandbox;
pub mod setup;
#[cfg(feature = "sim")]
pub mod sim;
pub mod tool;
pub mod validate;

// Use crate-level exit codes module (direct items to avoid E0603)
use crate::exit_codes::{ReasonCode, RunOutcome, EXIT_SUCCESS};

pub async fn dispatch(cli: Cli, legacy_mode: bool) -> anyhow::Result<i32> {
    match cli.cmd {
        Command::Init(args) => init::run(args).await,
        Command::Run(args) => cmd_run(args, legacy_mode).await,
        Command::Ci(args) => cmd_ci(args, legacy_mode).await,
        Command::Validate(args) => validate::run(args, legacy_mode).await,
        Command::Fix(args) => fix::run(args, legacy_mode).await,
        Command::Doctor(args) => doctor::run(args, legacy_mode).await,
        Command::Import(args) => import::cmd_import(args),
        Command::Quarantine(args) => cmd_quarantine(args).await,
        Command::Trace(args) => trace::cmd_trace(args, legacy_mode).await,
        Command::Calibrate(args) => calibrate::cmd_calibrate(args).await,
        Command::Baseline(args) => match args.cmd {
            BaselineSub::Report(report_args) => {
                baseline::cmd_baseline_report(report_args).map(|_| EXIT_SUCCESS)
            }
            BaselineSub::Record(record_args) => {
                baseline::cmd_baseline_record(record_args).map(|_| EXIT_SUCCESS)
            }
            BaselineSub::Check(check_args) => {
                baseline::cmd_baseline_check(check_args).map(|_| EXIT_SUCCESS)
            }
        },
        Command::Migrate(args) => migrate::cmd_migrate(args),
        Command::Coverage(args) => coverage::cmd_coverage(args).await,
        Command::Explain(args) => explain::run(args).await,
        Command::Demo(args) => demo::cmd_demo(args).await,
        Command::InitCi(args) => init_ci::cmd_init_ci(args),
        Command::Mcp(args) => mcp::run(args).await,
        Command::Discover(args) => discover::run(args).await,
        Command::Kill(args) => kill::run(args).await,
        Command::Monitor(args) => monitor::run(args).await,
        Command::Policy(args) => policy::run(args).await,
        Command::Generate(args) => generate::run(args),
        Command::Record(args) => record::run(args).await,
        Command::Profile(args) => profile::run(args),
        Command::Sandbox(args) => sandbox::run(args).await,
        Command::Evidence(args) => evidence::run(args),
        #[cfg(feature = "sim")]
        Command::Sim(args) => sim::run(args),
        Command::Setup(args) => setup::run(args).await,
        Command::Tool(args) => Ok(tool::cmd_tool(args.cmd)),
        Command::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(EXIT_SUCCESS)
        }
    }
}

async fn cmd_run(args: RunArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    // determine strictly what version to use? args.exit_codes is available.
    let version = args.exit_codes;
    let run_json_path = PathBuf::from("run.json");

    if args.deny_deprecations {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }

    // Helper to write error run.json and return specific exit code
    let write_error = |reason: ReasonCode, msg: String| -> anyhow::Result<i32> {
        let mut o = RunOutcome::from_reason(reason, Some(msg), None);
        o.exit_code = reason.exit_code_for(version);
        write_run_json_minimal(&o, &run_json_path).ok(); // Best effort write
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

    let cfg = match assay_core::config::load_config(&args.config, legacy_mode, false) {
        Ok(c) => c,
        Err(e) => {
            // Heuristic: is it missing?
            let msg = e.to_string();
            let reason = if msg.contains("not found") {
                ReasonCode::EMissingConfig
            } else {
                ReasonCode::ECfgParse
            };
            return write_error(reason, msg);
        }
    };

    // Check for deprecated legacy usage
    if !cfg.is_legacy() && cfg.has_legacy_usage() {
        eprintln!("WARN: Deprecated policy file usage detected in version {} config. Run 'assay migrate' to inline policies.", cfg.version);
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
            if msg.contains("config error") {
                return write_error(ReasonCode::ECfgParse, msg.clone());
            }
            if msg.to_lowercase().contains("trace")
                && (msg.contains("not found")
                    || msg.contains("No such file")
                    || msg.contains("failed to load trace"))
            {
                return write_error(ReasonCode::ETraceNotFound, msg);
            }
            // General initialization failure
            return write_error(ReasonCode::ECfgParse, msg);
        }
    };

    let mut artifacts = runner.run_suite(&cfg).await?;

    if args.redact_prompts {
        let policy = assay_core::redaction::RedactionPolicy::new(true);
        for row in &mut artifacts.results {
            policy.redact_judge_metadata(&mut row.details);
        }
    }

    let outcome = decide_run_outcome(&artifacts.results, args.strict, args.exit_codes);
    // Use extended writer for authoritative reason coding in run.json
    write_extended_run_json(&artifacts, &outcome, &run_json_path)?;

    assay_core::report::console::print_summary(&artifacts.results, args.explain_skip);

    // PR11: Export baseline logic
    if let Some(path) = &args.export_baseline {
        if let Err(e) =
            export_baseline(path, &PathBuf::from(&args.config), &cfg, &artifacts.results)
        {
            eprintln!("Failed to export baseline: {}", e);
            // Non-fatal? Or change outcome?
            // Usually artifacts are written. This is auxiliary.
            // We return existing outcome exit code, but maybe warn.
        }
    }

    Ok(outcome.exit_code)
}

async fn cmd_ci(args: CiArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    let version = args.exit_codes;
    let run_json_path = PathBuf::from("run.json");

    if args.deny_deprecations {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }

    // Helper to write error run.json and return specific exit code
    let write_error = |reason: ReasonCode, msg: String| -> anyhow::Result<i32> {
        let mut o = RunOutcome::from_reason(reason, Some(msg), None);
        o.exit_code = reason.exit_code_for(version);
        write_run_json_minimal(&o, &run_json_path).ok();
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

    let cfg = match assay_core::config::load_config(&args.config, legacy_mode, false) {
        Ok(c) => c,
        Err(e) => return write_error(ReasonCode::ECfgParse, e.to_string()),
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
            if msg.to_lowercase().contains("trace")
                && (msg.contains("not found")
                    || msg.contains("No such file")
                    || msg.contains("failed to load trace"))
            {
                return write_error(ReasonCode::ETraceNotFound, msg);
            }
            return write_error(ReasonCode::ECfgParse, msg);
        }
    };

    let mut artifacts = runner.run_suite(&cfg).await?;

    if args.redact_prompts {
        let policy = assay_core::redaction::RedactionPolicy::new(true);
        for row in &mut artifacts.results {
            policy.redact_judge_metadata(&mut row.details);
        }
    }

    // Determine and Write Outcome FIRST (Safety against report write failures)
    let mut outcome = decide_run_outcome(&artifacts.results, args.strict, args.exit_codes);
    write_extended_run_json(&artifacts, &outcome, &run_json_path)?;

    // Then Write Output Formats (Best Effort - Option B: Don't fail workflow on report IO)
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

    if let Err(e) = (|| -> anyhow::Result<()> {
        if let Some(parent) = args.sarif.parent() {
            std::fs::create_dir_all(parent)?;
        }
        assay_core::report::sarif::write_sarif("assay", &artifacts.results, &args.sarif)?;
        Ok(())
    })() {
        let msg = format!("Failed to write SARIF report: {}", e);
        eprintln!("WARNING: {}", msg);
        outcome.warnings.push(msg);
    }

    // Re-write run.json if warnings occurred (to maintain Single Source of Truth fidelity)
    if !outcome.warnings.is_empty() {
        write_extended_run_json(&artifacts, &outcome, &run_json_path)?;
    }

    assay_core::report::console::print_summary(&artifacts.results, args.explain_skip);

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

    Ok(outcome.exit_code)
}

async fn cmd_quarantine(args: QuarantineArgs) -> anyhow::Result<i32> {
    ensure_parent_dir(&args.db)?;
    let store = assay_core::storage::Store::open(&args.db)?;
    store.init_schema()?;
    let svc = assay_core::quarantine::QuarantineService::new(store);

    match args.cmd {
        QuarantineSub::Add { test_id, reason } => {
            svc.add(&args.suite, &test_id, &reason)?;
            eprintln!("quarantine added: suite={} test_id={}", args.suite, test_id);
        }
        QuarantineSub::Remove { test_id } => {
            svc.remove(&args.suite, &test_id)?;
            eprintln!(
                "quarantine removed: suite={} test_id={}",
                args.suite, test_id
            );
        }
        QuarantineSub::List => {
            eprintln!("quarantine list: not implemented");
        }
    }
    Ok(EXIT_SUCCESS)
}

fn decide_run_outcome(
    results: &[assay_core::model::TestResultRow],
    strict: bool,
    version: crate::exit_codes::ExitCodeVersion,
) -> crate::exit_codes::RunOutcome {
    use assay_core::model::TestStatus;

    // Helper to ensure exit code matches requested version
    let make_outcome = |reason: ReasonCode, msg: Option<String>, context: Option<&str>| {
        let mut o = RunOutcome::from_reason(reason, msg, context);
        o.exit_code = reason.exit_code_for(version);
        o
    };

    // Priority 1: Config Errors (Exit 2)
    // Granular detection per user request
    for r in results {
        let msg = r.message.to_lowercase();
        // Trace Not Found
        if msg.contains("trace not found") || msg.contains("tracenotfound") {
            return make_outcome(ReasonCode::ETraceNotFound, Some(r.message.clone()), None);
        }
        // Missing Config (generic heuristic)
        if msg.contains("no config found") || msg.contains("config missing") {
            return make_outcome(ReasonCode::EMissingConfig, Some(r.message.clone()), None);
        }
        // Config Parse / General Config Error
        if msg.contains("config error") || msg.contains("configerror") {
            return make_outcome(ReasonCode::ECfgParse, Some(r.message.clone()), None);
        }
    }

    // Priority 2: Infrastructure Failures (Refined Heuristics)
    let infra_errors: Vec<&assay_core::model::TestResultRow> = results
        .iter()
        .filter(|r| matches!(r.status, TestStatus::Error))
        .collect();

    if !infra_errors.is_empty() {
        let reason = pick_infra_reason(&infra_errors);
        return make_outcome(
            reason,
            Some("Infrastructure failures detected".into()),
            None,
        );
    }

    // Priority 3: Test Failures
    let fails = results
        .iter()
        .filter(|r| matches!(r.status, TestStatus::Fail))
        .count();
    if fails > 0 {
        let mut o = RunOutcome::test_failure(fails);
        o.exit_code = ReasonCode::ETestFailed.exit_code_for(version);
        return o;
    }

    // Priority 4: Strict Mode Violations
    if strict {
        let violations = results
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    TestStatus::Warn | TestStatus::Flaky | TestStatus::Unstable
                )
            })
            .count();
        if violations > 0 {
            return make_outcome(
                ReasonCode::EPolicyViolation,
                Some(format!("Strict mode: {} policy violations", violations)),
                None,
            );
        }
    }

    // Success (ensure version compliance though Success is usually 0 in all versions)
    let mut o = RunOutcome::success();
    o.exit_code = ReasonCode::Success.exit_code_for(version);
    o
}

fn pick_infra_reason(
    errors: &[&assay_core::model::TestResultRow],
) -> crate::exit_codes::ReasonCode {
    // Heuristic: check messages for known infra patterns
    for r in errors {
        let msg = r.message.to_lowercase();
        if msg.contains("rate limit") || msg.contains("429") {
            return ReasonCode::ERateLimit;
        }
        if msg.contains("timeout") {
            return ReasonCode::ETimeout;
        }
        if msg.contains("500")
            || msg.contains("502")
            || msg.contains("503")
            || msg.contains("504")
            || msg.contains("provider error")
        {
            return ReasonCode::EProvider5xx;
        }
        if msg.contains("network") || msg.contains("connection") || msg.contains("dns") {
            return ReasonCode::ENetworkError;
        }
    }
    // Default fallback
    ReasonCode::EJudgeUnavailable
}

fn write_extended_run_json(
    artifacts: &assay_core::report::RunArtifacts,
    outcome: &crate::exit_codes::RunOutcome,
    path: &PathBuf,
) -> anyhow::Result<()> {
    // Manually construct the JSON to inject outcome fields
    let mut v = serde_json::to_value(artifacts)?;
    if let Some(obj) = v.as_object_mut() {
        // Inject top-level outcome fields for machine-readability (Canonical Contract)
        obj.insert(
            "exit_code".to_string(),
            serde_json::json!(outcome.exit_code),
        );
        obj.insert(
            "reason_code".to_string(),
            serde_json::json!(outcome.reason_code),
        );

        // Conflict avoidance: Move full details to 'resolution' object
        // Do NOT inject 'message' or 'next_step' top-level to avoid collisions with artifact fields.
        obj.insert("resolution".to_string(), serde_json::to_value(outcome)?);

        if !outcome.warnings.is_empty() {
            obj.insert("warnings".into(), serde_json::json!(outcome.warnings));
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&v)?)?;
    Ok(())
}

fn write_run_json_minimal(
    outcome: &crate::exit_codes::RunOutcome,
    path: &PathBuf,
) -> anyhow::Result<()> {
    // Minimal JSON for early exits (no artifacts available)
    let v = serde_json::json!({
        "exit_code": outcome.exit_code,
        "reason_code": outcome.reason_code,
        "resolution": outcome
    });
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&v)?)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn build_runner(
    store: assay_core::storage::Store,
    trace_file: &Option<PathBuf>,
    cfg: &assay_core::model::EvalConfig,
    rerun_failures_arg: u32,
    quarantine_mode_str: &str,
    embedder_provider: &str,
    embedding_model: &str,
    refresh_embeddings: bool,
    incremental: bool,
    refresh_cache: bool,
    judge_args: &JudgeArgs,
    baseline_arg: &Option<PathBuf>,
    cfg_path: PathBuf,
    replay_strict: bool,
) -> anyhow::Result<assay_core::engine::runner::Runner> {
    store.init_schema()?;
    let cache = assay_core::cache::vcr::VcrCache::new(store.clone());

    use anyhow::Context;
    use assay_core::providers::llm::fake::FakeClient;
    use assay_core::providers::llm::LlmClient;
    use assay_core::providers::trace::TraceClient;

    let mut client: Arc<dyn LlmClient + Send + Sync> = if let Some(trace_path) = trace_file {
        let trace_client = TraceClient::from_path(trace_path).context("failed to load trace")?;
        Arc::new(trace_client)
    } else {
        Arc::new(FakeClient::new(cfg.model.clone()))
    };

    // Strict Mode Wiring (LLM)
    if replay_strict && client.provider_name() != "trace" {
        use assay_core::providers::strict::StrictLlmClient;
        client = Arc::new(StrictLlmClient::new(client));
    }

    let metrics = assay_metrics::default_metrics();

    let replay_mode = trace_file.is_some();

    let rerun_failures = if replay_mode {
        if rerun_failures_arg > 0 {
            eprintln!("note: replay mode active; forcing --rerun-failures=0 for determinism");
        }
        0
    } else {
        rerun_failures_arg
    };

    let policy = assay_core::engine::runner::RunPolicy {
        rerun_failures,
        quarantine_mode: assay_core::quarantine::QuarantineMode::parse(quarantine_mode_str),
        replay_strict,
    };

    // Embedder construction
    use assay_core::providers::embedder::{fake::FakeEmbedder, openai::OpenAIEmbedder, Embedder};

    let mut embedder: Option<Arc<dyn Embedder>> = match embedder_provider {
        "none" => None,
        "openai" => {
            let key = if replay_strict {
                "strict-placeholder".to_string() // Don't ask for key if strict, we will wrap anyway
            } else {
                match std::env::var("OPENAI_API_KEY") {
                    Ok(k) => k,
                    Err(_) => {
                        eprint!("OPENAI_API_KEY not set. Enter key: ");
                        use std::io::Write;
                        std::io::stderr().flush()?;
                        let mut input = String::new();
                        let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
                        reader.read_line(&mut input).await?;
                        let trimmed = input.trim().to_string();
                        if trimmed.is_empty() {
                            anyhow::bail!("OpenAI API key is required");
                        }
                        trimmed
                    }
                }
            };
            Some(Arc::new(OpenAIEmbedder::new(
                embedding_model.to_string(),
                key,
            )))
        }
        "fake" => {
            // Useful for testing CLI flow
            Some(Arc::new(FakeEmbedder::new(
                embedding_model,
                vec![1.0, 0.0, 0.0],
            )))
        }
        _ => anyhow::bail!("unknown embedder provider: {}", embedder_provider),
    };

    if replay_strict {
        if let Some(inner) = embedder {
            use assay_core::providers::strict::StrictEmbedder;
            embedder = Some(Arc::new(StrictEmbedder::new(inner)));
        }
    }

    // Judge Construction
    // ------------------
    let judge_config = assay_core::judge::JudgeRuntimeConfig {
        enabled: judge_args.judge != "none" && !judge_args.no_judge,
        provider: judge_args.judge.clone(),
        model: judge_args.judge_model.clone(),
        samples: judge_args.judge_samples,
        temperature: judge_args.judge_temperature,
        max_tokens: judge_args.judge_max_tokens,
        refresh: judge_args.judge_refresh,
    };

    let mut judge_client: Option<Arc<dyn assay_core::providers::llm::LlmClient>> = if !judge_config
        .enabled
    {
        None
    } else {
        match judge_config.provider.as_str() {
            "openai" => {
                let key = if replay_strict {
                    "strict-placeholder".into()
                } else {
                    match &judge_args.judge_api_key {
                        Some(k) => k.clone(),
                        None => std::env::var("OPENAI_API_KEY")
                            .map_err(|_| anyhow::anyhow!("Judge enabled (openai) but OPENAI_API_KEY not set (VERDICT_JUDGE_API_KEY also empty)"))?
                    }
                };
                let model = judge_config
                    .model
                    .clone()
                    .unwrap_or("gpt-4o-mini".to_string());
                Some(Arc::new(
                    assay_core::providers::llm::openai::OpenAIClient::new(
                        model,
                        key,
                        judge_config.temperature,
                        judge_config.max_tokens,
                    ),
                ))
            }
            "fake" => {
                // For now, create a dummy client named "fake-judge"
                Some(Arc::new(DummyClient::new("fake-judge")))
            }
            "none" => None,
            other => anyhow::bail!("unknown judge provider: {}", other),
        }
    };

    if replay_strict {
        if let Some(inner) = judge_client {
            use assay_core::providers::strict::StrictLlmClient;
            judge_client = Some(Arc::new(StrictLlmClient::new(inner)));
        }
    }

    let judge_store = assay_core::storage::judge_cache::JudgeCache::new(store.clone());
    let judge_service =
        assay_core::judge::JudgeService::new(judge_config, judge_store, judge_client);

    // Load baseline if provided
    let baseline = if let Some(path) = baseline_arg {
        let b = assay_core::baseline::Baseline::load(path)?;
        let fp = assay_core::baseline::compute_config_fingerprint(&cfg_path);
        if let Err(e) = b.validate(&cfg.suite, &fp) {
            eprintln!("fatal: {}", e);
            return Err(anyhow::anyhow!("config error").context(e));
        }
        Some(b)
    } else {
        None
    };

    Ok(assay_core::engine::runner::Runner {
        store,
        cache,
        client,
        metrics,
        policy,
        embedder,
        refresh_embeddings,
        incremental,
        refresh_cache,
        judge: Some(judge_service),
        baseline,
    })
}

fn ensure_parent_dir(path: &std::path::Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn export_baseline(
    path: &PathBuf,
    config_path: &Path,
    cfg: &assay_core::model::EvalConfig,
    results: &[assay_core::model::TestResultRow],
) -> anyhow::Result<()> {
    let mut entries = Vec::new();

    // Convert results to baseline entries
    // For now, we only baseline passing tests? Or all tests with scores?
    // ADR Decision: Baseline captures current state. If current state is failing, we probably shouldn't baseline it, or maybe we should?
    // Usually you baseline known-good. But filtering on PASS might exclude valid but low-scoring things.
    // Let's assume user knows what they are doing. We export SCORES.

    for r in results {
        // We need to drill into details.metrics to get per-metric scores.
        // The root 'score' is aggregated. Baseline needs granular metric scores.

        if let Some(metrics) = r.details.get("metrics").and_then(|v| v.as_object()) {
            for (metric_name, m_val) in metrics {
                if let Some(score) = m_val.get("score").and_then(|s| s.as_f64()) {
                    entries.push(assay_core::baseline::BaselineEntry {
                        test_id: r.test_id.clone(),
                        metric: metric_name.clone(),
                        score,
                        meta: None, // Could add model info here if available
                    });
                }
            }
        }
    }

    let b = assay_core::baseline::Baseline {
        schema_version: 1,
        suite: cfg.suite.clone(),
        assay_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        config_fingerprint: assay_core::baseline::compute_config_fingerprint(config_path),
        git_info: None,
        entries,
    };

    b.save(path)?;
    eprintln!("exported baseline to {}", path.display());
    Ok(())
}

#[derive(Clone)]
struct DummyClient {
    model: String,
}

impl DummyClient {
    fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl assay_core::providers::llm::LlmClient for DummyClient {
    async fn complete(
        &self,
        prompt: &str,
        _context: Option<&[String]>,
    ) -> anyhow::Result<assay_core::model::LlmResponse> {
        let text = format!("hello from {} :: {}", self.model, prompt);
        Ok(assay_core::model::LlmResponse {
            text,
            provider: self.provider_name().to_string(),
            model: self.model.clone(),
            cached: false,
            meta: serde_json::json!({"dummy": true}),
        })
    }

    fn provider_name(&self) -> &'static str {
        "dummy"
    }
}
