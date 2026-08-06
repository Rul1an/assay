//! Baseline command arguments.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use super::OutputFormat;

#[derive(Parser, Clone)]
pub struct BaselineArgs {
    #[command(subcommand)]
    pub cmd: BaselineSub,
}

#[derive(Subcommand, Clone)]
pub enum BaselineSub {
    Report(BaselineReportArgs),
    /// Record the latest run as a baseline
    Record(BaselineRecordArgs),
    /// Check the latest run against a baseline
    Check(BaselineCheckArgs),
}

#[derive(Parser, Clone)]
pub struct BaselineRecordArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,

    /// Test suite name (if omitted, inferred from config)
    #[arg(long)]
    pub suite: Option<String>,

    /// Run ID to record (default: latest for suite)
    #[arg(long)]
    pub run_id: Option<String>,

    /// Output path
    #[arg(long, default_value = "assay-baseline.json")]
    pub out: PathBuf,
}

#[derive(Parser, Clone)]
pub struct BaselineCheckArgs {
    #[arg(long, default_value = "eval.yaml")]
    pub config: PathBuf,
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,

    /// Test suite name (if omitted, inferred from config)
    #[arg(long)]
    pub suite: Option<String>,

    /// Run ID to check (default: latest for suite)
    #[arg(long)]
    pub run_id: Option<String>,

    /// Baseline path
    #[arg(long, default_value = "assay-baseline.json")]
    pub baseline: PathBuf,

    /// Fail on regression
    #[arg(long, default_value_t = true)]
    pub fail_on_regression: bool,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

#[derive(Parser, Clone)]
pub struct BaselineReportArgs {
    #[arg(long, default_value = ".eval/eval.db")]
    pub db: PathBuf,

    /// Test suite name
    #[arg(long)]
    pub suite: String,

    /// Number of recent runs to include
    #[arg(long, default_value_t = 50)]
    pub last: u32,

    /// Output path (JSON or Markdown based on extension or format)
    #[arg(long, default_value = "hygiene.json")]
    pub out: PathBuf,

    /// Output format.
    #[arg(long, default_value = "json")]
    pub format: BaselineReportFormat,
}

/// The shapes `baseline report` can write.
///
/// It was a bare `String` matched with `==`, and the final `else` wrote JSON with a
/// `// Default to JSON` comment -- so `--format mdd` produced a JSON file at `hygiene.md`, at
/// exit 0, with no note that the spelling was not honoured. That is the class #2039 exists to
/// close: a fixed set of meanings carried as a string and enumerated at the point of use.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BaselineReportFormat {
    #[default]
    Json,
    Md,
}
