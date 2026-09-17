//! Policy command arguments.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::cli::commands::{generate::GenerateArgs, record::RecordArgs};

use super::common::OutputFormat;

#[derive(Args, Clone, Debug)]
pub struct PolicyArgs {
    #[command(subcommand)]
    pub cmd: PolicyCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub enum PolicyCommand {
    /// Generate policy from trace or profile input
    Generate(GenerateArgs),

    /// Capture runtime behavior and generate a policy
    Record(RecordArgs),

    /// Validate policy syntax and (v2) JSON Schemas
    Validate(PolicyValidateArgs),

    /// Migrate v1.x constraints policy to v2.0 schemas
    Migrate(PolicyMigrateArgs),

    /// Format policy YAML (normalizes formatting)
    Fmt(PolicyFmtArgs),

    /// Dump the resolved policy this Assay version would load
    Resolve(PolicyResolveArgs),

    /// Activate a validated policy into a policy root
    Activate(PolicyActivateArgs),

    /// Roll back an active policy to its previously activated version
    Rollback(PolicyRollbackArgs),

    /// Check active policy status and verify synchronization with activation history
    Status(PolicyStatusArgs),
}

#[derive(Args, Clone, Debug)]
pub struct PolicyValidateArgs {
    /// Policy file path (YAML)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Fail if deprecated v1 policy format is detected
    #[arg(long)]
    pub deny_deprecations: bool,

    /// Output format; JSON writes a machine-readable validation report to stdout
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

impl PolicyValidateArgs {
    pub(crate) fn is_json(&self) -> bool {
        self.format == OutputFormat::Json
    }
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

#[derive(Args, Clone, Debug)]
pub struct PolicyResolveArgs {
    /// Policy file path (YAML)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Output format; only json emits a document
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Args, Clone, Debug)]
pub struct PolicyActivateArgs {
    /// Source policy file path (YAML)
    #[arg(value_name = "SRC")]
    pub src: PathBuf,

    /// Policy root directory
    #[arg(
        long,
        visible_alias = "policy-root",
        alias = "policy-root",
        default_value = "."
    )]
    pub root: PathBuf,

    /// Target policy file name within the policy root
    #[arg(long = "as", visible_alias = "name")]
    pub as_name: Option<String>,
}

impl PolicyActivateArgs {
    pub fn target_name(&self) -> anyhow::Result<String> {
        if let Some(ref name) = self.as_name {
            Ok(name.clone())
        } else if let Some(file_name) = self.src.file_name() {
            Ok(file_name.to_string_lossy().to_string())
        } else {
            anyhow::bail!(
                "cannot determine policy name from source path {}",
                self.src.display()
            )
        }
    }
}

#[derive(Args, Clone, Debug)]
pub struct PolicyRollbackArgs {
    /// Policy file name within the policy root
    #[arg(value_name = "NAME")]
    pub name: String,

    /// Policy root directory
    #[arg(
        long,
        visible_alias = "policy-root",
        alias = "policy-root",
        default_value = "."
    )]
    pub root: PathBuf,
}

#[derive(Args, Clone, Debug)]
pub struct PolicyStatusArgs {
    /// Policy file name within the policy root
    #[arg(value_name = "NAME")]
    pub name: String,

    /// Policy root directory
    #[arg(
        long,
        visible_alias = "policy-root",
        alias = "policy-root",
        default_value = "."
    )]
    pub root: PathBuf,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}
