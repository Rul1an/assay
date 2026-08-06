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

/// What a consumer may conclude about one dimension.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "conclusion")]
pub enum CodingAgentDimensionConclusion {
    /// Something watched this dimension. `ceiling` is the strongest claim the observing position can
    /// support — never more, however the record is signed or anchored.
    Supported { ceiling: CodingAgentClaimCeiling },
    /// Nobody watched this dimension. Never a clean pass, whatever the source class.
    Incomplete { gap: CodingAgentCoverageGap },
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
/// *whether it looked*, and neither alone licenses a conclusion.
///
/// A boundary observer configured not to watch the network is well-positioned and blind; a
/// producer-reported account of the network is present and interested. Both yield less than a clean
/// pass, for different reasons, and collapsing them loses the reason.
///
/// ```
/// use assay_evidence::{
///     coding_agent_dimension_conclusion, CodingAgentClaimCeiling, CodingAgentCoverageGap,
///     CodingAgentCoverageState, CodingAgentDimensionConclusion, CodingAgentSourceClass,
/// };
///
/// // Top-tier vantage, but it was not watching this dimension.
/// assert_eq!(
///     coding_agent_dimension_conclusion(
///         CodingAgentSourceClass::IndependentlyObserved,
///         CodingAgentCoverageState::Absent,
///     ),
///     CodingAgentDimensionConclusion::Incomplete { gap: CodingAgentCoverageGap::NotObserved },
/// );
///
/// // A receiver watched, and a receiver's ceiling is what a receiver can see.
/// assert_eq!(
///     coding_agent_dimension_conclusion(
///         CodingAgentSourceClass::ReceiverReceipt,
///         CodingAgentCoverageState::Observed,
///     ),
///     CodingAgentDimensionConclusion::Supported {
///         ceiling: CodingAgentClaimCeiling::ObservedAtReceiver
///     },
/// );
/// ```
pub fn coding_agent_dimension_conclusion(
    source_class: CodingAgentSourceClass,
    coverage: CodingAgentCoverageState,
) -> CodingAgentDimensionConclusion {
    use CodingAgentCoverageGap as Gap;
    use CodingAgentCoverageState as Cov;
    use CodingAgentDimensionConclusion as Conclusion;

    // Coverage first, on its own. An observer that did not watch has nothing to be right or wrong
    // about, however well positioned it is.
    let gap = match coverage {
        Cov::Observed => None,
        Cov::Absent => Some(Gap::NotObserved),
        Cov::Unavailable => Some(Gap::ObserverUnavailable),
        Cov::SelfReported => Some(Gap::SelfReportedOnly),
        Cov::Partial => Some(Gap::PartialOnly),
    };
    match gap {
        Some(gap) => Conclusion::Incomplete { gap },
        None => Conclusion::Supported {
            ceiling: coding_agent_claim_ceiling(source_class),
        },
    }
}

/// Per-dimension conclusions for one evidence payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodingAgentCoverageReport {
    pub files: CodingAgentDimensionConclusion,
    pub commands: CodingAgentDimensionConclusion,
    pub network: CodingAgentDimensionConclusion,
    pub mcp_tools: CodingAgentDimensionConclusion,
    /// `None` when the run declared no test command: a dimension nobody claimed is not a gap.
    pub test: Option<CodingAgentDimensionConclusion>,
}

impl CodingAgentCoverageReport {
    /// The strongest claim this report as a whole can support: the **weakest** rung across every
    /// dimension the run claims. `None` when any claimed dimension is `Incomplete` — a gap binds
    /// harder than any rung, because there is no rung to take a minimum with.
    pub fn weakest_ceiling(&self) -> Option<CodingAgentClaimCeiling> {
        let mut weakest: Option<CodingAgentClaimCeiling> = None;
        for conclusion in self.claimed() {
            match conclusion {
                CodingAgentDimensionConclusion::Incomplete { .. } => return None,
                CodingAgentDimensionConclusion::Supported { ceiling } => {
                    weakest = Some(weakest.map_or(ceiling, |w| w.min(ceiling)));
                }
            }
        }
        weakest
    }

    /// Whether every claimed dimension supports at least `required`.
    pub fn meets(&self, required: CodingAgentClaimCeiling) -> bool {
        self.weakest_ceiling().is_some_and(|c| c >= required)
    }

    /// Every dimension that cannot support a conclusion at all, with the reason, in declaration order.
    pub fn gaps(&self) -> Vec<(&'static str, CodingAgentCoverageGap)> {
        self.named()
            .into_iter()
            .filter_map(|(name, c)| match c {
                CodingAgentDimensionConclusion::Incomplete { gap } => Some((name, gap)),
                CodingAgentDimensionConclusion::Supported { .. } => None,
            })
            .collect()
    }

    fn named(&self) -> Vec<(&'static str, CodingAgentDimensionConclusion)> {
        [
            ("files", Some(self.files)),
            ("commands", Some(self.commands)),
            ("network", Some(self.network)),
            ("mcp_tools", Some(self.mcp_tools)),
            ("test", self.test),
        ]
        .into_iter()
        .filter_map(|(name, c)| c.map(|c| (name, c)))
        .collect()
    }

    fn claimed(&self) -> Vec<CodingAgentDimensionConclusion> {
        self.named().into_iter().map(|(_, c)| c).collect()
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

    /// Per-dimension conclusions for this payload, combining the source class with what was
    /// actually watched.
    ///
    /// `test` is reported only when the run declared an expected test command — a dimension the run
    /// never claimed is out of scope, not a gap. That distinction is the whole point: an absent
    /// claim and an unmet one are different facts.
    pub fn coverage_report(&self) -> CodingAgentCoverageReport {
        let conclude = |coverage: CodingAgentCoverageState| {
            coding_agent_dimension_conclusion(self.source_class, coverage)
        };
        CodingAgentCoverageReport {
            files: conclude(self.coverage.files),
            commands: conclude(self.coverage.commands),
            network: conclude(self.coverage.network),
            mcp_tools: conclude(self.coverage.mcp_tools),
            test: self
                .declared_scope
                .expected_test_command
                .is_some()
                .then(|| conclude(self.coverage.test)),
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
