use std::cmp::Ordering;
use std::fmt;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

pub const PROFILE: &str = "gateway-path.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Complete,
    Partial,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceClass {
    ProducerReported,
    IssuerAttested,
    ReceiverReceipt,
    BoundaryObserved,
    ThirdPartyObserved,
    // Deliberate exception to the closed schema: unknown provenance is a
    // typed fail-closed reason, not a generic malformed shape.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum Ceiling {
    Asserted,
    AssertedSigned,
    ObservedAtReceiver,
    ObservedInPath,
    IndependentlyConfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    PathVerified,
    PathMismatch,
    Incomplete,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    RouteSubstitution,
    RouteNotAllowed,
    FallbackMismatch,
    FallbackNotAllowed,
    EndpointMismatch,
    PolicyHashMismatch,
    StreamCommitmentMismatch,
    AttestationStale,
    AttestationFreshnessMissing,
    StreamEvidenceMissing,
    EvidenceNotVerified,
    CoverageNotComplete,
    MalformedInput,
    UnknownSourceClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum NonClaim {
    NotGatewayEnforcement,
    NotProviderHonesty,
    NotResponseTruth,
    NotTeeRootVerification,
    NotSafetyOrCompliance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayResult {
    pub profile: &'static str,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ceiling: Option<Ceiling>,
    pub reasons: Vec<Reason>,
    pub non_claims: Vec<NonClaim>,
}

impl ReplayResult {
    pub fn new(status: Status, ceiling: Option<Ceiling>, mut reasons: Vec<Reason>) -> Self {
        reasons.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
        reasons.dedup();
        Self {
            profile: PROFILE,
            status,
            ceiling,
            reasons,
            non_claims: NonClaim::all().to_vec(),
        }
    }

    pub fn invalid() -> Self {
        Self::new(Status::Invalid, None, vec![Reason::MalformedInput])
    }
}

impl Reason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Reason::RouteSubstitution => "route_substitution",
            Reason::RouteNotAllowed => "route_not_allowed",
            Reason::FallbackMismatch => "fallback_mismatch",
            Reason::FallbackNotAllowed => "fallback_not_allowed",
            Reason::EndpointMismatch => "endpoint_mismatch",
            Reason::PolicyHashMismatch => "policy_hash_mismatch",
            Reason::StreamCommitmentMismatch => "stream_commitment_mismatch",
            Reason::AttestationStale => "attestation_stale",
            Reason::AttestationFreshnessMissing => "attestation_freshness_missing",
            Reason::StreamEvidenceMissing => "stream_evidence_missing",
            Reason::EvidenceNotVerified => "evidence_not_verified",
            Reason::CoverageNotComplete => "coverage_not_complete",
            Reason::MalformedInput => "malformed_input",
            Reason::UnknownSourceClass => "unknown_source_class",
        }
    }
}

impl NonClaim {
    pub const fn all() -> &'static [NonClaim] {
        &[
            NonClaim::NotGatewayEnforcement,
            NonClaim::NotProviderHonesty,
            NonClaim::NotResponseTruth,
            NonClaim::NotTeeRootVerification,
            NonClaim::NotSafetyOrCompliance,
        ]
    }
}

impl SourceClass {
    pub const fn ceiling(self) -> Option<Ceiling> {
        match self {
            SourceClass::ProducerReported => Some(Ceiling::Asserted),
            SourceClass::IssuerAttested => Some(Ceiling::AssertedSigned),
            SourceClass::ReceiverReceipt => Some(Ceiling::ObservedAtReceiver),
            SourceClass::BoundaryObserved => Some(Ceiling::ObservedInPath),
            SourceClass::ThirdPartyObserved => Some(Ceiling::IndependentlyConfirmed),
            SourceClass::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceBundle {
    pub profile: String,
    pub request_id: String,
    pub coverage: Coverage,
    pub source_class: SourceClass,
    pub now: String,
    pub claim: Claim,
    pub policy: Policy,
    pub evidence: Evidence,
}

impl EvidenceBundle {
    pub fn validate_shape(&self) -> Result<(), ShapeError> {
        if self.profile != PROFILE {
            return Err(ShapeError);
        }
        require_nonempty(&self.request_id)?;
        parse_utc(&self.now)?;
        self.claim.validate_shape()?;
        self.policy.validate_shape()?;
        self.evidence.validate_shape()?;
        Ok(())
    }

    pub fn now_utc(&self) -> Result<DateTime<Utc>, ShapeError> {
        parse_utc(&self.now)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub requested_route: String,
    pub expected_fallback: Option<String>,
    pub expected_endpoint: String,
    pub expected_stream_commitment: Option<String>,
}

impl Claim {
    fn validate_shape(&self) -> Result<(), ShapeError> {
        require_nonempty(&self.requested_route)?;
        require_optional_nonempty(self.expected_fallback.as_deref())?;
        require_nonempty(&self.expected_endpoint)?;
        require_optional_nonempty(self.expected_stream_commitment.as_deref())?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub allowed_routes: Vec<String>,
    pub allowed_fallbacks: Vec<String>,
    pub endpoint: String,
    pub policy_hash: String,
}

impl Policy {
    fn validate_shape(&self) -> Result<(), ShapeError> {
        require_nonempty_list(&self.allowed_routes)?;
        require_nonempty_list_allow_empty(&self.allowed_fallbacks)?;
        require_nonempty(&self.endpoint)?;
        require_nonempty(&self.policy_hash)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub signature_verified: bool,
    pub runtime_measurement_verified: bool,
    pub attestation_valid_until: Option<String>,
    pub selected_route: String,
    pub fallback_route: Option<String>,
    pub endpoint: Option<String>,
    pub policy_hash: Option<String>,
    pub stream_commitment: Option<String>,
}

impl Evidence {
    fn validate_shape(&self) -> Result<(), ShapeError> {
        require_optional_nonempty(self.attestation_valid_until.as_deref())?;
        if let Some(value) = self.attestation_valid_until.as_deref() {
            parse_utc(value)?;
        }
        require_nonempty(&self.selected_route)?;
        require_optional_nonempty(self.fallback_route.as_deref())?;
        require_optional_nonempty(self.endpoint.as_deref())?;
        require_optional_nonempty(self.policy_hash.as_deref())?;
        require_optional_nonempty(self.stream_commitment.as_deref())?;
        Ok(())
    }

    pub fn attestation_valid_until_utc(&self) -> Option<DateTime<Utc>> {
        self.attestation_valid_until
            .as_deref()
            .and_then(|value| parse_utc(value).ok())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeError;

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("malformed gateway-path evidence")
    }
}

impl std::error::Error for ShapeError {}

pub fn parse_utc(value: &str) -> Result<DateTime<Utc>, ShapeError> {
    if value.trim().is_empty() || !value.ends_with('Z') {
        return Err(ShapeError);
    }
    let stripped = value.strip_suffix('Z').ok_or(ShapeError)?;
    let naive =
        NaiveDateTime::parse_from_str(stripped, "%Y-%m-%dT%H:%M:%S").map_err(|_| ShapeError)?;
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

fn require_nonempty(value: &str) -> Result<(), ShapeError> {
    if value.trim().is_empty() {
        Err(ShapeError)
    } else {
        Ok(())
    }
}

fn require_optional_nonempty(value: Option<&str>) -> Result<(), ShapeError> {
    if let Some(value) = value {
        require_nonempty(value)?;
    }
    Ok(())
}

fn require_nonempty_list(values: &[String]) -> Result<(), ShapeError> {
    if values.is_empty() {
        return Err(ShapeError);
    }
    require_nonempty_list_allow_empty(values)
}

fn require_nonempty_list_allow_empty(values: &[String]) -> Result<(), ShapeError> {
    if values.iter().all(|value| !value.trim().is_empty()) {
        Ok(())
    } else {
        Err(ShapeError)
    }
}

impl Ord for SourceClass {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ceiling().cmp(&other.ceiling())
    }
}

impl PartialOrd for SourceClass {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
