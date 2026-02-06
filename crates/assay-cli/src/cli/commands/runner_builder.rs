use super::super::args::JudgeArgs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn build_runner(
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

    // Observability (OTel GenAI) - E8
    if let Err(e) = cfg.otel.validate() {
        return Err(anyhow::anyhow!("telemetry config error: {}", e));
    }
    use assay_core::providers::llm::tracing::TracingLlmClient;
    client = Arc::new(TracingLlmClient::new(client, cfg.otel.clone()));

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
        reliability: cfg
            .settings
            .judge
            .as_ref()
            .map(|j| j.reliability.clone())
            .unwrap_or_default(),
        system_prompt_version: String::new(),
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

pub(crate) fn ensure_parent_dir(path: &std::path::Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[derive(Clone)]
pub(crate) struct DummyClient {
    model: String,
}

impl DummyClient {
    pub(crate) fn new(model: &str) -> Self {
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
