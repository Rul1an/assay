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

pub mod exit_codes {
    pub const OK: i32 = 0;
    pub const TEST_FAILED: i32 = 1;
    pub const CONFIG_ERROR: i32 = 2;
}

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
                baseline::cmd_baseline_report(report_args).map(|_| exit_codes::OK)
            }
            BaselineSub::Record(record_args) => {
                baseline::cmd_baseline_record(record_args).map(|_| exit_codes::OK)
            }
            BaselineSub::Check(check_args) => {
                baseline::cmd_baseline_check(check_args).map(|_| exit_codes::OK)
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
            Ok(exit_codes::OK)
        }
    }
}

async fn cmd_run(args: RunArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    if args.deny_deprecations {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }
    ensure_parent_dir(&args.db)?;

    // Argument validation
    if args.baseline.is_some() && args.export_baseline.is_some() {
        eprintln!("config error: cannot use --baseline and --export-baseline together");
        return Ok(exit_codes::CONFIG_ERROR);
    }

    let cfg = assay_core::config::load_config(&args.config, legacy_mode, false)
        .map_err(|e| anyhow::anyhow!(e))?;

    // Check for deprecated legacy usage
    if !cfg.is_legacy() && cfg.has_legacy_usage() {
        eprintln!("WARN: Deprecated policy file usage detected in version {} config. Run 'assay migrate' to inline policies.", cfg.version);
    }

    let store = assay_core::storage::Store::open(&args.db)?;

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
                return Ok(exit_codes::CONFIG_ERROR);
            }
            if e.to_string().contains("config error") {
                return Ok(exit_codes::CONFIG_ERROR);
            }
            return Err(e);
        }
    };

    let mut artifacts = runner.run_suite(&cfg).await?;

    if args.redact_prompts {
        let policy = assay_core::redaction::RedactionPolicy::new(true);
        for row in &mut artifacts.results {
            policy.redact_judge_metadata(&mut row.details);
        }
    }

    assay_core::report::json::write_json(&artifacts, &PathBuf::from("run.json"))?;
    assay_core::report::console::print_summary(&artifacts.results, args.explain_skip);

    // PR11: Export baseline logic
    if let Some(path) = &args.export_baseline {
        export_baseline(path, &PathBuf::from(&args.config), &cfg, &artifacts.results)?;
    }

    Ok(decide_exit_code(
        &artifacts.results,
        args.strict,
        args.exit_codes,
    ))
}

async fn cmd_ci(args: CiArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    if args.deny_deprecations {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }
    ensure_parent_dir(&args.db)?;

    // Argument Validation
    if args.baseline.is_some() && args.export_baseline.is_some() {
        eprintln!("config error: cannot use --baseline and --export-baseline together");
        return Ok(exit_codes::CONFIG_ERROR);
    }

    // Shared Store for Auto-Ingest
    let store = assay_core::storage::Store::open(&args.db)?;
    store.init_schema()?; // Ensure tables exist for ingest

    // In Strict Replay mode, we MUST ingest the trace into the DB
    // so that Agent Assertions (which query the DB) can find the episodes/steps.
    if args.replay_strict {
        if let Some(trace_path) = &args.trace_file {
            let stats = assay_core::trace::ingest::ingest_into_store(&store, trace_path)
                .map_err(|e| anyhow::anyhow!("failed to ingest trace: {}", e))?;

            eprintln!(
                "auto-ingest: loaded {} events into {} (from {})",
                stats.event_count,
                args.db.display(),
                trace_path.display()
            );
        }
    }

    let cfg = assay_core::config::load_config(&args.config, legacy_mode, false)
        .map_err(|e| anyhow::anyhow!(e))?;
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
                return Ok(exit_codes::CONFIG_ERROR);
            }
            if e.to_string().contains("config error") {
                return Ok(exit_codes::CONFIG_ERROR);
            }
            return Err(e);
        }
    };

    let mut artifacts = runner.run_suite(&cfg).await?;

    if args.redact_prompts {
        let policy = assay_core::redaction::RedactionPolicy::new(true);
        for row in &mut artifacts.results {
            policy.redact_judge_metadata(&mut row.details);
        }
    }

    assay_core::report::junit::write_junit(&cfg.suite, &artifacts.results, &args.junit)?;
    assay_core::report::sarif::write_sarif("assay", &artifacts.results, &args.sarif)?;
    assay_core::report::json::write_json(&artifacts, &PathBuf::from("run.json"))?;
    assay_core::report::console::print_summary(&artifacts.results, args.explain_skip);

    let otel_cfg = assay_core::otel::OTelConfig {
        jsonl_path: args.otel_jsonl.clone(),
        redact_prompts: args.redact_prompts,
    };
    let _ = assay_core::otel::export_jsonl(&otel_cfg, &cfg.suite, &artifacts.results);

    // PR11: Export baseline logic
    if let Some(path) = &args.export_baseline {
        export_baseline(path, &PathBuf::from(&args.config), &cfg, &artifacts.results)?;
    }

    Ok(decide_exit_code(
        &artifacts.results,
        args.strict,
        args.exit_codes,
    ))
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
    Ok(exit_codes::OK)
}

fn decide_exit_code(
    results: &[assay_core::model::TestResultRow],
    strict: bool,
    version: crate::exit_codes::ExitCodeVersion,
) -> i32 {
    use crate::exit_codes::ReasonCode;
    use assay_core::model::TestStatus;

    // Priority 1: Config Errors (Exit 2 in V2)
    if results.iter().any(|r| r.message.contains("config error:")) {
        // We lack precise reason codes from the runner artifacts yet (future todo),
        // but "config error" message implies ECfgParse usually.
        return ReasonCode::ECfgParse.exit_code_for(version);
    }

    // Priority 2: Fatal Errors (Fail/Error) -> Test Failure (Exit 1)
    // Note: If an error was Infra-related, runner should have returned Error above?
    // Actually runner returns Result. But individual tests can have Error status.
    let has_fatal = results
        .iter()
        .any(|r| matches!(r.status, TestStatus::Fail | TestStatus::Error));

    if has_fatal {
        return ReasonCode::ETestFailed.exit_code_for(version);
    }

    // Priority 3: Strict Mode Violations (Warn/Flaky) -> Test Failure
    if strict
        && results.iter().any(|r| {
            matches!(
                r.status,
                TestStatus::Warn | TestStatus::Flaky | TestStatus::Unstable
            )
        })
    {
        return ReasonCode::EPolicyViolation.exit_code_for(version);
    }

    // Success
    ReasonCode::Success.exit_code_for(version)
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
