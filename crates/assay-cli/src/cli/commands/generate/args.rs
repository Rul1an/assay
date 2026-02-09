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
        Self::validate_finite("--min-stability", self.min_stability)?;
        Self::validate_finite("--review-threshold", self.review_threshold)?;
        Self::validate_finite("--alpha", self.alpha)?;
        Self::validate_finite("--wilson-z", self.wilson_z)?;
        Self::validate_finite("--entropy-threshold", self.entropy_threshold)?;

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

    fn validate_finite(flag: &str, value: f64) -> Result<()> {
        if value.is_finite() {
            Ok(())
        } else {
            anyhow::bail!("{flag} must be finite");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GenerateArgs;
    use std::path::PathBuf;

    fn valid_args() -> GenerateArgs {
        GenerateArgs {
            input: None,
            profile: None,
            output: PathBuf::from("policy.yaml"),
            name: "Generated Policy".to_string(),
            format: "yaml".to_string(),
            dry_run: false,
            diff: false,
            heuristics: false,
            entropy_threshold: 3.8,
            min_stability: 0.7,
            review_threshold: 0.6,
            new_is_risky: false,
            alpha: 1.0,
            min_runs: 1,
            wilson_z: 1.96,
        }
    }

    #[test]
    fn validate_rejects_nan_min_stability() {
        let mut args = valid_args();
        args.min_stability = f64::NAN;
        let err = args.validate().expect_err("NaN must be rejected");
        assert!(err.to_string().contains("--min-stability must be finite"));
    }

    #[test]
    fn validate_rejects_infinite_alpha() {
        let mut args = valid_args();
        args.alpha = f64::INFINITY;
        let err = args
            .validate()
            .expect_err("infinite alpha must be rejected");
        assert!(err.to_string().contains("--alpha must be finite"));
    }

    #[test]
    fn validate_accepts_finite_values() {
        let args = valid_args();
        args.validate().expect("finite defaults should pass");
    }
}
