pub mod attestation;
pub mod bundle;
pub mod coding_agent;
pub mod crypto;
pub mod denial_marker;
pub mod diff;
pub mod g3_authorization_context;
pub mod json_strict;
pub mod lint;
pub mod mandate;
pub mod ndjson;
pub mod sanitize;
pub mod store;
pub mod trust_basis;
pub mod trust_card;
pub mod types;

// Convenience re-exports
pub use bundle::{
    verify_bundle, verify_bundle_with_limits, AlgorithmMeta, BundleInfo, BundleReader,
    BundleWriter, ErrorClass, ErrorCode, FileMeta, Manifest, VerifyError, VerifyLimits,
    VerifyLimitsOverrides, VerifyResult,
};
pub use coding_agent::{
    coding_agent_claim_ceiling, coding_agent_claim_decision, coding_agent_evidence_event,
    coding_agent_weakest_ceiling, session_finding_coverage_state, CodingAgentClaimCeiling,
    CodingAgentClaimDecision, CodingAgentClaimKind, CodingAgentCoverage, CodingAgentCoverageGap,
    CodingAgentCoverageReport, CodingAgentCoverageState, CodingAgentDeclaredScope,
    CodingAgentEvidencePayload, CodingAgentGateDecision, CodingAgentNetworkPolicy,
    CodingAgentObservedEffects, CodingAgentSourceClass, CodingAgentWeakestCeiling,
    CODING_AGENT_EVIDENCE_EVENT_TYPE, CODING_AGENT_EVIDENCE_SOURCE,
};
pub use denial_marker::{
    bindable_denial_marker, classify_denial_marker, BindableDenialMarker, DenialMarkerVersion,
    DENIED_CALL_OBSERVATION_V0, DENIED_CALL_OBSERVATION_V1, PROXY_DENIED_V0, PROXY_DENIED_V1,
    PROXY_ORIGIN,
};
pub use lint::packs::{load_pack, load_packs, LoadedPack, PackError, PackSource};
pub use ndjson::{read_events, write_events, NdjsonEvents};
pub use store::config::{resolve_store_url, StoreConfig};
pub use store::{
    BundleMeta, BundleStore, ObjectStoreBundleStore, StoreError, StoreSpec, StoreStatus,
};
pub use trust_basis::{
    diff_trust_basis, duplicate_trust_basis_claim_ids, generate_trust_basis,
    to_canonical_json_bytes, TrustBasis, TrustBasisClaim, TrustBasisClaimLevelDiff,
    TrustBasisClaimMetadataDiff, TrustBasisClaimPresenceDiff, TrustBasisDiffClass,
    TrustBasisDiffReport, TrustBasisDiffSummary, TrustBasisOptions, TrustClaimBoundary,
    TrustClaimId, TrustClaimLevel, TrustClaimSource, TRUST_BASIS_DIFF_SCHEMA,
};
pub use trust_card::{
    trust_basis_to_trust_card, trust_card_to_canonical_json_bytes, trust_card_to_html,
    trust_card_to_markdown, TrustCard, TRUST_CARD_NON_GOALS, TRUST_CARD_NOTE_EMPTY_PLACEHOLDER,
    TRUST_CARD_SCHEMA_VERSION,
};
pub use types::{
    Envelope, EvidenceEvent, ProducerMeta, ASSAY_EVIDENCE_SPEC_VERSION, CE_SPECVERSION,
    SPEC_VERSION,
};

// Re-export bytes for CLI convenience
pub use bytes::Bytes;
