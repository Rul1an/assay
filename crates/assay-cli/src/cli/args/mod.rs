use clap::{Parser, Subcommand};

pub mod baseline;
pub mod bundle;
pub mod common;
pub mod coverage;
pub mod evidence;
pub mod import;
pub mod mcp;
pub mod policy;
pub mod replay;
pub mod run;
pub mod runtime;
pub mod sim;
pub mod trust_basis;
pub mod trust_card;

pub use baseline::*;
pub use bundle::*;
pub use common::*;
pub use coverage::*;
pub use evidence::*;
pub use import::*;
pub use mcp::*;
pub use policy::*;
pub use replay::*;
pub use run::*;
pub use runtime::*;
pub use sim::*;
pub use trust_basis::*;
pub use trust_card::*;

#[derive(Parser)]
#[command(
    name = "assay",
    version,
    about = "CI-native evidence and trust compiler for agent runtime governance"
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run an evaluation suite and write run artifacts
    Run(RunArgs),
    /// Run the CI gate and emit CI report artifacts
    Ci(CiArgs),
    /// Create starter Assay config and trace fixtures
    Init(InitArgs),
    /// Manage quarantined or flaky tests
    Quarantine(QuarantineArgs),
    /// Inspect or transform trace inputs
    Trace(TraceArgs),
    /// Calibrate thresholds from previous run artifacts
    Calibrate(CalibrateArgs),
    /// Record or compare score baselines
    Baseline(BaselineArgs),
    /// Validate config and trace files without a full run
    Validate(ValidateArgs),
    /// Diagnose local setup, config, and trace health
    Doctor(DoctorArgs),
    /// Watch config/policy/trace files and rerun on changes
    Watch(WatchArgs),
    /// Import external artifacts into Assay-compatible data
    Import(ImportArgs),
    /// Migrate older config or policy formats
    Migrate(MigrateArgs),
    /// Report policy and trace coverage
    Coverage(CoverageArgs),
    /// Explain a test result or trace decision
    Explain(super::commands::explain::ExplainArgs),
    /// Generate and run the local demo project
    Demo(DemoArgs),
    /// Generate CI workflow scaffolding
    InitCi(InitCiArgs),
    /// Apply supported automatic fixes
    Fix(FixArgs),
    /// MCP runtime, discovery, kill-switch, and tool signing commands
    Mcp(McpArgs),
    /// Print Assay version information
    Version,
    /// Validate, format, and migrate policies
    Policy(PolicyArgs),
    /// Discover MCP servers on this machine (v1.8)
    #[command(hide = true)]
    Discover(DiscoverArgs),
    /// Kill/Terminate MCP servers (v1.8)
    #[command(hide = true)]
    Kill(super::commands::kill::KillArgs),
    /// Runtime eBPF Monitor (Linux only)
    Monitor(super::commands::monitor::MonitorArgs),
    /// Learning Mode: Generate policy from trace or profile
    Generate(super::commands::generate::GenerateArgs),
    /// Learning Mode: Capture and Generate in one flow
    Record(super::commands::record::RecordArgs),
    /// Internal Assay-Runner Phase 1 spike command
    #[cfg(feature = "runner")]
    #[command(name = "runner-spike", hide = true)]
    RunnerSpike(super::commands::runner_spike::RunnerSpikeArgs),
    /// Manage multi-run profiles for stability analysis
    Profile(super::commands::profile::ProfileArgs),
    /// Secure execution sandbox (v0.1)
    Sandbox(SandboxArgs),
    /// Evidence bundles, imports, verification, and stores
    Evidence(EvidenceArgs),
    /// Replay bundle management (create/verify)
    Bundle(BundleArgs),
    /// Replay a run from a replay bundle
    Replay(ReplayArgs),
    /// Attack Simulation (Hardening/Compliance)
    #[cfg(feature = "sim")]
    Sim(SimArgs),
    /// Interactive installer and environment setup (Phase 2)
    Setup(SetupArgs),
    /// Tool signing and verification
    #[command(hide = true)]
    Tool(ToolArgs),
    /// Generate canonical trust-basis artifacts from verified evidence bundles
    #[command(name = "trust-basis")]
    TrustBasis(TrustBasisArgs),
    /// Generate trust card artifacts (JSON, Markdown, and static HTML) from verified bundles
    #[command(name = "trust-card", alias = "trustcard")]
    TrustCard(TrustCardArgs),
}

#[derive(Parser, Debug)]
pub struct ToolArgs {
    #[command(subcommand)]
    pub cmd: super::commands::tool::ToolCmd,
}

#[cfg(test)]
mod tests;
