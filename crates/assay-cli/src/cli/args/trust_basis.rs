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
