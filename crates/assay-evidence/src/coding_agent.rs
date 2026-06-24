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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodingAgentNetworkPolicy {
    Allowed,
    Denied,
}

/// Coverage state for a coding-agent evidence surface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodingAgentCoverageState {
    Observed,
    Unavailable,
    SelfReported,
    Absent,
    Partial,
}

/// Source class for the observed effects in a coding-agent evidence pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

/// Coverage for the core coding-agent surfaces.
///
/// A clean downstream verdict should require observed coverage for files, commands, network, and MCP tools.
/// Test coverage is meaningful when a test command was declared.
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
