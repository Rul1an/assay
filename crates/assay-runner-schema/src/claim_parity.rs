//! The health half of the claim-support rule, stated as a table.
//!
//! Two questions look like one and are not. [`RunnerClaimGate`] answers *given how healthy this
//! run's observation was, which claim kinds may I make*. A separate rule — not in this crate, and
//! today not in the product at all — has to answer *given this observer class and its declared probe
//! set, which claim kinds can it support at all*. The first is the **health half**, the second the
//! **class half**. Only the health half ships.
//!
//! This module exists so the class half can be pinned against the health half instead of
//! approximating it. Per the CLAUDE.md one-rule-one-function posture, a parity test is the
//! sanctioned fallback when one rule cannot call the other, and a rule that has to cross a language
//! boundary cannot. So the table is `pub` and serializable rather than test-local: a conformance
//! fixture in another language recomputes these cells and compares, exactly as
//! `assay-evidence`'s `tests/claim_gate_parity.rs` does across two Rust vocabularies.
//!
//! **The table derives every decision from [`RunnerClaimGate`]; it never restates one.** A table
//! that hand-wrote the fifteen cells would be the second implementation this file exists to prevent.
//!
//! ## What building it surfaced
//!
//! [`CoverageClaimKind`] has three kinds. [`RunnerClaimGate`] has fields for two of them:
//! `measured_positive_claims` and `bounded_negative_claims`. There is **no field for
//! [`CoverageClaimKind::ExhaustiveSet`]**, so this table reports it as [`ClaimSupport::NotModelled`]
//! rather than folding it onto a neighbouring field. That is deliberate. Folding it onto
//! `measured_positive_claims` would read an exhaustive-set claim as merely degraded under
//! `Clipped`, when dropped ring-buffer records are precisely the thing that falsifies an
//! exhaustive set. [`crate::CoverageDescriptor::claim_decision`] does model the kind, and a caller
//! asking about exhaustiveness has to ask that gate. Recording the gap is the point; inferring past
//! it is what this module refuses to do.

use serde::{Deserialize, Serialize};

use crate::{ClaimGateDecision, CoverageClaimKind, RunnerClaimGate, RunnerFidelityVerdict};

pub const CLAIM_SUPPORT_PARITY_SCHEMA: &str = "assay.runner.claim_support_parity.v0";

const NON_CLAIMS: &[&str] = &[
    "parity_no_observer_class_typing",
    "parity_no_blinding_cost_verdict",
    "parity_no_aggregate_claim_score",
    "parity_no_coverage_descriptor_replacement",
];

/// What the health half decides for one (verdict, kind) pair.
///
/// `NotModelled` is a positive statement, not an error: this gate has no field for that claim kind,
/// and a reader must go to the gate that does rather than take a neighbouring field's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "support", content = "decision")]
pub enum ClaimSupport {
    Decided(ClaimGateDecision),
    NotModelled,
}

/// Whether a row's *cause* has a counterpart in the class half.
///
/// This is about the input that produces the verdict, not about the decision. `CorrelationPartial`
/// and `Clipped` happen to reach the same positive/negative decisions, but `CorrelationPartial` is
/// reached only by `cgroup_correlation == Partial`, which is a runner-internal binding property
/// with no analogue in a rule about observer classes. Marking it keeps a future class-half
/// implementation from inventing a cause to match a decision it recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimSupportScope {
    /// The cause has a class-half analogue, so both halves must agree on this row.
    Shared,
    /// The cause exists only on the health side; the class half is not expected to reproduce it.
    HealthInputOnly,
}

/// One cell of the (verdict x kind) table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimSupportRow {
    pub verdict: RunnerFidelityVerdict,
    pub kind: CoverageClaimKind,
    pub support: ClaimSupport,
    pub scope: ClaimSupportScope,
    pub rule: String,
}

/// The whole contract surface, for callers that want to emit or compare it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimSupportParityTable {
    pub schema: String,
    pub rows: Vec<ClaimSupportRow>,
    pub non_claims: Vec<String>,
}

/// Every verdict this gate can reach. Exhaustively destructured in [`claim_support_table`], so a
/// sixth variant stops the crate compiling rather than silently dropping a row.
#[must_use]
pub fn all_verdicts() -> Vec<RunnerFidelityVerdict> {
    vec![
        RunnerFidelityVerdict::Clean,
        RunnerFidelityVerdict::Clipped,
        RunnerFidelityVerdict::CorrelationPartial,
        RunnerFidelityVerdict::NotApplicable,
        RunnerFidelityVerdict::Failed,
    ]
}

/// Every claim kind the crate's coverage vocabulary carries.
#[must_use]
pub fn all_claim_kinds() -> Vec<CoverageClaimKind> {
    vec![
        CoverageClaimKind::PositiveExistence,
        CoverageClaimKind::ExhaustiveSet,
        CoverageClaimKind::BoundedNegative,
    ]
}

/// The health half's answer for one pair, read off [`RunnerClaimGate`] rather than restated.
#[must_use]
pub fn claim_support(verdict: RunnerFidelityVerdict, kind: CoverageClaimKind) -> ClaimSupportRow {
    let gate = RunnerClaimGate::for_verdict(verdict);

    let (support, rule) = match kind {
        CoverageClaimKind::PositiveExistence => (
            ClaimSupport::Decided(gate.measured_positive_claims),
            "fidelity_verdict_gates_measured_positive_claims",
        ),
        CoverageClaimKind::BoundedNegative => (
            ClaimSupport::Decided(gate.bounded_negative_claims),
            "fidelity_verdict_gates_bounded_negative_claims",
        ),
        // Not folded onto a neighbouring field on purpose; see the module doc.
        CoverageClaimKind::ExhaustiveSet => (
            ClaimSupport::NotModelled,
            "fidelity_verdict_has_no_exhaustive_set_field_ask_coverage_descriptor",
        ),
    };

    let scope = match verdict {
        RunnerFidelityVerdict::CorrelationPartial => ClaimSupportScope::HealthInputOnly,
        RunnerFidelityVerdict::Clean
        | RunnerFidelityVerdict::Clipped
        | RunnerFidelityVerdict::NotApplicable
        | RunnerFidelityVerdict::Failed => ClaimSupportScope::Shared,
    };

    ClaimSupportRow {
        verdict,
        kind,
        support,
        scope,
        rule: rule.to_string(),
    }
}

/// The full table, in a stable order: verdicts outer, kinds inner, both in declaration order.
#[must_use]
pub fn claim_support_table() -> ClaimSupportParityTable {
    let mut rows = Vec::new();
    for verdict in all_verdicts() {
        for kind in all_claim_kinds() {
            rows.push(claim_support(verdict, kind));
        }
    }

    ClaimSupportParityTable {
        schema: CLAIM_SUPPORT_PARITY_SCHEMA.to_string(),
        rows,
        non_claims: NON_CLAIMS
            .iter()
            .map(|non_claim| (*non_claim).to_string())
            .collect(),
    }
}

/// How permissive a decision is, for the one comparison this contract makes.
///
/// Deliberately not an `Ord` impl on [`ClaimGateDecision`] itself: an ordering on the public enum
/// invites arithmetic on it, and the only thing this crate needs to say is that absence is never
/// looser than occurrence.
#[must_use]
pub fn permissiveness(decision: ClaimGateDecision) -> u8 {
    match decision {
        ClaimGateDecision::Allowed => 2,
        ClaimGateDecision::Degraded => 1,
        ClaimGateDecision::Blocked => 0,
    }
}
