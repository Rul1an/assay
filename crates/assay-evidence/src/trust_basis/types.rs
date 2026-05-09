use crate::lint::engine::LintOptions;
use serde::{Deserialize, Serialize};

pub const TRUST_BASIS_DIFF_SCHEMA: &str = "assay.trust-basis.diff.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TrustClaimId {
    BundleVerified,
    SigningEvidencePresent,
    ProvenanceBackedClaimsPresent,
    DelegationContextVisible,
    /// G3 v1: policy-projected `principal` + `auth_scheme` + `auth_issuer` on decision evidence
    AuthorizationContextVisible,
    ContainmentDegradationObserved,
    ExternalEvalReceiptBoundaryVisible,
    ExternalDecisionReceiptBoundaryVisible,
    ExternalInventoryReceiptBoundaryVisible,
    AppliedPackFindingsPresent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustClaimLevel {
    Verified,
    SelfReported,
    Inferred,
    Absent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustClaimSource {
    BundleVerification,
    BundleProofSurface,
    CanonicalDecisionEvidence,
    CanonicalEventPresence,
    ExternalEvidenceReceipt,
    ExternalDecisionReceipt,
    ExternalInventoryReceipt,
    PackExecutionResults,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TrustClaimBoundary {
    BundleWide,
    SupportedDelegatedFlowsOnly,
    /// G3 v1: auth context fields from supported policy-projected MCP decision path only
    SupportedAuthProjectedFlowsOnly,
    SupportedContainmentFallbackPathsOnly,
    SupportedExternalEvalReceiptEventsOnly,
    SupportedExternalDecisionReceiptEventsOnly,
    SupportedExternalInventoryReceiptEventsOnly,
    ProofSurfacesOnly,
    PackExecutionOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBasisClaim {
    pub id: TrustClaimId,
    pub level: TrustClaimLevel,
    pub source: TrustClaimSource,
    pub boundary: TrustClaimBoundary,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBasis {
    pub claims: Vec<TrustBasisClaim>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustBasisDiffClass {
    Regressed,
    Improved,
    Removed,
    Added,
    MetadataChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBasisClaimLevelDiff {
    pub diff_class: TrustBasisDiffClass,
    pub claim_id: TrustClaimId,
    pub baseline_level: TrustClaimLevel,
    pub candidate_level: TrustClaimLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBasisClaimMetadataDiff {
    pub diff_class: TrustBasisDiffClass,
    pub claim_id: TrustClaimId,
    pub baseline_level: TrustClaimLevel,
    pub candidate_level: TrustClaimLevel,
    pub baseline_source: TrustClaimSource,
    pub candidate_source: TrustClaimSource,
    pub baseline_boundary: TrustClaimBoundary,
    pub candidate_boundary: TrustClaimBoundary,
    pub note_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBasisClaimPresenceDiff {
    pub diff_class: TrustBasisDiffClass,
    pub claim_id: TrustClaimId,
    pub baseline_level: Option<TrustClaimLevel>,
    pub candidate_level: Option<TrustClaimLevel>,
    pub baseline_source: Option<TrustClaimSource>,
    pub candidate_source: Option<TrustClaimSource>,
    pub baseline_boundary: Option<TrustClaimBoundary>,
    pub candidate_boundary: Option<TrustClaimBoundary>,
    pub baseline_note: Option<String>,
    pub candidate_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBasisDiffSummary {
    pub regressed_claims: usize,
    pub improved_claims: usize,
    pub removed_claims: usize,
    pub added_claims: usize,
    pub metadata_changes: usize,
    pub unchanged_claim_count: usize,
    pub has_regressions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBasisDiffReport {
    pub schema: String,
    pub claim_identity: String,
    pub level_order: Vec<TrustClaimLevel>,
    pub summary: TrustBasisDiffSummary,
    pub regressed_claims: Vec<TrustBasisClaimLevelDiff>,
    pub improved_claims: Vec<TrustBasisClaimLevelDiff>,
    pub removed_claims: Vec<TrustBasisClaimPresenceDiff>,
    pub added_claims: Vec<TrustBasisClaimPresenceDiff>,
    pub metadata_changes: Vec<TrustBasisClaimMetadataDiff>,
    pub unchanged_claim_count: usize,
}

impl TrustBasisDiffReport {
    pub fn has_changes(&self) -> bool {
        !self.regressed_claims.is_empty()
            || !self.improved_claims.is_empty()
            || !self.removed_claims.is_empty()
            || !self.added_claims.is_empty()
            || !self.metadata_changes.is_empty()
    }

    pub fn has_regressions(&self) -> bool {
        !self.regressed_claims.is_empty() || !self.removed_claims.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TrustBasisOptions {
    pub lint: Option<LintOptions>,
}
