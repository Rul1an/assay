//! Shared vocabulary for coverage-aware claim gates (ADR-048).
//!
//! Only the vocabulary lives here. The decision tables that consume it stay in their domain
//! crates — `RunnerClaimGate::for_verdict` and `CoverageDescriptor::claim_decision_for` in
//! `assay-runner-schema`, `coding_agent_claim_decision` in `assay-evidence` — because each is a
//! domain reading of the same lattice and a shared table would merge those readings into one word.
//! What moved is the construction: two crates spelling the same three members twice would let one
//! of them silently mean something different, and the first evidence-side draft did exactly that.
//!
//! Neither enum derives `Ord`. The one comparison the tree makes between decisions is
//! `assay_runner_schema::permissiveness`, a free function next to the invariant that needs it; an
//! ordering on the public type would invite arithmetic on it. Neither enum is `#[non_exhaustive]`:
//! a fourth member changes what every table means and must cost a major, not be absorbed.

use serde::{Deserialize, Serialize};

/// What a consumer may conclude, once a claim kind has been checked against how a run was
/// observed. The tables that write this are meant to keep an absence claim no more permissive
/// than an occurrence claim. That is pinned for `RunnerClaimGate::for_verdict`
/// (`claim_support_parity.rs`) and for the descriptor/evidence pair on corresponding inputs
/// (`claim_gate_parity.rs`). It is not pinned for `CoverageDescriptor::claim_decision_for_effect`,
/// which can degrade a positive claim for an effect class the descriptor does not observe while
/// the same complete descriptor still allows the absence claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimDecision {
    /// The observation supports the claim as made.
    Allowed,
    /// The claim may be made, but on a weaker footing than the consumer asked for — records were
    /// dropped, the account is the subject's own, coverage is partial, or the effect class was not
    /// observed. Which of those applies is the table's reading, carried in its `rule` and reason.
    Degraded,
    /// The observation cannot support the claim; no rung or basis applies.
    Blocked,
}

/// What kind of claim a consumer wants to make about one dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    /// "this effect happened" — seeing part of a run is enough to say what was seen.
    PositiveExistence,
    /// "these are all of them" — needs coverage of the whole dimension.
    ExhaustiveSet,
    /// "this did not happen" — the claim a blind spot silently destroys.
    BoundedNegative,
}
