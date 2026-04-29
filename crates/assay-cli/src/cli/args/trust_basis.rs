use super::common::OutputFormat;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct TrustBasisArgs {
    #[command(subcommand)]
    pub cmd: TrustBasisSub,
}

#[derive(Subcommand, Debug)]
pub enum TrustBasisSub {
    /// Generate canonical trust-basis.json from a verified evidence bundle
    Generate(TrustBasisGenerateArgs),
    /// Compare two canonical trust-basis.json artifacts
    Diff(TrustBasisDiffArgs),
    /// Assert required claim levels in one canonical trust-basis.json artifact
    Assert(TrustBasisAssertArgs),
}

#[derive(Args, Debug, Clone)]
pub struct TrustBasisGenerateArgs {
    /// Evidence bundle archive (.tar.gz)
    #[arg(value_name = "BUNDLE")]
    pub bundle: PathBuf,

    /// Optional output path for canonical trust-basis.json (defaults to stdout)
    #[arg(long, short = 'o')]
    pub out: Option<PathBuf>,

    /// Comma-separated pack references to execute while classifying pack findings
    #[arg(long, value_delimiter = ',')]
    pub pack: Option<Vec<String>>,

    /// Maximum lint results considered when pack execution is enabled
    #[arg(long, default_value = "500")]
    pub max_results: usize,
}

#[derive(Args, Debug, Clone)]
pub struct TrustBasisDiffArgs {
    /// Baseline trust-basis.json
    #[arg(value_name = "BASELINE")]
    pub baseline: PathBuf,

    /// Candidate trust-basis.json
    #[arg(value_name = "CANDIDATE")]
    pub candidate: PathBuf,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Exit non-zero when the candidate removes or lowers a baseline claim
    #[arg(long)]
    pub fail_on_regression: bool,
}

#[derive(Args, Debug, Clone)]
pub struct TrustBasisAssertArgs {
    /// Trust Basis JSON artifact produced by `assay trust-basis generate`
    #[arg(long, short = 'i', value_name = "TRUST_BASIS")]
    pub input: PathBuf,

    /// Required claim level, formatted as <claim-id>=<level>
    #[arg(long = "require", value_name = "CLAIM=LEVEL", required = true)]
    pub requirements: Vec<String>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}
