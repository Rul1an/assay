//! Policy compilation into enforcement tiers.
//!
//! The public surface stays on this module; implementation details live under
//! `tiers_next` so downstream users can keep importing `assay_policy::tiers::*`.

#[path = "tiers_next/mod.rs"]
mod tiers_next;

pub use tiers_next::{
    compile, CidrRule, CompilationStats, CompiledPolicy, DestRule, FilePolicy, GlobRule, InodeRule,
    NetworkPolicy, PathRule, Policy, PortRule, ProcessPolicy, Tier1Rules, Tier2Rules,
};
