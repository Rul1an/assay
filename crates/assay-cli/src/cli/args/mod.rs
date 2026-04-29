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
    Run(RunArgs),
    Ci(CiArgs),
    Init(InitArgs),
    Quarantine(QuarantineArgs),
    Trace(TraceArgs),
    Calibrate(CalibrateArgs),
    Baseline(BaselineArgs),
    Validate(ValidateArgs),
    Doctor(DoctorArgs),
    /// Watch config/policy/trace files and rerun on changes
    Watch(WatchArgs),
    Import(ImportArgs),
    Migrate(MigrateArgs),
    Coverage(CoverageArgs),
    Explain(super::commands::explain::ExplainArgs),
    Demo(DemoArgs),
    InitCi(InitCiArgs),
    Fix(FixArgs),
    /// Experimental: MCP Process Wrapper
    #[command(hide = true)]
    Mcp(McpArgs),
    Version,
    Policy(PolicyArgs),
    /// Discover MCP servers on this machine (v1.8)
    Discover(DiscoverArgs),
    /// Kill/Terminate MCP servers (v1.8)
    Kill(super::commands::kill::KillArgs),
    /// Runtime eBPF Monitor (Linux only)
    Monitor(super::commands::monitor::MonitorArgs),
    /// Learning Mode: Generate policy from trace or profile
    Generate(super::commands::generate::GenerateArgs),
    /// Learning Mode: Capture and Generate in one flow
    Record(super::commands::record::RecordArgs),
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
    Tool(ToolArgs),
    /// Generate canonical trust-basis artifacts from verified evidence bundles
    #[command(name = "trust-basis")]
    TrustBasis(TrustBasisArgs),
    /// Generate trust card artifacts (trustcard.json + trustcard.md) from verified bundles
    #[command(name = "trustcard")]
    TrustCard(TrustCardArgs),
}

#[derive(Parser, Debug)]
pub struct ToolArgs {
    #[command(subcommand)]
    pub cmd: super::commands::tool::ToolCmd,
}

#[cfg(test)]
mod tests;
