//! Coding-agent evidence pack primitives.
//!
//! These types model the Assay-side facts for one coding-agent run: declared scope,
//! observed effects, coverage, source class, and non-claims. They deliberately do not
//! carry a pass/fail verdict. Downstream consumers may compute a bounded verdict from
//! these facts, but the evidence event itself stays an observed-effect record.

use crate::crypto::id::compute_content_hash;
use crate::types::EvidenceEvent;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Event type for the v0 coding-agent evidence pack payload.
pub const CODING_AGENT_EVIDENCE_EVENT_TYPE: &str = "assay.coding_agent.evidence_pack.v0";

/// Default event source for coding-agent evidence emitted by Assay.
pub const CODING_AGENT_EVIDENCE_SOURCE: &str = "urn:assay:coding-agent";

const DEFAULT_NON_CLAIMS: &[&str] = &[
    "does_not_prove_code_correctness",
    "does_not_prove_agent_intent",
    "does_not_replace_human_review",
];

/// Declared network policy for the coding-agent run.
///
/// This is deliberately non-optional: a high-blast-radius surface cannot be omitted from a reviewable
/// coding-agent evidence pack.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodingAgentNetworkPolicy {
    Allowed,
    Denied,
}

/// Coverage state for a coding-agent evidence surface.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodingAgentCoverageState {
    Observed,
    Unavailable,
    SelfReported,
    Absent,
    Partial,
}

/// Source class for the observed effects in a coding-agent evidence pack.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodingAgentSourceClass {
    BoundaryObserved,
    IndependentlyObserved,
    ThirdPartyObserved,
    ProducerReported,
    IssuerAttested,
    ReceiverReceipt,
}

/// Declared authorization and scope for one coding-agent run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodingAgentDeclaredScope {
    pub allowed_files: Vec<String>,
    pub allowed_commands: Vec<String>,
    pub network: CodingAgentNetworkPolicy,
    pub allowed_mcp_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_test_command: Option<String>,
    pub authorized: bool,
}

/// Effects observed for one coding-agent run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodingAgentObservedEffects {
    pub files_changed: Vec<String>,
    pub commands_executed: Vec<String>,
    pub network_attempts: Vec<String>,
    pub mcp_tool_calls: Vec<String>,
    pub test_observed: bool,
}

/// Why a dimension cannot support a conclusion.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodingAgentCoverageGap {
    /// Nothing watched this dimension.
    NotObserved,
    /// An observer was configured but could not report.
    ObserverUnavailable,
    /// The only account of this dimension comes from the subject of the run.
    SelfReportedOnly,
    /// Watched for part of the run, so silence over the rest is not a fact.
    PartialOnly,
}

/// The strongest claim an observation can support, as published in the RGE-Bench ceiling ladder
/// (`rge-bench/rge-bench` README): `asserted` < `asserted_signed` < `observed_at_receiver` <
/// `observed_in_path` < `independently_confirmed`.
///
/// The ladder has five rungs and this crate has six source classes: `third_party_observed` and
/// `independently_observed` both sit at the top rung, because the ladder distinguishes independence
/// from the subject, not the two ways of achieving it. Stated rather than silently invented.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CodingAgentClaimCeiling {
    Asserted,
    AssertedSigned,
    ObservedAtReceiver,
    ObservedInPath,
    IndependentlyConfirmed,
}

/// What kind of claim a consumer wants to make about one dimension.
///
/// Mirrors `assay_runner_schema::CoverageClaimKind` deliberately. The runner substrate has gated
/// claims by kind since 2026-06-01 (`RunnerClaimGate`) and by coverage descriptor since 2026-06-04.
/// The first draft of this module ignored the kind entirely, which made it **contradict** that rule
/// rather than merely duplicate it: a positive claim under partial coverage is `Allowed` there and
/// was `Incomplete` here. Keeping the vocabularies parallel is what lets
/// `tests/claim_gate_parity.rs` assert the two agree.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodingAgentClaimKind {
    /// "this effect happened" — seeing part of a run is enough to say what was seen.
    PositiveExistence,
    /// "these are all of them" — needs coverage of the whole dimension.
    ExhaustiveSet,
    /// "this did not happen" — the claim a blind spot silently destroys.
    BoundedNegative,
}

/// Mirrors `assay_runner_schema::ClaimGateDecision`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodingAgentGateDecision {
    Allowed,
    Degraded,
    Blocked,
}

/// What a consumer may conclude about one dimension, for one kind of claim.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodingAgentClaimDecision {
    pub decision: CodingAgentGateDecision,
    /// The strongest claim the observing position supports. `None` exactly when `Blocked`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ceiling: Option<CodingAgentClaimCeiling>,
    /// What is missing, when something is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<CodingAgentCoverageGap>,
    pub rule: String,
}

/// The strongest claim a source class can support, independent of whether it looked.
///
/// Attestation does not appear here on purpose: signing raises tamper-evidence, never vantage.
pub fn coding_agent_claim_ceiling(source_class: CodingAgentSourceClass) -> CodingAgentClaimCeiling {
    use CodingAgentClaimCeiling as Ceiling;
    match source_class {
        CodingAgentSourceClass::ProducerReported => Ceiling::Asserted,
        CodingAgentSourceClass::IssuerAttested => Ceiling::AssertedSigned,
        CodingAgentSourceClass::ReceiverReceipt => Ceiling::ObservedAtReceiver,
        CodingAgentSourceClass::BoundaryObserved => Ceiling::ObservedInPath,
        CodingAgentSourceClass::ThirdPartyObserved
        | CodingAgentSourceClass::IndependentlyObserved => Ceiling::IndependentlyConfirmed,
    }
}

/// The rule this primitive exists to keep: a source class says *where an observer sat*, coverage says
/// *whether it looked*, and the kind of claim decides how much looking is enough.
///
/// Coverage is evaluated before the source class: an observer that did not watch has nothing to be
/// right or wrong about, however well positioned it is. But *did not watch everything* is not *did
/// not watch* — partial coverage still supports saying what **was** seen, and stops supporting only
/// exhaustiveness and absence. That asymmetry is the point, and it is the runner substrate's rule
/// (`assay_runner_schema`, 2026-06-01/06-04), mirrored here rather than reinvented.
///
/// ```
/// use assay_evidence::{
///     coding_agent_claim_decision, CodingAgentClaimCeiling, CodingAgentClaimKind,
///     CodingAgentCoverageState, CodingAgentGateDecision, CodingAgentSourceClass,
/// };
///
/// // Partial coverage still supports "this happened". Refusing it would be the defect.
/// let seen = coding_agent_claim_decision(
///     CodingAgentSourceClass::BoundaryObserved,
///     CodingAgentCoverageState::Partial,
///     CodingAgentClaimKind::PositiveExistence,
/// );
/// assert_eq!(seen.decision, CodingAgentGateDecision::Allowed);
/// assert_eq!(seen.ceiling, Some(CodingAgentClaimCeiling::ObservedInPath));
///
/// // The same partial coverage cannot support "this did not happen".
/// let absent = coding_agent_claim_decision(
///     CodingAgentSourceClass::BoundaryObserved,
///     CodingAgentCoverageState::Partial,
///     CodingAgentClaimKind::BoundedNegative,
/// );
/// assert_eq!(absent.decision, CodingAgentGateDecision::Blocked);
/// assert_eq!(absent.ceiling, None);
/// ```
pub fn coding_agent_claim_decision(
    source_class: CodingAgentSourceClass,
    coverage: CodingAgentCoverageState,
    claim_kind: CodingAgentClaimKind,
) -> CodingAgentClaimDecision {
    use CodingAgentClaimKind as Kind;
    use CodingAgentCoverageGap as Gap;
    use CodingAgentCoverageState as Cov;
    use CodingAgentGateDecision as Decision;

    let blocked = |gap, rule: &str| CodingAgentClaimDecision {
        decision: Decision::Blocked,
        ceiling: None,
        gap: Some(gap),
        rule: rule.to_string(),
    };

    // Nothing watched: no claim kind survives. Mirrors the runner's missing-descriptor gate.
    match coverage {
        Cov::Absent => return blocked(Gap::NotObserved, "coverage_absent_blocks_claim"),
        Cov::Unavailable => {
            return blocked(
                Gap::ObserverUnavailable,
                "observer_unavailable_blocks_claim",
            )
        }
        // Watched, but by the subject. The account exists; it cannot establish its own completeness.
        Cov::SelfReported => {
            return match claim_kind {
                Kind::PositiveExistence => CodingAgentClaimDecision {
                    decision: Decision::Degraded,
                    // A self-reported account caps at `asserted` however the run is otherwise
                    // classed: the weaker of the two axes binds.
                    ceiling: Some(
                        CodingAgentClaimCeiling::Asserted
                            .min(coding_agent_claim_ceiling(source_class)),
                    ),
                    gap: Some(Gap::SelfReportedOnly),
                    rule: "self_reported_degrades_positive_claim".to_string(),
                },
                Kind::ExhaustiveSet | Kind::BoundedNegative => blocked(
                    Gap::SelfReportedOnly,
                    "self_reported_blocks_completeness_claim",
                ),
            };
        }
        Cov::Partial | Cov::Observed => {}
    }

    let ceiling = coding_agent_claim_ceiling(source_class);

    if coverage == Cov::Partial {
        return match claim_kind {
            Kind::PositiveExistence => CodingAgentClaimDecision {
                decision: Decision::Allowed,
                ceiling: Some(ceiling),
                gap: None,
                rule: "partial_coverage_allows_positive_claim".to_string(),
            },
            Kind::ExhaustiveSet => CodingAgentClaimDecision {
                decision: Decision::Degraded,
                ceiling: Some(ceiling),
                gap: Some(Gap::PartialOnly),
                rule: "partial_coverage_degrades_exhaustive_claim".to_string(),
            },
            Kind::BoundedNegative => {
                blocked(Gap::PartialOnly, "partial_coverage_blocks_absence_claim")
            }
        };
    }

    CodingAgentClaimDecision {
        decision: Decision::Allowed,
        ceiling: Some(ceiling),
        gap: None,
        rule: "observed_coverage_allows_claim".to_string(),
    }
}

/// Per-dimension decisions for one evidence payload, for one kind of claim.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodingAgentCoverageReport {
    pub claim_kind: CodingAgentClaimKind,
    pub files: CodingAgentClaimDecision,
    pub commands: CodingAgentClaimDecision,
    pub network: CodingAgentClaimDecision,
    pub mcp_tools: CodingAgentClaimDecision,
    /// `None` when the run declared no test command: a dimension nobody claimed is not a gap.
    pub test: Option<CodingAgentClaimDecision>,
}

/// The result of folding a set of per-item ceilings into one answer for the whole set.
///
/// Three states rather than an `Option`, because an `Option` conflates two of them. A caller whose
/// set can be empty and a caller whose set never is were both served by `Option` returning `None`,
/// and the first caller then could not tell "nothing here made a claim" from "something here is
/// blocked". That is the occurrence-versus-absence distinction this module exists to keep, so the
/// fold is not allowed to lose it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CodingAgentWeakestCeiling {
    /// Nothing in the set claimed anything, so there is no rung to report and no gap either.
    NothingClaimed,
    /// At least one member is blocked. A block is not the bottom rung; it is the statement that no
    /// rung applies, and there is nothing to take a minimum with.
    Blocked,
    /// The weakest rung across every member, all of which carried one.
    Rung { ceiling: CodingAgentClaimCeiling },
}

/// Fold per-decision ceilings into the strongest claim the whole set supports: the **weakest**
/// rung across its members.
///
/// A fold and never a maximum. One member at the top rung does not raise what the set as a whole
/// supports, and a reader who takes the strongest member away has learnt the wrong thing.
///
/// This is the single implementation of that rule. It used to exist twice — here over coverage
/// dimensions and again in the CLI over side-effect calls — and the copy had already drifted at the
/// moment it was written, because this container is never empty and that one can be.
///
/// Consumes `CodingAgentClaimDecision` references so `Blocked` is encoded by the decision's own
/// verdict and `NothingClaimed` only by an empty iterator. The previous signature accepted
/// `Option<CodingAgentClaimCeiling>`, which conflated blocked (the decision says no rung applies)
/// with absent (the caller passed `None` for its own reasons), and a caller with both states had
/// no way to tell them apart.
///
/// # The verdict decides, not the rung
///
/// `CodingAgentClaimDecision` derives `Deserialize`, so a decision can arrive from outside this
/// crate carrying any combination of its fields. The two inconsistent combinations are handled
/// explicitly rather than left to whichever field happens to be read first:
///
/// * `Blocked` with a rung attached is **blocked**. Reading the rung and ignoring the verdict would
///   let a crafted or corrupted record promote itself, which is the whole failure this fold exists
///   to prevent. The doc above says the verdict encodes the block, so the verdict is what is read.
/// * `Allowed` or `Degraded` with no rung is **blocked**, which is the fail-closed answer. It is
///   deliberately not `NothingClaimed`: a claim was made and cannot be graded, and reporting a
///   non-claim would invent an absence that the input does not state. Only an empty iterator ever
///   yields `NothingClaimed`.
pub fn coding_agent_weakest_ceiling<'a>(
    decisions: impl IntoIterator<Item = &'a CodingAgentClaimDecision>,
) -> CodingAgentWeakestCeiling {
    let mut weakest: Option<CodingAgentClaimCeiling> = None;
    for decision in decisions {
        // The verdict is read before the rung. A block is not the bottom rung; it is the statement
        // that no rung applies, and there is nothing to take a minimum with.
        if decision.decision == CodingAgentGateDecision::Blocked {
            return CodingAgentWeakestCeiling::Blocked;
        }
        // Allowed or Degraded, so this decision owes a rung. Fail closed when it does not have one.
        let Some(ceiling) = decision.ceiling else {
            return CodingAgentWeakestCeiling::Blocked;
        };
        weakest = Some(weakest.map_or(ceiling, |w| w.min(ceiling)));
    }
    match weakest {
        Some(ceiling) => CodingAgentWeakestCeiling::Rung { ceiling },
        None => CodingAgentWeakestCeiling::NothingClaimed,
    }
}

impl CodingAgentCoverageReport {
    /// The strongest claim this report as a whole supports: the **weakest** rung across every
    /// dimension the run claims. `None` when any claimed dimension is `Blocked` — a block binds
    /// harder than any rung, because there is no rung to take a minimum with.
    ///
    /// `NothingClaimed` also maps to `None`, and that arm is unreachable here: `Self::claimed`
    /// always yields the four fixed dimensions. The mapping is stated rather than assumed, because
    /// it is precisely the assumption that made a copy of this fold wrong elsewhere.
    pub fn weakest_ceiling(&self) -> Option<CodingAgentClaimCeiling> {
        let claimed = self.claimed();
        match coding_agent_weakest_ceiling(claimed.iter()) {
            CodingAgentWeakestCeiling::Rung { ceiling } => Some(ceiling),
            CodingAgentWeakestCeiling::Blocked | CodingAgentWeakestCeiling::NothingClaimed => None,
        }
    }

    /// Whether every claimed dimension supports at least `required` without degradation.
    pub fn meets(&self, required: CodingAgentClaimCeiling) -> bool {
        self.claimed()
            .into_iter()
            .all(|d| d.decision == CodingAgentGateDecision::Allowed)
            && self.weakest_ceiling().is_some_and(|c| c >= required)
    }

    /// Every dimension that is not cleanly `Allowed`, with its reason, in declaration order.
    pub fn gaps(&self) -> Vec<(&'static str, CodingAgentClaimDecision)> {
        self.named()
            .into_iter()
            .filter(|(_, d)| d.decision != CodingAgentGateDecision::Allowed)
            .collect()
    }

    fn named(&self) -> Vec<(&'static str, CodingAgentClaimDecision)> {
        [
            ("files", Some(self.files.clone())),
            ("commands", Some(self.commands.clone())),
            ("network", Some(self.network.clone())),
            ("mcp_tools", Some(self.mcp_tools.clone())),
            ("test", self.test.clone()),
        ]
        .into_iter()
        .filter_map(|(name, d)| d.map(|d| (name, d)))
        .collect()
    }

    fn claimed(&self) -> Vec<CodingAgentClaimDecision> {
        self.named().into_iter().map(|(_, d)| d).collect()
    }
}

/// Coverage for the core coding-agent surfaces.
///
/// A clean downstream conclusion requires observed coverage for files, commands, network, and MCP
/// tools; see [`CodingAgentEvidencePayload::coverage_report`], which enforces that rather than
/// leaving it to the reader. Test coverage is meaningful when a test command was declared.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodingAgentCoverage {
    pub files: CodingAgentCoverageState,
    pub commands: CodingAgentCoverageState,
    pub network: CodingAgentCoverageState,
    pub mcp_tools: CodingAgentCoverageState,
    pub test: CodingAgentCoverageState,
}

/// Assay-side evidence payload for one coding-agent run.
///
/// This payload carries facts and explicit non-claims. It does not carry a verdict or sufficiency conclusion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodingAgentEvidencePayload {
    pub declared_scope: CodingAgentDeclaredScope,
    pub observed_effects: CodingAgentObservedEffects,
    pub coverage: CodingAgentCoverage,
    pub source_class: CodingAgentSourceClass,
    pub non_claims: Vec<String>,
}

impl CodingAgentEvidencePayload {
    /// Build a v0 coding-agent evidence payload with the default non-claims.
    pub fn new(
        declared_scope: CodingAgentDeclaredScope,
        observed_effects: CodingAgentObservedEffects,
        coverage: CodingAgentCoverage,
        source_class: CodingAgentSourceClass,
    ) -> Self {
        Self {
            declared_scope,
            observed_effects,
            coverage,
            source_class,
            non_claims: DEFAULT_NON_CLAIMS
                .iter()
                .map(|claim| (*claim).to_string())
                .collect(),
        }
    }

    /// Per-dimension decisions for this payload and one kind of claim.
    ///
    /// `test` is reported only when the run declared an expected test command — a dimension the run
    /// never claimed is out of scope, not a gap. An absent claim and an unmet one are different
    /// facts.
    pub fn coverage_report(&self, claim_kind: CodingAgentClaimKind) -> CodingAgentCoverageReport {
        let decide = |coverage: CodingAgentCoverageState| {
            coding_agent_claim_decision(self.source_class, coverage, claim_kind)
        };
        CodingAgentCoverageReport {
            claim_kind,
            files: decide(self.coverage.files),
            commands: decide(self.coverage.commands),
            network: decide(self.coverage.network),
            mcp_tools: decide(self.coverage.mcp_tools),
            test: self
                .declared_scope
                .expected_test_command
                .is_some()
                .then(|| decide(self.coverage.test)),
        }
    }
}

/// Create a content-addressed EvidenceEvent carrying a coding-agent evidence payload.
///
/// The resulting event contains the typed payload as `data` and computes the hard `content_hash` immediately.
/// It does not compute or carry any verdict; consumers may review the facts separately.
pub fn coding_agent_evidence_event(
    run_id: impl Into<String>,
    seq: u64,
    payload: CodingAgentEvidencePayload,
) -> Result<EvidenceEvent> {
    let mut event = EvidenceEvent::new(
        CODING_AGENT_EVIDENCE_EVENT_TYPE,
        CODING_AGENT_EVIDENCE_SOURCE,
        run_id,
        seq,
        serde_json::to_value(payload)?,
    );
    event.content_hash = Some(compute_content_hash(&event)?);
    Ok(event)
}

#[cfg(test)]
mod weakest_ceiling_tests {
    use super::*;

    /// Helper: build a decision carrying a rung.
    fn allowed(ceiling: CodingAgentClaimCeiling) -> CodingAgentClaimDecision {
        CodingAgentClaimDecision {
            decision: CodingAgentGateDecision::Allowed,
            ceiling: Some(ceiling),
            gap: None,
            rule: "test".to_string(),
        }
    }

    /// Helper: build a blocked decision (no rung).
    fn blocked() -> CodingAgentClaimDecision {
        CodingAgentClaimDecision {
            decision: CodingAgentGateDecision::Blocked,
            ceiling: None,
            gap: Some(CodingAgentCoverageGap::NotObserved),
            rule: "test_blocked".to_string(),
        }
    }

    #[test]
    fn empty_iterator_is_nothing_claimed() {
        let empty: [CodingAgentClaimDecision; 0] = [];
        assert_eq!(
            coding_agent_weakest_ceiling(empty.iter()),
            CodingAgentWeakestCeiling::NothingClaimed,
        );
    }

    #[test]
    fn single_blocked_decision_is_blocked() {
        let decisions = [blocked()];
        assert_eq!(
            coding_agent_weakest_ceiling(decisions.iter()),
            CodingAgentWeakestCeiling::Blocked,
        );
    }

    #[test]
    fn single_rung_is_that_rung() {
        let decisions = [allowed(CodingAgentClaimCeiling::ObservedInPath)];
        assert_eq!(
            coding_agent_weakest_ceiling(decisions.iter()),
            CodingAgentWeakestCeiling::Rung {
                ceiling: CodingAgentClaimCeiling::ObservedInPath
            },
        );
    }

    #[test]
    fn fold_takes_weakest_not_strongest() {
        let decisions = [
            allowed(CodingAgentClaimCeiling::IndependentlyConfirmed),
            allowed(CodingAgentClaimCeiling::Asserted),
        ];
        assert_eq!(
            coding_agent_weakest_ceiling(decisions.iter()),
            CodingAgentWeakestCeiling::Rung {
                ceiling: CodingAgentClaimCeiling::Asserted
            },
        );
    }

    #[test]
    fn blocked_among_rungs_collapses_to_blocked() {
        let decisions = [
            allowed(CodingAgentClaimCeiling::IndependentlyConfirmed),
            blocked(),
            allowed(CodingAgentClaimCeiling::Asserted),
        ];
        assert_eq!(
            coding_agent_weakest_ceiling(decisions.iter()),
            CodingAgentWeakestCeiling::Blocked,
        );
    }

    /// A blocked verdict that still carries a rung must read as blocked.
    ///
    /// This combination cannot be produced by `coding_agent_claim_decision`, but the type derives
    /// `Deserialize`, so it can arrive from outside this crate. A fold that reads the rung and
    /// ignores the verdict lets such a record promote itself past the gate that blocked it. The
    /// rung here is the top one on purpose: if the verdict were ignored, this test would report
    /// `IndependentlyConfirmed`, the strongest possible answer, from the input least entitled to it.
    #[test]
    fn a_blocked_verdict_carrying_a_rung_is_still_blocked() {
        let inconsistent = CodingAgentClaimDecision {
            decision: CodingAgentGateDecision::Blocked,
            ceiling: Some(CodingAgentClaimCeiling::IndependentlyConfirmed),
            gap: Some(CodingAgentCoverageGap::NotObserved),
            rule: "hostile_or_corrupted".to_string(),
        };
        let decisions = [inconsistent];
        assert_eq!(
            coding_agent_weakest_ceiling(decisions.iter()),
            CodingAgentWeakestCeiling::Blocked,
        );
    }

    /// The mirror inconsistency: a claim that was allowed but carries no rung to grade it.
    ///
    /// Fail closed to `Blocked`, and specifically not to `NothingClaimed`. The set is not empty and
    /// a claim was made, so reporting a non-claim would invent an absence the input never stated.
    #[test]
    fn an_allowed_verdict_without_a_rung_fails_closed_rather_than_reading_as_nothing_claimed() {
        for verdict in [
            CodingAgentGateDecision::Allowed,
            CodingAgentGateDecision::Degraded,
        ] {
            let rungless = CodingAgentClaimDecision {
                decision: verdict,
                ceiling: None,
                gap: None,
                rule: "hostile_or_corrupted".to_string(),
            };
            let decisions = [rungless];
            assert_eq!(
                coding_agent_weakest_ceiling(decisions.iter()),
                CodingAgentWeakestCeiling::Blocked,
                "{verdict:?} without a rung must fail closed",
            );
        }
    }

    /// A degraded verdict that does carry a rung still participates as that rung.
    ///
    /// The guard above must not turn "not Allowed" into "blocked": `Degraded` is the gate's way of
    /// saying the claim survives at a reduced ceiling, and folding it away would silently discard
    /// the case the ladder is most useful for.
    #[test]
    fn a_degraded_verdict_with_a_rung_still_folds_as_that_rung() {
        let degraded = CodingAgentClaimDecision {
            decision: CodingAgentGateDecision::Degraded,
            ceiling: Some(CodingAgentClaimCeiling::Asserted),
            gap: Some(CodingAgentCoverageGap::SelfReportedOnly),
            rule: "self_reported_degrades_positive_claim".to_string(),
        };
        let decisions = [degraded, allowed(CodingAgentClaimCeiling::ObservedInPath)];
        assert_eq!(
            coding_agent_weakest_ceiling(decisions.iter()),
            CodingAgentWeakestCeiling::Rung {
                ceiling: CodingAgentClaimCeiling::Asserted
            },
        );
    }
}
