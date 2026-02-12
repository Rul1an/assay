//! Run and CI command arguments.

use std::path::PathBuf;

use clap::Parser;

use super::JudgeArgs;

#[derive(Parser, Clone)]
pub struct RunArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,

    #[arg(long, default_value_t = 0)]
    pub rerun_failures: u32,

    /// quarantine mode: off|warn|strict (controls status of quarantined tests)
    #[arg(long, default_value = "warn")]
    pub quarantine_mode: String,

    /// Trace file to use as Source of Truth for replay (auto-ingested in strict mode)
    #[arg(long)]
    pub trace_file: Option<PathBuf>,

    #[arg(long)]
    pub redact_prompts: bool,

    #[arg(long)]
    pub baseline: Option<PathBuf>,

    #[arg(long)]
    pub export_baseline: Option<PathBuf>,

    /// strict mode (controls exit code policy: warn/flaky -> exit 1)
    #[arg(long)]
    pub strict: bool,

    /// embedder provider (none|openai|fake)
    #[arg(long, default_value = "none")]
    pub embedder: String,

    /// embedding model name
    #[arg(long, default_value = "text-embedding-3-small")]
    pub embedding_model: String,

    /// force refresh of embeddings (ignore cache)
    #[arg(long)]
    pub refresh_embeddings: bool,

    /// enable incremental execution (skip passing tests with same fingerprint)
    #[arg(long)]
    pub incremental: bool,

    /// ignore incremental cache (force re-run)
    #[arg(long)]
    pub refresh_cache: bool,

    /// Explicitly disable cache usage (alias for --refresh-cache)
    #[arg(long)]
    pub no_cache: bool,

    /// show details for skipped tests
    #[arg(long)]
    pub explain_skip: bool,

    #[command(flatten)]
    pub judge: JudgeArgs,

    /// strict replay mode: use trace-file as truth, forbid network, auto-ingest to DB
    #[arg(long)]
    pub replay_strict: bool,

    /// Fail if deprecated v1 policy format is detected
    #[arg(long)]
    pub deny_deprecations: bool,

    /// Exit code compatibility mode: v1 (legacy) or v2 (standard)
    /// defaults to v2 (trace not found = 2)
    #[arg(long, value_enum, default_value_t, env = "ASSAY_EXIT_CODES")]
    pub exit_codes: crate::exit_codes::ExitCodeVersion,

    /// Disable signature verification (UNSAFE); recorded in summary.json as verify_enabled: false
    #[arg(long)]
    pub no_verify: bool,
}

impl Default for RunArgs {
    fn default() -> Self {
        Self {
            config: PathBuf::from("eval.yaml"),
            db: PathBuf::from(".eval/eval.db"),
            rerun_failures: 0,
            quarantine_mode: "warn".to_string(),
            trace_file: None,
            redact_prompts: false,
            baseline: None,
            export_baseline: None,
            strict: false,
            embedder: "none".to_string(),
            embedding_model: "text-embedding-3-small".to_string(),
            refresh_embeddings: false,
            incremental: false,
            refresh_cache: false,
            no_cache: false,
            explain_skip: false,
            judge: JudgeArgs::default(),
            replay_strict: false,
            deny_deprecations: false,
            exit_codes: crate::exit_codes::ExitCodeVersion::default(),
            no_verify: false,
        }
    }
}

#[derive(Parser, Clone)]
pub struct CiArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,
    #[arg(long, default_value = "junit.xml")]
    pub junit: PathBuf,
    #[arg(long, default_value = "sarif.json")]
    pub sarif: PathBuf,

    #[arg(long, default_value_t = 2)]
    pub rerun_failures: u32,
    #[arg(long, default_value = "warn")]
    pub quarantine_mode: String,

    #[arg(long)]
    pub otel_jsonl: Option<PathBuf>,

    /// Trace file to use as Source of Truth for replay (auto-ingested in strict mode)
    #[arg(long)]
    pub trace_file: Option<PathBuf>,

    #[arg(long)]
    pub redact_prompts: bool,

    #[arg(long)]
    pub baseline: Option<PathBuf>,

    #[arg(long)]
    pub export_baseline: Option<PathBuf>,

    /// strict mode (controls exit code policy: warn/flaky -> exit 1)
    #[arg(long)]
    pub strict: bool,

    #[arg(long, default_value = "none")]
    pub embedder: String,

    #[arg(long, default_value = "text-embedding-3-small")]
    pub embedding_model: String,

    #[arg(long)]
    pub refresh_embeddings: bool,

    /// enable incremental execution (skip passing tests with same fingerprint)
    #[arg(long)]
    pub incremental: bool,

    /// ignore incremental cache (force re-run)
    #[arg(long)]
    pub refresh_cache: bool,

    /// Explicitly disable cache usage (alias for --refresh-cache)
    #[arg(long)]
    pub no_cache: bool,

    /// show details for skipped tests
    #[arg(long)]
    pub explain_skip: bool,

    #[command(flatten)]
    pub judge: JudgeArgs,

    /// strict replay mode: use trace-file as truth, forbid network, auto-ingest to DB
    #[arg(long)]
    pub replay_strict: bool,

    /// Fail if deprecated v1 policy format is detected
    #[arg(long)]
    pub deny_deprecations: bool,

    /// Exit code compatibility mode: v1 (legacy) or v2 (standard)
    #[arg(long, value_enum, default_value_t, env = "ASSAY_EXIT_CODES")]
    pub exit_codes: crate::exit_codes::ExitCodeVersion,

    /// Disable signature verification (UNSAFE); recorded in summary.json as verify_enabled: false
    #[arg(long)]
    pub no_verify: bool,

    /// Write PR comment body (markdown) to file for GitHub Actions
    #[arg(long)]
    pub pr_comment: Option<PathBuf>,
}
