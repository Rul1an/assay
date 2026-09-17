//! Incident Package v1 container reader and verification report vocabulary.
//!
//! Implements the outer container format and report serialization defined in
//! `docs/architecture/SPEC-Incident-Package-v1.md`.

pub mod container;

pub use container::{read_incident_container, ContainerMember, IncidentContainer};

use serde::{Deserialize, Serialize};

/// Canonical schema identifier for incident package verification results.
pub const INCIDENT_VERIFY_SCHEMA_V1: &str = "assay.incident.verify.v1";

/// Fixed non-claims emitted with every incident verification report.
pub const NON_CLAIMS_DEFAULT: [&str; 3] = [
    "no_activity_completeness",
    "no_provider_outcome",
    "no_automatic_trust",
];

/// Outcome of package verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentOutcome {
    /// The incident package is verified and all checks passed.
    PackageVerified,
    /// The incident package was refused due to an admission, schema, or semantic fault.
    PackageRefused,
    /// Verification could not complete due to unavailable required input or I/O failure.
    VerificationUnavailable,
}

/// Closed reason set across all phases defined in SPEC-Incident-Package-v1 section 10.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentReason {
    /// Phase 1: Context input shape or validity interval invalid.
    TrustInput,
    /// Phase 2 / 5 / 6: Outer framing, member grammar, or format framing invalid.
    InputShape,
    /// Phase 3: Format or emitted event type unrecognized.
    UnknownFormat,
    /// Phase 3: Schema or version identity unrecognized.
    UnknownSchema,
    /// Phase 3: Declared surface does not equal admitted source mapping.
    Mapping,
    /// Phase 4: Object digest/length mismatch or assessment digest mismatch.
    Digest,
    /// Phase 4: Retained policy reference does not bind original policy bytes.
    PolicyBinding,
    /// Phase 5: Required non-policy input or reference target not present.
    Locator,
    /// Phase 6: Canonical attestation verification returned an error.
    AttestationVerification,
    /// Phase 7: Disclosed result mismatch, withholding error, or unit enumeration fault.
    Disclosure,
    /// Phase 8: CAP-1 JSON Schema 2020-12 validation rejected.
    Cap1Shape,
    /// Phase 8: CAP-1 normative rules R0–R8 rejected.
    Cap1Rules,
    /// Phase 9: Claim gate or proposition contradiction violation.
    ClaimBoundary,
    /// Phase 10: External expectation mismatch or required disclosure unsatisfied.
    ExpectationMismatch,
    /// Phase 11: Recomputed assessment fields differ from recorded assessment.
    StaleAssessment,
    /// Phase 12: Atomic output publishing failure or existing destination conflict.
    AtomicOutput,
    /// Guard: Resource hard maximum exceeded.
    ResourceHard,
    /// Guard: Resource caller-selected limit exceeded.
    ResourceSelected,
    /// Guard: Required bytes/key material unavailable or I/O interrupted.
    IoUnavailable,
}

/// Status of expectation comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentExpectation {
    /// Expectation evaluation was not reached due to prior refusal or failure.
    NotEvaluated,
    /// No expectations were supplied by the caller.
    NotRequested,
    /// All supplied expectations matched.
    Matched,
    /// One or more supplied expectations mismatched.
    Mismatched,
}

/// Verification or refusal report for an incident package.
///
/// Serializes to RFC 8785 JCS canonical JSON with a single trailing newline (`LF`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentVerifyReport {
    /// Schema identity (`assay.incident.verify.v1`).
    pub schema: String,
    /// High-level verification outcome.
    pub outcome: IncidentOutcome,
    /// Specific refusal or error reason, if refused or unavailable.
    pub reason: Option<IncidentReason>,
    /// SHA-256 digest of outer raw package bytes, if established.
    pub artifact_sha256: Option<String>,
    /// SHA-256 digest of inventory.json raw bytes, if established.
    pub inventory_sha256: Option<String>,
    /// SHA-256 digest of assessment.json raw bytes, if established.
    pub assessment_sha256: Option<String>,
    /// Status of external expectation evaluation.
    pub expectation: IncidentExpectation,
    /// Attestation checks executed or evaluated.
    pub attestations: Vec<serde_json::Value>,
    /// Embedded verification context, if established on success.
    pub verification_context: Option<serde_json::Value>,
    /// SHA-256 digest of canonical verification context bytes, if established.
    pub verification_context_sha256: Option<String>,
    /// List of resolved disclosed result references.
    pub resolved_results: Vec<serde_json::Value>,
    /// Verification counts, if established on success.
    pub counts: Option<serde_json::Value>,
    /// Fixed three non-claims strings in canonical order.
    pub non_claims: Vec<String>,
}

impl IncidentVerifyReport {
    /// Construct a terminal refusal report for the given reason.
    pub fn refusal(reason: IncidentReason) -> Self {
        Self {
            schema: INCIDENT_VERIFY_SCHEMA_V1.to_string(),
            outcome: IncidentOutcome::PackageRefused,
            reason: Some(reason),
            artifact_sha256: None,
            inventory_sha256: None,
            assessment_sha256: None,
            expectation: IncidentExpectation::NotEvaluated,
            attestations: Vec::new(),
            verification_context: None,
            verification_context_sha256: None,
            resolved_results: Vec::new(),
            counts: None,
            non_claims: NON_CLAIMS_DEFAULT.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Serialize this report to RFC 8785 JCS canonical JSON bytes followed by one LF.
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, assay_canonical::Error> {
        let mut bytes = assay_canonical::jcs::to_vec(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Serialize this report to an RFC 8785 JCS canonical JSON string ending with LF.
    pub fn to_canonical_string(&self) -> Result<String, assay_canonical::Error> {
        let bytes = self.to_canonical_bytes()?;
        String::from_utf8(bytes).map_err(|e| assay_canonical::Error::Canonicalize(e.to_string()))
    }
}
