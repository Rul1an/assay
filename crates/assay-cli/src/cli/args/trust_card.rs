use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct TrustCardArgs {
    #[command(subcommand)]
    pub cmd: TrustCardSub,
}

#[derive(Subcommand, Debug)]
pub enum TrustCardSub {
    /// Generate trustcard.json and trustcard.md from a verified evidence bundle
    Generate(TrustCardGenerateArgs),
}

#[derive(Args, Debug, Clone)]
pub struct TrustCardGenerateArgs {
    /// Evidence bundle archive (.tar.gz)
    #[arg(value_name = "BUNDLE")]
    pub bundle: PathBuf,

    /// Output directory for trustcard.json and trustcard.md
    #[arg(long = "out-dir", value_name = "DIR")]
    pub out_dir: PathBuf,

    /// Comma-separated pack references to execute while classifying pack findings
    #[arg(long, value_delimiter = ',')]
    pub pack: Option<Vec<String>>,

    /// Maximum lint results considered when pack execution is enabled
    #[arg(long, default_value = "500")]
    pub max_results: usize,
}
