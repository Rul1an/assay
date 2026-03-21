use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Clone)]
pub struct InitArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,

    /// generate CI scaffolding (smoke test, traces, workflow)
    /// Pass a provider name (e.g. "github") or leave empty for default.
    #[arg(long, num_args = 0..=1, default_missing_value = "github")]
    pub ci: Option<String>,

    /// generate .gitignore for artifacts/db
    #[arg(long)]
    pub gitignore: bool,

    /// Starter preset to use: default | hardened | dev
    /// Backward-compatible alias: --pack
    #[arg(long = "preset", alias = "pack", default_value = "default")]
    pub preset: String,

    /// List available presets and exit
    /// Backward-compatible alias: --list-packs
    #[arg(long = "list-presets", alias = "list-packs")]
    pub list_presets: bool,

    /// Generate policy from an existing trace file (JSONL events)
    #[arg(long)]
    pub from_trace: Option<PathBuf>,

    /// Enable heuristics (entropy/risk analysis) when generating from trace
    #[arg(long, requires = "from_trace")]
    pub heuristics: bool,

    /// Generate a ready-to-run hello trace + smoke suite scaffold
    #[arg(long, conflicts_with = "from_trace")]
    pub hello_trace: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct ImportArgs {
    /// Input file (MCP transcript or Inspector JSON)
    pub input: std::path::PathBuf,

    /// Input format: inspector | jsonrpc | streamable-http | http-sse (alias: sse-legacy)
    #[arg(long, default_value = "inspector")]
    pub format: String,

    /// Generate initial eval config and policy
    #[arg(long)]
    pub init: bool,

    /// Output trace file path (default: derived from input name)
    #[arg(long)]
    pub out_trace: Option<std::path::PathBuf>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct MigrateArgs {
    #[arg(long, default_value = "mcp-eval.yaml")]
    pub config: std::path::PathBuf,

    /// Dry run (print to stdout instead of overwriting)
    #[arg(long)]
    pub dry_run: bool,

    /// Check if migration is needed (exit 2 if needed, 0 if clean)
    #[arg(long)]
    pub check: bool,
}

#[derive(Parser, Clone, Debug)]
pub struct DemoArgs {
    /// Output directory for demo files
    #[arg(long, default_value = "assay-demo")]
    pub out: PathBuf,
}

#[derive(Parser, Clone, Debug)]
pub struct InitCiArgs {
    /// CI Provider: github | gitlab
    #[arg(long, default_value = "github")]
    pub provider: String,

    /// Output path (default depends on provider)
    #[arg(long)]
    pub out: Option<PathBuf>,
}
