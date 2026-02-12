//! Policy command arguments.

use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Args, Clone, Debug)]
pub struct PolicyArgs {
    #[command(subcommand)]
    pub cmd: PolicyCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub enum PolicyCommand {
    /// Validate policy syntax and (v2) JSON Schemas
    Validate(PolicyValidateArgs),

    /// Migrate v1.x constraints policy to v2.0 schemas
    Migrate(PolicyMigrateArgs),

    /// Format policy YAML (normalizes formatting)
    Fmt(PolicyFmtArgs),
}

#[derive(Args, Clone, Debug)]
pub struct PolicyValidateArgs {
    /// Policy file path (YAML)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Fail if deprecated v1 policy format is detected
    #[arg(long)]
    pub deny_deprecations: bool,
}

#[derive(Args, Clone, Debug)]
pub struct PolicyMigrateArgs {
    /// Input policy file (v1.x or v2.0)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Output file (default: overwrite input)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Dry run (print to stdout instead of overwriting)
    #[arg(long)]
    pub dry_run: bool,

    /// Preview only (no write)
    #[arg(long)]
    pub check: bool,
}

#[derive(Args, Clone, Debug)]
pub struct PolicyFmtArgs {
    /// Policy file path (YAML)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Output file (default: overwrite input)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}
