//! Bundle and Replay command arguments.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
pub struct BundleArgs {
    #[command(subcommand)]
    pub cmd: BundleSub,
}

#[derive(Subcommand, Debug)]
pub enum BundleSub {
    /// Create replay bundle from run artifacts
    Create(BundleCreateArgs),
    /// Verify replay bundle integrity and safety
    Verify(BundleVerifyArgs),
}

#[derive(Args, Debug, Clone)]
pub struct BundleCreateArgs {
    /// Source run path (directory or run.json)
    #[arg(long)]
    pub from: Option<PathBuf>,

    /// Source run id (used for selection and default output naming)
    #[arg(long, conflicts_with = "from")]
    pub run_id: Option<String>,

    /// Output replay bundle path (default: .assay/bundles/<run_id>.tar.gz)
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Optional config file to include in bundle
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Optional trace file to include in bundle
    #[arg(long)]
    pub trace_file: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct BundleVerifyArgs {
    /// Replay bundle archive (.tar.gz)
    #[arg(long)]
    pub bundle: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct ReplayArgs {
    /// Replay bundle archive (.tar.gz)
    #[arg(long)]
    pub bundle: PathBuf,

    /// Allow live provider calls during replay (default: offline hermetic)
    #[arg(long)]
    pub live: bool,

    /// Override replay seed (order seed)
    #[arg(long)]
    pub seed: Option<u64>,

    /// Exit code compatibility mode
    #[arg(long, value_enum, default_value_t, env = "ASSAY_EXIT_CODES")]
    pub exit_codes: crate::exit_codes::ExitCodeVersion,
}
