use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
#[command(about = "Generate policy from trace or profile")]
pub struct GenerateArgs {
    /// Input trace file (single-run mode)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Profile file (multi-run mode)
    #[arg(long)]
    pub profile: Option<PathBuf>,

    #[arg(short, long, default_value = "policy.yaml")]
    pub output: PathBuf,

    #[arg(long, default_value = "Generated Policy")]
    pub name: String,

    #[arg(long, default_value = "yaml")]
    pub format: String,

    #[arg(long)]
    pub dry_run: bool,

    /// Show policy diff versus existing output file
    #[arg(long)]
    pub diff: bool,

    // Single-run heuristics
    #[arg(long)]
    pub heuristics: bool,

    #[arg(long, default_value_t = 3.8)]
    pub entropy_threshold: f64,

    // Profile stability
    /// Minimum stability to auto-allow (profile mode)
    #[arg(long, default_value_t = 0.7)]
    pub min_stability: f64,

    /// Below this, mark as needs_review if --new-is-risky
    #[arg(long, default_value_t = 0.6)]
    pub review_threshold: f64,

    /// Treat low-stability items as risky (else skip them)
    #[arg(long)]
    pub new_is_risky: bool,

    /// Smoothing parameter (Laplace) for display
    #[arg(long, default_value_t = 1.0)]
    pub alpha: f64,

    /// Minimum runs before anything can be auto-allowed (safety belt)
    #[arg(long, default_value_t = 1)]
    pub min_runs: u32,

    /// Z-score for Wilson lower bound gating (1.96 ≈ 95% confidence)
    #[arg(long, default_value_t = 1.96)]
    pub wilson_z: f64,
}

impl GenerateArgs {
    pub fn validate(&self) -> Result<()> {
        if self.min_stability < 0.0 || self.min_stability > 1.0 {
            anyhow::bail!("--min-stability must be between 0.0 and 1.0");
        }
        if self.review_threshold < 0.0 || self.review_threshold > 1.0 {
            anyhow::bail!("--review-threshold must be between 0.0 and 1.0");
        }
        if self.min_stability < self.review_threshold {
            anyhow::bail!(
                "--min-stability ({}) must be >= --review-threshold ({})",
                self.min_stability,
                self.review_threshold
            );
        }
        if self.alpha <= 0.0 {
            anyhow::bail!("--alpha must be positive");
        }
        if self.wilson_z <= 0.0 {
            anyhow::bail!("--wilson-z must be positive");
        }
        if self.entropy_threshold < 0.0 {
            anyhow::bail!("--entropy-threshold must be non-negative");
        }
        Ok(())
    }
}
