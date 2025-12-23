use super::args::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;

pub mod baseline;
pub mod calibrate;
pub mod trace;

pub mod doctor;
pub mod validate;

pub mod exit_codes {
    pub const OK: i32 = 0;
    pub const TEST_FAILED: i32 = 1;
    pub const CONFIG_ERROR: i32 = 2;
}

pub async fn dispatch(cli: Cli) -> anyhow::Result<i32> {
    match cli.cmd {
        Command::Init(args) => cmd_init(args).await,
        Command::Run(args) => cmd_run(args).await,
        Command::Ci(args) => cmd_ci(args).await,
        Command::Validate(args) => validate::run(args).await,
        Command::Doctor(args) => doctor::run(args).await,
        Command::Quarantine(args) => cmd_quarantine(args).await,
        Command::Trace(args) => trace::cmd_trace(args).await,
        Command::Calibrate(args) => calibrate::cmd_calibrate(args).await,
        Command::Baseline(args) => match args.cmd {
            BaselineSub::Report(report_args) => {
                baseline::cmd_baseline_report(report_args).map(|_| exit_codes::OK)
            }
        },
        Command::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(exit_codes::OK)
        }
    }
}

async fn cmd_init(args: InitArgs) -> anyhow::Result<i32> {
    // 1. Basic Config
    write_sample_config_if_missing(&args.config)?;

    // 2. Gitignore
    if args.gitignore {
        write_file_if_missing(
            std::path::Path::new(".gitignore"),
            crate::templates::GITIGNORE,
        )?;
    }

    // 3. CI Scaffolding
    if args.ci {
        write_file_if_missing(
            std::path::Path::new("ci-eval.yaml"),
            crate::templates::CI_EVAL_YAML,
        )?;
        write_file_if_missing(
            std::path::Path::new("schemas/ci_answer.schema.json"),
            crate::templates::CI_SCHEMA_JSON,
        )?;
        write_file_if_missing(
            std::path::Path::new("traces/ci.jsonl"),
            crate::templates::CI_TRACES_JSONL,
        )?;
        write_file_if_missing(
            std::path::Path::new(".github/workflows/verdict.yml"),
            crate::templates::CI_WORKFLOW_YML,
        )?;
    }

    Ok(exit_codes::OK)
}

fn write_file_if_missing(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        std::fs::write(path, content)?;
        eprintln!("created {}", path.display());
    } else {
        eprintln!("note: {} already exists (skipped)", path.display());
    }
    Ok(())
}

fn write_sample_config_if_missing(path: &std::path::Path) -> anyhow::Result<()> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        verdict_core::config::write_sample_config(path)?;
        eprintln!("created {}", path.display());
    } else {
        eprintln!("note: {} already exists", path.display());
    }
    Ok(())
}

async fn cmd_run(args: RunArgs) -> anyhow::Result<i32> {
    ensure_parent_dir(&args.db)?;

    // Argument validation
    if args.baseline.is_some() && args.export_baseline.is_some() {
        eprintln!("config error: cannot use --baseline and --export-baseline together");
        return Ok(exit_codes::CONFIG_ERROR);
    }

    let cfg = verdict_core::config::load_config(&args.config).map_err(|e| anyhow::anyhow!(e))?;
    let store = verdict_core::storage::Store::open(&args.db)?;

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
        args.refresh_cache,
        &args.judge,
        &args.baseline,
        PathBuf::from(&args.config),
        args.replay_strict,
    )
    .await;

    let runner = match runner {
        Ok(r) => r,
        Err(e) => {
            if let Some(diag) = verdict_core::errors::try_map_error(&e) {
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
        let policy = verdict_core::redaction::RedactionPolicy::new(true);
        for row in &mut artifacts.results {
            policy.redact_judge_metadata(&mut row.details);
        }
    }

    verdict_core::report::json::write_json(&artifacts, &PathBuf::from("run.json"))?;
    verdict_core::report::console::print_summary(&artifacts.results, args.explain_skip);

    // PR11: Export baseline logic
    if let Some(path) = &args.export_baseline {
        export_baseline(path, &PathBuf::from(&args.config), &cfg, &artifacts.results)?;
    }

    Ok(decide_exit_code(&artifacts.results, args.strict))
}

async fn cmd_ci(args: CiArgs) -> anyhow::Result<i32> {
    ensure_parent_dir(&args.db)?;

    // Argument Validation
    if args.baseline.is_some() && args.export_baseline.is_some() {
        eprintln!("config error: cannot use --baseline and --export-baseline together");
        return Ok(exit_codes::CONFIG_ERROR);
    }

    // Shared Store for Auto-Ingest
    let store = verdict_core::storage::Store::open(&args.db)?;
    store.init_schema()?; // Ensure tables exist for ingest

    // In Strict Replay mode, we MUST ingest the trace into the DB
    // so that Agent Assertions (which query the DB) can find the episodes/steps.
    if args.replay_strict {
        if let Some(trace_path) = &args.trace_file {
            let stats = verdict_core::trace::ingest::ingest_into_store(&store, trace_path)
                .map_err(|e| anyhow::anyhow!("failed to ingest trace: {}", e))?;

            eprintln!(
                "auto-ingest: loaded {} events into {} (from {})",
                stats.event_count,
                args.db.display(),
                trace_path.display()
            );
        }
    }

    let cfg = verdict_core::config::load_config(&args.config).map_err(|e| anyhow::anyhow!(e))?;
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
        args.refresh_cache,
        &args.judge,
        &args.baseline,
        PathBuf::from(&args.config),
        args.replay_strict,
    )
    .await;

    let runner = match runner {
        Ok(r) => r,
        Err(e) => {
            if let Some(diag) = verdict_core::errors::try_map_error(&e) {
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
        let policy = verdict_core::redaction::RedactionPolicy::new(true);
        for row in &mut artifacts.results {
            policy.redact_judge_metadata(&mut row.details);
        }
    }

    verdict_core::report::junit::write_junit(&cfg.suite, &artifacts.results, &args.junit)?;
    verdict_core::report::sarif::write_sarif("verdict", &artifacts.results, &args.sarif)?;
    verdict_core::report::json::write_json(&artifacts, &PathBuf::from("run.json"))?;
    verdict_core::report::console::print_summary(&artifacts.results, args.explain_skip);

    let otel_cfg = verdict_core::otel::OTelConfig {
        jsonl_path: args.otel_jsonl.clone(),
        redact_prompts: args.redact_prompts,
    };
    let _ = verdict_core::otel::export_jsonl(&otel_cfg, &cfg.suite, &artifacts.results);

    // PR11: Export baseline logic
    if let Some(path) = &args.export_baseline {
        export_baseline(path, &PathBuf::from(&args.config), &cfg, &artifacts.results)?;
    }

    Ok(decide_exit_code(&artifacts.results, args.strict))
}

async fn cmd_quarantine(args: QuarantineArgs) -> anyhow::Result<i32> {
    ensure_parent_dir(&args.db)?;
    let store = verdict_core::storage::Store::open(&args.db)?;
    store.init_schema()?;
    let svc = verdict_core::quarantine::QuarantineService::new(store);

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
            eprintln!("quarantine list: TODO (skeleton)");
        }
    }
    Ok(exit_codes::OK)
}

fn decide_exit_code(results: &[verdict_core::model::TestResultRow], strict: bool) -> i32 {
    use verdict_core::model::TestStatus;

    if results.iter().any(|r| r.message.contains("config error:")) {
        return exit_codes::CONFIG_ERROR;
    }

    let has_fatal = results
        .iter()
        .any(|r| matches!(r.status, TestStatus::Fail | TestStatus::Error));

    if has_fatal {
        return exit_codes::TEST_FAILED;
    }

    if strict
        && results.iter().any(|r| {
            matches!(
                r.status,
                TestStatus::Warn | TestStatus::Flaky | TestStatus::Unstable
            )
        })
    {
        return exit_codes::TEST_FAILED;
    }

    exit_codes::OK
}

#[allow(clippy::too_many_arguments)]
async fn build_runner(
    store: verdict_core::storage::Store,
    trace_file: &Option<PathBuf>,
    cfg: &verdict_core::model::EvalConfig,
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
) -> anyhow::Result<verdict_core::engine::runner::Runner> {
    store.init_schema()?;
    let cache = verdict_core::cache::vcr::VcrCache::new(store.clone());

    use anyhow::Context;
    use verdict_core::providers::llm::fake::FakeClient;
    use verdict_core::providers::llm::LlmClient;
    use verdict_core::providers::trace::TraceClient;

    let mut client: Arc<dyn LlmClient + Send + Sync> = if let Some(trace_path) = trace_file {
        let trace_client = TraceClient::from_path(trace_path).context("failed to load trace")?;
        Arc::new(trace_client)
    } else {
        Arc::new(FakeClient::new(cfg.model.clone()))
    };

    // Strict Mode Wiring (LLM)
    if replay_strict && client.provider_name() != "trace" {
        use verdict_core::providers::strict::StrictLlmClient;
        client = Arc::new(StrictLlmClient::new(client));
    }

    let metrics = verdict_metrics::default_metrics();

    let replay_mode = trace_file.is_some();

    let rerun_failures = if replay_mode {
        if rerun_failures_arg > 0 {
            eprintln!("note: replay mode active; forcing --rerun-failures=0 for determinism");
        }
        0
    } else {
        rerun_failures_arg
    };

    let policy = verdict_core::engine::runner::RunPolicy {
        rerun_failures,
        quarantine_mode: verdict_core::quarantine::QuarantineMode::parse(quarantine_mode_str),
        replay_strict,
    };

    // Embedder construction
    use verdict_core::providers::embedder::{fake::FakeEmbedder, openai::OpenAIEmbedder, Embedder};

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
            use verdict_core::providers::strict::StrictEmbedder;
            embedder = Some(Arc::new(StrictEmbedder::new(inner)));
        }
    }

    // Judge Construction
    // ------------------
    let judge_config = verdict_core::judge::JudgeRuntimeConfig {
        enabled: judge_args.judge != "none" && !judge_args.no_judge,
        provider: judge_args.judge.clone(),
        model: judge_args.judge_model.clone(),
        samples: judge_args.judge_samples,
        temperature: judge_args.judge_temperature,
        max_tokens: judge_args.judge_max_tokens,
        refresh: judge_args.judge_refresh,
    };

    let mut judge_client: Option<Arc<dyn verdict_core::providers::llm::LlmClient>> =
        if !judge_config.enabled {
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
                        verdict_core::providers::llm::openai::OpenAIClient::new(
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
            use verdict_core::providers::strict::StrictLlmClient;
            judge_client = Some(Arc::new(StrictLlmClient::new(inner)));
        }
    }

    let judge_store = verdict_core::storage::judge_cache::JudgeCache::new(store.clone());
    let judge_service =
        verdict_core::judge::JudgeService::new(judge_config, judge_store, judge_client);

    // Load baseline if provided
    let baseline = if let Some(path) = baseline_arg {
        let b = verdict_core::baseline::Baseline::load(path)?;
        let fp = verdict_core::baseline::compute_config_fingerprint(&cfg_path);
        if let Err(e) = b.validate(&cfg.suite, &fp) {
            eprintln!("fatal: {}", e);
            return Err(anyhow::anyhow!("config error").context(e));
        }
        Some(b)
    } else {
        None
    };

    Ok(verdict_core::engine::runner::Runner {
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
    cfg: &verdict_core::model::EvalConfig,
    results: &[verdict_core::model::TestResultRow],
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
                    entries.push(verdict_core::baseline::BaselineEntry {
                        test_id: r.test_id.clone(),
                        metric: metric_name.clone(),
                        score,
                        meta: None, // Could add model info here if available
                    });
                }
            }
        }
    }

    let b = verdict_core::baseline::Baseline {
        schema_version: 1,
        suite: cfg.suite.clone(),
        verdict_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        config_fingerprint: verdict_core::baseline::compute_config_fingerprint(config_path),
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
impl verdict_core::providers::llm::LlmClient for DummyClient {
    async fn complete(
        &self,
        prompt: &str,
        _context: Option<&[String]>,
    ) -> anyhow::Result<verdict_core::model::LlmResponse> {
        let text = format!("hello from {} :: {}", self.model, prompt);
        Ok(verdict_core::model::LlmResponse {
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
