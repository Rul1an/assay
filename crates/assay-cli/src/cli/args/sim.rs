use clap::Subcommand;

#[cfg(feature = "sim")]
#[derive(clap::Args, Clone, Debug)]
pub struct SimArgs {
    #[command(subcommand)]
    pub cmd: SimSub,
}

#[cfg(feature = "sim")]
#[derive(Subcommand, Clone, Debug)]
pub enum SimSub {
    /// Run an attack simulation suite
    Run(SimRunArgs),
    /// Run soak reliability simulation
    Soak(SimSoakArgs),
}

#[cfg(feature = "sim")]
#[derive(clap::Args, Clone, Debug)]
pub struct SimRunArgs {
    /// Simulation suite to run (quick, nightly, stress, chaos)
    #[arg(long, default_value = "quick")]
    pub suite: String,

    /// Specific attack vector to run (overrides suite)
    #[arg(long)]
    pub attack: Option<String>,

    /// Target bundle for simulation (not required with --print-config)
    #[arg(long, short, required_unless_present = "print_config")]
    pub target: Option<std::path::PathBuf>,

    /// Seed for reproducible mutations
    #[arg(long)]
    pub seed: Option<u64>,

    /// Number of iterations per attack
    #[arg(long)]
    pub iterations: Option<usize>,

    /// Path to write machine-readable JSON report
    #[arg(long)]
    pub report: Option<std::path::PathBuf>,

    /// Directory to write mutated artifacts for forensics
    #[arg(long, short)]
    pub output: Option<std::path::PathBuf>,

    /// Verification limits as JSON, or @path to load from file
    #[arg(long)]
    pub limits: Option<String>,

    /// Path to JSON file with limits (overrides --limits if both given)
    #[arg(long)]
    pub limits_file: Option<std::path::PathBuf>,

    /// Suite time budget in seconds (default: 60). Must be > 0.
    #[arg(long, default_value = "60")]
    pub time_budget: u64,

    /// Print effective limits and time budget, then exit
    #[arg(long)]
    pub print_config: bool,
}

#[cfg(feature = "sim")]
#[derive(clap::Args, Clone, Debug)]
pub struct SimSoakArgs {
    /// Number of iterations to run (default: 20). Must be > 0.
    #[arg(long, default_value = "20")]
    pub iterations: u32,

    /// RNG seed for deterministic runs (optional).
    #[arg(long)]
    pub seed: Option<u64>,

    /// Target identifier (e.g. bundle/scenario id)
    #[arg(long)]
    pub target: String,

    /// Output path for soak report JSON
    #[arg(long)]
    pub report: std::path::PathBuf,

    /// Suite time budget in seconds (default: 60). Must be > 0.
    #[arg(long, default_value = "60")]
    pub time_budget: u64,
}
