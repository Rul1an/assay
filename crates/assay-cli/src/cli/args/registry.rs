//! `assay registry` — registry-carrier commands.
//!
//! Intentionally narrow: this slice exposes only the supply-chain-conformance carrier emitter
//! (A5a-1). It is not a general registry platform.

use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct RegistryArgs {
    #[command(subcommand)]
    pub sub: RegistrySub,
}

#[derive(Subcommand, Debug)]
pub enum RegistrySub {
    /// Emit the assay.supply_chain_conformance.v0 carrier from a local input descriptor.
    ///
    /// Performs offline checks over the supplied inputs and reports carrier status. It does not
    /// assert supply-chain safety, policy approval, compliance, Sigstore trust, Rekor inclusion,
    /// issuer identity, or artifact runtime integrity.
    #[command(name = "supply-chain-conformance")]
    SupplyChainConformance(SupplyChainConformanceArgs),
}

#[derive(Args, Debug)]
pub struct SupplyChainConformanceArgs {
    /// Path to the input descriptor (an `assay.supply_chain_conformance.input.v0` JSON file).
    #[arg(long)]
    pub input: String,

    /// Output path for the emitted carrier JSON; `-` (default) writes to stdout.
    #[arg(long, default_value = "-")]
    pub out: String,

    /// Affirm offline operation. The emitter performs no network I/O by construction; this guard is
    /// accepted for recipe explicitness and is required to hard-fail before any fetch if a
    /// fetch-capable path is ever introduced.
    #[arg(long)]
    pub offline: bool,
}
