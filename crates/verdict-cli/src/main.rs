use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "verdict",
    version,
    about = "CI-first PR regression gate for RAG pipelines (skeleton)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    Run(RunArgs),
    Ci(CiArgs),
    Init(InitArgs),
    Quarantine(QuarantineArgs),
    Version,
}

#[derive(Parser, Clone)]
struct RunArgs {
    #[arg(long, default_value = "eval.yaml")]
    config: PathBuf,
    #[arg(long, default_value = ".eval/eval.db")]
    db: PathBuf,

    #[arg(long, default_value_t = 0)]
    rerun_failures: u32,

    /// quarantine mode: off|warn|strict
    #[arg(long, default_value = "warn")]
    quarantine_mode: String,

    #[arg(long)]
    trace_file: Option<PathBuf>,
}

#[derive(Parser, Clone)]
struct CiArgs {
    #[arg(long, default_value = "eval.yaml")]
    config: PathBuf,
    #[arg(long, default_value = ".eval/eval.db")]
    db: PathBuf,
    #[arg(long, default_value = "junit.xml")]
    junit: PathBuf,
    #[arg(long, default_value = "sarif.json")]
    sarif: PathBuf,

    #[arg(long, default_value_t = 1)]
    rerun_failures: u32,
    #[arg(long, default_value = "warn")]
    quarantine_mode: String,

    #[arg(long)]
    otel_jsonl: Option<PathBuf>,

    #[arg(long)]
    trace_file: Option<PathBuf>,
}

#[derive(Parser)]
struct InitArgs {
    #[arg(long, default_value = "eval.yaml")]
    config: PathBuf,
}

#[derive(Parser)]
struct QuarantineArgs {
    #[command(subcommand)]
    cmd: QuarantineSub,
    #[arg(long, default_value = ".eval/eval.db")]
    db: PathBuf,
    #[arg(long, default_value = "demo")]
    suite: String,
}

#[derive(Subcommand)]
enum QuarantineSub {
    Add {
        #[arg(long)]
        test_id: String,
        #[arg(long)]
        reason: String,
    },
    Remove {
        #[arg(long)]
        test_id: String,
    },
    List,
}

mod exit_codes {
    pub const OK: i32 = 0;
    pub const TEST_FAILED: i32 = 1;
    pub const CONFIG_ERROR: i32 = 2;
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let cli = Cli::parse();
    let code = match dispatch(cli).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fatal: {e:?}");
            exit_codes::CONFIG_ERROR
        }
    };
    std::process::exit(code);
}

async fn dispatch(cli: Cli) -> anyhow::Result<i32> {
    match cli.cmd {
        Command::Init(args) => cmd_init(args).await,
        Command::Run(args) => cmd_run(args).await,
        Command::Ci(args) => cmd_ci(args).await,
        Command::Quarantine(args) => cmd_quarantine(args).await,
        Command::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(exit_codes::OK)
        }
    }
}

async fn cmd_init(args: InitArgs) -> anyhow::Result<i32> {
    verdict_core::config::write_sample_config(&args.config)?;
    eprintln!("wrote {}", args.config.display());
    Ok(exit_codes::OK)
}

async fn cmd_run(args: RunArgs) -> anyhow::Result<i32> {
    ensure_parent_dir(&args.db)?;
    let cfg = verdict_core::config::load_config(&args.config).map_err(|e| anyhow::anyhow!(e))?;

    let store = verdict_core::storage::Store::open(&args.db)?;
    store.init_schema()?;
    let cache = verdict_core::cache::vcr::VcrCache::new(store.clone());

    let client: Arc<dyn verdict_core::providers::llm::LlmClient> =
        if let Some(trace_path) = &args.trace_file {
            Arc::new(
                verdict_core::providers::trace::TraceClient::from_path(trace_path)
                    .map_err(|e| anyhow::anyhow!(e))?,
            )
        } else {
            Arc::new(DummyClient::new(&cfg.model))
        };
    let metrics = verdict_metrics::default_metrics();

    let policy = verdict_core::engine::runner::RunPolicy {
        rerun_failures: args.rerun_failures,
        quarantine_mode: verdict_core::quarantine::QuarantineMode::parse(&args.quarantine_mode),
    };

    let runner = verdict_core::engine::runner::Runner {
        store,
        cache,
        client,
        metrics,
        policy,
    };
    let artifacts = runner.run_suite(&cfg).await?;

    verdict_core::report::console::print_summary(&artifacts.results);
    Ok(decide_exit_code(&artifacts.results))
}

async fn cmd_ci(args: CiArgs) -> anyhow::Result<i32> {
    ensure_parent_dir(&args.db)?;
    let cfg = verdict_core::config::load_config(&args.config).map_err(|e| anyhow::anyhow!(e))?;

    let store = verdict_core::storage::Store::open(&args.db)?;
    store.init_schema()?;
    let cache = verdict_core::cache::vcr::VcrCache::new(store.clone());

    let client: Arc<dyn verdict_core::providers::llm::LlmClient> =
        if let Some(trace_path) = &args.trace_file {
            Arc::new(
                verdict_core::providers::trace::TraceClient::from_path(trace_path)
                    .map_err(|e| anyhow::anyhow!(e))?,
            )
        } else {
            Arc::new(DummyClient::new(&cfg.model))
        };
    let metrics = verdict_metrics::default_metrics();

    let policy = verdict_core::engine::runner::RunPolicy {
        rerun_failures: args.rerun_failures,
        quarantine_mode: verdict_core::quarantine::QuarantineMode::parse(&args.quarantine_mode),
    };

    let runner = verdict_core::engine::runner::Runner {
        store: store.clone(),
        cache,
        client,
        metrics,
        policy,
    };
    let artifacts = runner.run_suite(&cfg).await?;

    verdict_core::report::junit::write_junit(&cfg.suite, &artifacts.results, &args.junit)?;
    verdict_core::report::sarif::write_sarif("verdict", &artifacts.results, &args.sarif)?;
    verdict_core::report::json::write_json(&artifacts, &PathBuf::from("run.json"))?;

    let otel_cfg = verdict_core::otel::OTelConfig {
        jsonl_path: args.otel_jsonl.clone(),
        redact_prompts: false,
    };
    let _ = verdict_core::otel::export_jsonl(&otel_cfg, &cfg.suite, &artifacts.results);

    Ok(decide_exit_code(&artifacts.results))
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

fn decide_exit_code(results: &[verdict_core::model::TestResultRow]) -> i32 {
    use verdict_core::model::TestStatus;
    let mut has_fail = false;
    let mut has_error = false;

    for r in results {
        match r.status {
            TestStatus::Pass | TestStatus::Warn | TestStatus::Flaky => {}
            TestStatus::Fail => has_fail = true,
            TestStatus::Error => has_error = true,
        }
    }

    if has_error || has_fail {
        exit_codes::TEST_FAILED
    } else {
        exit_codes::OK
    }
}

fn ensure_parent_dir(path: &std::path::Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
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
