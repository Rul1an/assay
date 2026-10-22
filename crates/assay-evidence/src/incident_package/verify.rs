//! Incident Package v1 verification engine.
//!
//! Enforces the 11 ordered phases of SPEC-Incident-Package-v1 section 10,
//! composing the container reader and the normative CAP-1 verifier.

#[path = "export.rs"]
mod export;
pub use export::{build_verified_incident_package, prepare_incident_export, IncidentExportObject};

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Seek};

use assay_common::limits::{LimitExceeded, LimitKind, LimitReader};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    container::read_incident_container_with_limits, IncidentExpectation, IncidentOutcome,
    IncidentReason, IncidentVerifyReport, INCIDENT_VERIFY_SCHEMA_V1, NON_CLAIMS_DEFAULT,
};
use crate::attestation::{verify_attestation_for_bundle_with_extent_and_limits, DsseEnvelope};
use crate::bundle::writer::{
    BundleJsonKind, MemberLimitBudget, MemberLimitCause, MemberLimitEvidence,
};
use crate::bundle::{BundleReader, ErrorClass, ErrorCode, VerifyError, VerifyLimits};
use crate::coding_agent::{CodingAgentCoverageState, CodingAgentEvidencePayload};
use crate::coverage_attestation::{
    verify_cap1_document, Cap1AdmissionLimits, Cap1BasisKind, Cap1Disposition, Cap1Document,
    Cap1Stage, CAP1_SCHEMA_SHA256,
};
use crate::json_strict::{
    check_next, validate_json_strict, validate_json_strict_bounded,
    validate_json_strict_with_depth, BoundedJsonError, BoundedJsonHooks, BoundedJsonLimits,
    BudgetDimension, BudgetHit, BudgetLimit, BudgetTier, PathSegment, StrictJsonError, StringClass,
};
use crate::types::EvidenceEvent;
use assay_runner_schema::{
    is_known_kernel_event_key, CaptureOrigin, CgroupCorrelationStatus, CoverageCompleteness,
    CoverageDescriptor, EffectDimension, KernelEventValidationError, KernelLayerEvent,
    KernelLayerStatus, ObservationHealth, COVERAGE_DESCRIPTOR_SCHEMA, KERNEL_EVENT_SCHEMA,
    KERNEL_EVENT_SCHEMA_V1, OBSERVATION_HEALTH_SCHEMA, REQUIRED_KERNEL_EVENT_KEYS,
};
use ed25519_dalek::pkcs8::DecodePublicKey;
use ed25519_dalek::VerifyingKey;

/// Exact 28 numeric resource limit keys defined in SPEC-Incident-Package-v1 §9.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextLimits {
    pub outer_bytes: u64,
    pub members: u64,
    pub outer_path_bytes: u64,
    pub object_bytes_total: u64,
    pub inputs: u64,
    pub inner_compressed_bytes: u64,
    pub decoded_bytes_total: u64,
    pub nested_archive_depth: u64,
    pub inventory_bytes: u64,
    pub assessment_bytes: u64,
    pub policy_bytes: u64,
    pub transcript_bytes: u64,
    pub health_bytes: u64,
    pub envelope_bytes: u64,
    pub key_bytes: u64,
    pub retained_report_bytes: u64,
    pub inner_manifest_bytes: u64,
    pub inner_events_bytes: u64,
    pub records: u64,
    pub record_bytes: u64,
    pub inner_path_bytes: u64,
    pub json_depth: u64,
    pub json_key_bytes: u64,
    pub non_payload_string_bytes: u64,
    pub payload_string_bytes: u64,
    pub references: u64,
    pub reference_depth: u64,
    pub diagnostic_bytes: u64,
    pub fresh_result_bytes: u64,
}

impl ContextLimits {
    /// Immutable hard maxima defined in SPEC-Incident-Package-v1 §9 table.
    pub const HARD: Self = Self {
        outer_bytes: 268_435_456,
        members: 128,
        outer_path_bytes: 80,
        object_bytes_total: 251_658_240,
        inputs: 64,
        inner_compressed_bytes: 104_857_600,
        decoded_bytes_total: 536_870_912,
        nested_archive_depth: 1,
        inventory_bytes: 1_048_576,
        assessment_bytes: 16_777_216,
        policy_bytes: 1_048_576,
        transcript_bytes: 16_777_216,
        health_bytes: 1_048_576,
        envelope_bytes: 33_554_432,
        key_bytes: 16_384,
        retained_report_bytes: 16_777_216,
        inner_manifest_bytes: 10_485_760,
        inner_events_bytes: 524_288_000,
        records: 100_000,
        record_bytes: 1_048_576,
        inner_path_bytes: 256,
        json_depth: 64,
        json_key_bytes: 256,
        non_payload_string_bytes: 65_536,
        payload_string_bytes: 1_048_576,
        references: 100_000,
        reference_depth: 1,
        diagnostic_bytes: 4096,
        fresh_result_bytes: 16_777_216,
    };

    /// Validate that all limits are positive and <= hard maxima.
    pub fn is_valid(&self) -> bool {
        self.outer_bytes > 0
            && self.outer_bytes <= Self::HARD.outer_bytes
            && self.members > 0
            && self.members <= Self::HARD.members
            && self.outer_path_bytes > 0
            && self.outer_path_bytes <= Self::HARD.outer_path_bytes
            && self.object_bytes_total > 0
            && self.object_bytes_total <= Self::HARD.object_bytes_total
            && self.inputs > 0
            && self.inputs <= Self::HARD.inputs
            && self.inner_compressed_bytes > 0
            && self.inner_compressed_bytes <= Self::HARD.inner_compressed_bytes
            && self.decoded_bytes_total > 0
            && self.decoded_bytes_total <= Self::HARD.decoded_bytes_total
            && self.nested_archive_depth > 0
            && self.nested_archive_depth <= Self::HARD.nested_archive_depth
            && self.inventory_bytes > 0
            && self.inventory_bytes <= Self::HARD.inventory_bytes
            && self.assessment_bytes > 0
            && self.assessment_bytes <= Self::HARD.assessment_bytes
            && self.policy_bytes > 0
            && self.policy_bytes <= Self::HARD.policy_bytes
            && self.transcript_bytes > 0
            && self.transcript_bytes <= Self::HARD.transcript_bytes
            && self.health_bytes > 0
            && self.health_bytes <= Self::HARD.health_bytes
            && self.envelope_bytes > 0
            && self.envelope_bytes <= Self::HARD.envelope_bytes
            && self.key_bytes > 0
            && self.key_bytes <= Self::HARD.key_bytes
            && self.retained_report_bytes > 0
            && self.retained_report_bytes <= Self::HARD.retained_report_bytes
            && self.inner_manifest_bytes > 0
            && self.inner_manifest_bytes <= Self::HARD.inner_manifest_bytes
            && self.inner_events_bytes > 0
            && self.inner_events_bytes <= Self::HARD.inner_events_bytes
            && self.records > 0
            && self.records <= Self::HARD.records
            && self.record_bytes > 0
            && self.record_bytes <= Self::HARD.record_bytes
            && self.inner_path_bytes > 0
            && self.inner_path_bytes <= Self::HARD.inner_path_bytes
            && self.json_depth > 0
            && self.json_depth <= Self::HARD.json_depth
            && self.json_key_bytes > 0
            && self.json_key_bytes <= Self::HARD.json_key_bytes
            && self.non_payload_string_bytes > 0
            && self.non_payload_string_bytes <= Self::HARD.non_payload_string_bytes
            && self.payload_string_bytes > 0
            && self.payload_string_bytes <= Self::HARD.payload_string_bytes
            && self.references > 0
            && self.references <= Self::HARD.references
            && self.reference_depth > 0
            && self.reference_depth <= Self::HARD.reference_depth
            && self.diagnostic_bytes > 0
            && self.diagnostic_bytes <= Self::HARD.diagnostic_bytes
            && self.fresh_result_bytes > 0
            && self.fresh_result_bytes <= Self::HARD.fresh_result_bytes
    }
}

impl Default for ContextLimits {
    fn default() -> Self {
        Self::HARD
    }
}

/// External expectation entry in ContextInput.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextExpectation {
    pub input_id: String,
    pub sha256: String,
    pub require_disclosure: bool,
}

/// A digest-bound package locator reference (`Ref` in SPEC §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRef {
    pub input_id: String,
    pub sha256: String,
    pub locator: String,
}

/// UTC window specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextWindow {
    pub start: String,
    pub end: String,
    pub clock_basis: String,
}

/// External surface observation applicability entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextApplicability {
    pub surface: String,
    pub input_refs: Vec<PackageRef>,
    pub window: Option<ContextWindow>,
    pub coverage: String,
    pub retention: String,
    pub correspondence: String,
    pub source_class: String,
}

/// Invocation metadata in ContextInput.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextInvocation {
    pub tool: String,
    pub version: String,
    pub options: Vec<String>,
}

/// Relying-party context and trust inputs (`ContextInput` in SPEC §9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextInput {
    pub schema_pin: String,
    pub limits: ContextLimits,
    pub keys: Vec<String>,
    pub expectations: Vec<ContextExpectation>,
    pub applicability: Vec<ContextApplicability>,
    pub as_of: String,
    pub valid_from: String,
    pub valid_until: String,
    pub invocation: ContextInvocation,
}

impl Default for ContextInput {
    fn default() -> Self {
        Self {
            schema_pin: CAP1_SCHEMA_SHA256.to_string(),
            limits: ContextLimits::default(),
            keys: Vec::new(),
            expectations: Vec::new(),
            applicability: Vec::new(),
            as_of: "2026-09-08T00:00:00Z".to_string(),
            valid_from: "2026-09-08T00:00:00Z".to_string(),
            valid_until: "2026-09-09T00:00:00Z".to_string(),
            invocation: ContextInvocation {
                tool: "illustrative-reader".to_string(),
                version: "example-only".to_string(),
                options: Vec::new(),
            },
        }
    }
}

const VALID_SURFACES: [&str; 5] = [
    "tool_call",
    "filesystem",
    "network",
    "process",
    "transcript_join",
];

const ADMITTED_FORMATS: [&str; 13] = [
    "assay-bundle-v1",
    "assay-health-json",
    "assay-coverage-json",
    "incident-context-v1",
    "incident-results-v1",
    "ed25519-public-key-pem",
    "proxy-decision-ndjson",
    "runner-kernel-ndjson-v0",
    "runner-kernel-ndjson-v1",
    "policy-bytes",
    "dsse-attestation",
    "attestation-report",
    "inspector-protocol-793d103",
];

const KNOWN_ATTESTATION_REPORT_FIELDS: [&str; 9] = [
    "schema",
    "outcome",
    "signature_verified",
    "subject_matched",
    "artifact_sha256",
    "predicate_type",
    "subject_name",
    "extent_stated",
    "extent",
];

fn is_valid_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

fn is_valid_id(s: &str) -> bool {
    if s.is_empty() || s.len() > 64 {
        return false;
    }
    let bytes = s.as_bytes();
    if !matches!(bytes[0], b'a'..=b'z' | b'0'..=b'9') {
        return false;
    }
    bytes
        .iter()
        .all(|&b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
}

fn parse_utc_timestamp(s: &str) -> Option<DateTime<chrono::FixedOffset>> {
    if !s.ends_with('Z') {
        return None;
    }
    let without_z = &s[..s.len() - 1];
    let (_date_part, time_part) = without_z.split_once('T')?;
    let (hms, fraction) = match time_part.split_once('.') {
        Some((hms, frac)) => (hms, Some(frac)),
        None => (time_part, None),
    };
    if let Some(frac) = fraction {
        if frac.is_empty() || frac.len() > 9 || !frac.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
    }
    let mut parts = hms.split(':');
    let _hh = parts.next()?;
    let _mm = parts.next()?;
    let ss = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if ss == "60" {
        return None;
    }
    DateTime::parse_from_rfc3339(s).ok()
}

// Phase 1 validation of caller ContextInput
const VALID_OBSERVATIONS: &[&str] = &["unknown", "limited", "adequate"];

// These checks receive an already materialized ContextInput. They do not bound
// the caller's parsing/allocation; they preserve hard-before-selected refusal.
fn validate_context_text(text: &str, limits: &ContextLimits) -> Result<(), IncidentReason> {
    if text.len() as u64 > ContextLimits::HARD.non_payload_string_bytes {
        return Err(IncidentReason::ResourceHard);
    }
    if text.len() as u64 > limits.non_payload_string_bytes {
        return Err(IncidentReason::ResourceSelected);
    }
    if text.trim().is_empty() {
        return Err(IncidentReason::TrustInput);
    }
    Ok(())
}

fn validate_applicability(
    app: &ContextApplicability,
    limits: &ContextLimits,
) -> Result<(), IncidentReason> {
    let w = app.window.as_ref().ok_or(IncidentReason::TrustInput)?;
    validate_context_text(&w.clock_basis, limits)?;
    let w_start = parse_utc_timestamp(&w.start).ok_or(IncidentReason::TrustInput)?;
    let w_end = parse_utc_timestamp(&w.end).ok_or(IncidentReason::TrustInput)?;
    if w_start > w_end {
        return Err(IncidentReason::TrustInput);
    }
    for grade in [&app.coverage, &app.retention] {
        if !VALID_OBSERVATIONS.contains(&grade.as_str()) {
            return Err(IncidentReason::TrustInput);
        }
    }
    if !matches!(
        app.correspondence.as_str(),
        "unique" | "ambiguous" | "unknown"
    ) {
        return Err(IncidentReason::TrustInput);
    }
    // Reuse the producer's closed vocabulary; the package additionally admits
    // the explicit unknown class without upgrading any provenance assertion.
    if app.source_class != "unknown" {
        let value =
            serde::de::value::StrDeserializer::<serde::de::value::Error>::new(&app.source_class);
        crate::coding_agent::CodingAgentSourceClass::deserialize(value)
            .map_err(|_| IncidentReason::TrustInput)?;
    }
    for reference in &app.input_refs {
        validate_context_text(&reference.locator, limits)?;
        if !is_valid_id(&reference.input_id) || !is_valid_hex64(&reference.sha256) {
            return Err(IncidentReason::TrustInput);
        }
    }
    Ok(())
}

/// Validate the typed external context before incident file admission.
pub fn validate_context_input(ctx: &ContextInput) -> Result<(), IncidentReason> {
    if ctx.schema_pin != CAP1_SCHEMA_SHA256 {
        return Err(IncidentReason::TrustInput);
    }
    if !ctx.limits.is_valid() {
        return Err(IncidentReason::TrustInput);
    }
    // Keys: unique and sorted, each 64 lowercase hex
    for i in 0..ctx.keys.len() {
        if !is_valid_hex64(&ctx.keys[i]) {
            return Err(IncidentReason::TrustInput);
        }
        if i > 0 && ctx.keys[i - 1] >= ctx.keys[i] {
            return Err(IncidentReason::TrustInput);
        }
    }
    // Expectations: unique and sorted by input_id
    for i in 0..ctx.expectations.len() {
        let exp = &ctx.expectations[i];
        if !is_valid_id(&exp.input_id) || !is_valid_hex64(&exp.sha256) {
            return Err(IncidentReason::TrustInput);
        }
        if i > 0 && ctx.expectations[i - 1].input_id >= exp.input_id {
            return Err(IncidentReason::TrustInput);
        }
    }
    // Timestamps: valid UTC and valid_from <= as_of <= valid_until
    let t_as_of = parse_utc_timestamp(&ctx.as_of).ok_or(IncidentReason::TrustInput)?;
    let t_from = parse_utc_timestamp(&ctx.valid_from).ok_or(IncidentReason::TrustInput)?;
    let t_until = parse_utc_timestamp(&ctx.valid_until).ok_or(IncidentReason::TrustInput)?;
    if t_from > t_as_of || t_as_of > t_until {
        return Err(IncidentReason::TrustInput);
    }
    // Applicability: at most one row per surface, in fixed Surface order
    let mut last_surface_idx: Option<usize> = None;
    for app in &ctx.applicability {
        let idx = VALID_SURFACES
            .iter()
            .position(|&s| s == app.surface)
            .ok_or(IncidentReason::TrustInput)?;
        if let Some(prev) = last_surface_idx {
            if idx <= prev {
                return Err(IncidentReason::TrustInput);
            }
        }
        last_surface_idx = Some(idx);
        validate_applicability(app, &ctx.limits)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryBinding {
    bundle_input: String,
    key_input: String,
    attestation_input: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryInput {
    id: String,
    sha256: String,
    bytes: u64,
    format: String,
    surfaces: Vec<String>,
    binding: Option<InventoryBinding>,
}

const CONTEXT_KINDS: &[&str] = &[
    "original_digests",
    "chain_gaps",
    "custody",
    "producer_invocation",
    "clock",
    "actor",
    "correlation",
    "policy_activation",
    "recoverability",
    "resource_effects",
    "context_provenance",
    "denominator",
    "missing_evidence",
    "withholding_derivatives",
    "retention",
    "offline_verification",
    "signature_basis",
    "schema_versions",
    "windows",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryDisclosure {
    // Value (rather than Option) keeps a required null distinct from a missing field.
    value: serde_json::Value,
    reason: String,
    refs: Vec<PackageRef>,
}

fn valid_inventory_context(context: &BTreeMap<String, InventoryDisclosure>) -> bool {
    context.len() == CONTEXT_KINDS.len()
        && context.iter().all(|(kind, disclosure)| {
            CONTEXT_KINDS.contains(&kind.as_str())
                && match disclosure.reason.as_str() {
                    "stated" => disclosure.value.is_string() && !disclosure.refs.is_empty(),
                    "not_available" | "not_applicable" => disclosure.value.is_null(),
                    _ => false,
                }
                && disclosure.refs.iter().all(|reference| {
                    is_valid_id(&reference.input_id) && is_valid_hex64(&reference.sha256)
                })
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryDocument {
    schema: String,
    assessment_sha256: String,
    inputs: Vec<InventoryInput>,
    context: BTreeMap<String, InventoryDisclosure>,
    non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode")]
enum UnitDisclosure {
    #[serde(rename = "inline")]
    Inline { value: ExaminationResult },
    #[serde(rename = "reference")]
    Reference { target: PackageRef },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExaminationResult {
    operation: String,
    source: PackageRef,
    value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IncidentResultsDocument {
    schema: String,
    results: Vec<ExaminationResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentUnit {
    id: String,
    source: PackageRef,
    surface: String,
    disposition: String,
    result: Option<UnitDisclosure>,
    withheld_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentWindow {
    start: String,
    end: String,
    clock_basis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SurfaceResult {
    name: String,
    source_inputs: Vec<String>,
    observation: String,
    basis_refs: Vec<PackageRef>,
    window: Option<AssessmentWindow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentJoin {
    id: String,
    left: PackageRef,
    right: Option<PackageRef>,
    contract: JoinContract,
    proposition: JoinProposition,
    result: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JoinContract {
    schema: String,
    left_artifact_role: String,
    right_artifact_role: String,
    join_key: String,
    join_value: Option<String>,
    join_grade: String,
    scope: String,
    unique_within_scope: bool,
    fallback_used: bool,
    evidence_refs: Vec<String>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JoinProposition {
    kind: String,
    left_pointer: String,
    right_pointer: String,
    expected: serde_json::Value,
    surface: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentClaim {
    id: String,
    kind: String,
    surface: String,
    stratum: String,
    proposition: String,
    evidence_refs: Vec<PackageRef>,
    decision: String,
}

fn validate_join_claim_shape(assessment: &AssessmentDocument) -> bool {
    let roles = [
        "otel_family_trace",
        "measured_run_archive",
        "joined_artifacts",
        "external_receipt",
    ];
    let join_keys = [
        "tool_call_id",
        "run_id",
        "session_id",
        "trace_span_id",
        "timestamp_or_order",
    ];
    let grades = ["strong", "weak", "diagnostic", "failed"];
    let scopes = ["tool_call", "run", "session", "trace_local", "diagnostic"];
    let mut previous = None;
    for join in &assessment.joins {
        if !is_valid_id(&join.id) || previous.is_some_and(|id: &str| id >= join.id.as_str()) {
            return false;
        }
        previous = Some(join.id.as_str());
        let contract = &join.contract;
        if contract.schema != "assay.observability.join_result.v0"
            || !roles.contains(&contract.left_artifact_role.as_str())
            || !roles.contains(&contract.right_artifact_role.as_str())
            || !join_keys.contains(&contract.join_key.as_str())
            || !grades.contains(&contract.join_grade.as_str())
            || !scopes.contains(&contract.scope.as_str())
            || contract.evidence_refs.is_empty()
            || contract
                .evidence_refs
                .iter()
                .any(|reference| reference.is_empty())
            || contract.notes.iter().any(|note| note.is_empty())
            || !matches!(
                join.result.as_str(),
                "confirmed" | "contradicted" | "unobservable"
            )
            || !matches!(
                join.proposition.kind.as_str(),
                "equal_retained_value" | "counterpart_present"
            )
            || !join.proposition.left_pointer.starts_with('/')
            || !join.proposition.right_pointer.starts_with('/')
            || !VALID_SURFACES.contains(&join.proposition.surface.as_str())
            || join.proposition.expected.is_array()
            || join.proposition.expected.is_object()
            || (join.proposition.expected.is_number()
                && join.proposition.expected.as_u64().is_none())
            || (join.proposition.kind == "equal_retained_value"
                && !join.proposition.expected.is_null())
            || (join.proposition.kind == "counterpart_present"
                && join.proposition.expected.is_null())
        {
            return false;
        }
        if contract.join_grade == "strong"
            && (!matches!(contract.join_key.as_str(), "tool_call_id" | "run_id")
                || !matches!(contract.scope.as_str(), "tool_call" | "run")
                || !contract.unique_within_scope
                || contract.fallback_used)
        {
            return false;
        }
        if contract.join_key == "run_id" && contract.scope != "run" {
            return false;
        }
        if contract.join_key == "tool_call_id"
            && contract.unique_within_scope
            && !contract.fallback_used
            && contract.scope != "tool_call"
        {
            return false;
        }
        if matches!(
            contract.join_key.as_str(),
            "session_id" | "trace_span_id" | "timestamp_or_order"
        ) && contract.join_grade == "strong"
        {
            return false;
        }
        if contract.join_grade == "failed"
            && (contract.join_value.is_some() || contract.fallback_used)
        {
            return false;
        }
    }
    previous = None;
    for claim in &assessment.claims {
        if !is_valid_id(&claim.id) || previous.is_some_and(|id: &str| id >= claim.id.as_str()) {
            return false;
        }
        previous = Some(claim.id.as_str());
        if !matches!(
            claim.kind.as_str(),
            "positive_presence" | "bounded_negative" | "exhaustive_set"
        ) || !VALID_SURFACES.contains(&claim.surface.as_str())
            || !is_valid_id(&claim.stratum)
            || !is_valid_id(&claim.proposition)
            || !matches!(
                claim.decision.as_str(),
                "supported" | "degraded" | "blocked"
            )
        {
            return false;
        }
    }
    true
}

fn required_nullable_join_members_present(value: &serde_json::Value) -> bool {
    value
        .get("joins")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|joins| {
            joins.iter().all(|join| {
                join.as_object()
                    .is_some_and(|object| object.contains_key("right"))
            })
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DerivedSurfaceResult {
    observation: String,
    basis_refs: Vec<PackageRef>,
    window: Option<AssessmentWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ObservationCeiling {
    Adequate,
    Limited,
    Unknown,
}

#[derive(Debug, Clone, Default)]
struct RetainedContextInventory {
    ceilings: BTreeMap<&'static str, ObservationCeiling>,
    correspondence_ambiguous: bool,
    enforcement_degraded: bool,
}

#[derive(Debug, Clone)]
struct AdmittedActionRecord {
    reference: PackageRef,
    surface: &'static str,
    operation: String,
    value: serde_json::Value,
    timestamp: Option<DateTime<chrono::FixedOffset>>,
    examination_succeeded: bool,
}

#[derive(Debug, Clone)]
struct ParsedBundle {
    events_sha256: String,
    events: Vec<ParsedCanonicalEvent>,
}

#[derive(Debug, Clone)]
struct ParsedCanonicalEvent {
    typed: EvidenceEvent,
    original: serde_json::Value,
}

impl RetainedContextInventory {
    fn reduce(&mut self, surface: &'static str, ceiling: ObservationCeiling) {
        self.ceilings
            .entry(surface)
            .and_modify(|current| *current = (*current).max(ceiling))
            .or_insert(ceiling);
    }

    fn ceiling(&self, surface: &str) -> ObservationCeiling {
        self.ceilings
            .get(surface)
            .copied()
            .unwrap_or(ObservationCeiling::Adequate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveRefError {
    Locator,
    InputShape,
    UnknownSchema,
    Mapping,
    ResourceHard,
    ResourceSelected,
}

impl std::fmt::Display for ResolveRefError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Locator => "incident reference locator failure",
            Self::InputShape => "incident referenced input shape failure",
            Self::UnknownSchema => "incident referenced input schema failure",
            Self::Mapping => "incident referenced input surface mapping failure",
            Self::ResourceHard => "incident referenced input hard resource ceiling",
            Self::ResourceSelected => "incident referenced input selected resource ceiling",
        })
    }
}

impl std::error::Error for ResolveRefError {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct IncidentContextDocument {
    schema: String,
    entries: Vec<IncidentContextEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct IncidentContextEntry {
    kind: String,
    value: String,
    source_refs: Vec<PackageRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct EnforcementHealthV0Wire {
    schema: String,
    scope: String,
    network_enforcement: String,
    attach_confirmed: bool,
    blocked_count: u64,
    allowed_count: u64,
    enforcement_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct EnforcementFailureV1Wire {
    reason_code: String,
    detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct LandlockBlockV1Wire {
    abi: u32,
    net_connect_tcp_supported: Option<bool>,
    handled_access_net: Option<Vec<String>>,
    allowed_connect_tcp_ports: Option<Vec<u16>>,
    no_new_privs_confirmed: bool,
    restrict_self_confirmed: bool,
    restriction_shedding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct EnforcementProbeV1Wire {
    kind: String,
    transport: String,
    blocked_action: String,
    blocked_port: u16,
    blocked_errno: String,
    listener_reached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct EnforcementHealthV1Wire {
    schema: String,
    status: String,
    mechanism: String,
    scope: String,
    policy_semantics: String,
    enforcement_class: String,
    failure: Option<EnforcementFailureV1Wire>,
    landlock: LandlockBlockV1Wire,
    probe: serde_json::Value,
    non_claims: Vec<String>,
}

fn parse_strict_json(bytes: &[u8]) -> Result<serde_json::Value, ResolveRefError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ResolveRefError::InputShape)?;
    validate_json_strict(text).map_err(|_| ResolveRefError::InputShape)?;
    serde_json::from_str(text).map_err(|_| ResolveRefError::InputShape)
}

#[derive(Default)]
struct SourceBudgetState {
    inputs: u64,
    records: u64,
    references: u64,
}

impl SourceBudgetState {
    fn charge_input(&mut self, context: &ContextInput) -> Result<(), BudgetHit> {
        self.inputs = check_next(
            self.inputs,
            1,
            BudgetLimit {
                hard: ContextLimits::HARD.inputs,
                selected: context.limits.inputs,
            },
            BudgetDimension::Items,
        )?;
        Ok(())
    }

    fn charge_record(&mut self, context: &ContextInput) -> Result<(), BudgetHit> {
        self.records = check_next(
            self.records,
            1,
            BudgetLimit {
                hard: ContextLimits::HARD.records,
                selected: context.limits.records,
            },
            BudgetDimension::Items,
        )?;
        Ok(())
    }

    fn charge_reference(&mut self, context: &ContextInput) -> Result<(), BudgetHit> {
        self.references = check_next(
            self.references,
            1,
            BudgetLimit {
                hard: ContextLimits::HARD.references,
                selected: context.limits.references,
            },
            BudgetDimension::Items,
        )?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum SourceJsonKind {
    Inventory,
    Assessment,
    Canonical,
    Proxy,
    Kernel,
    Inspector,
    IncidentContext,
    IncidentResults,
    Dsse,
    Metadata,
}

struct SourceJsonHooks<'a> {
    kind: SourceJsonKind,
    budget: &'a mut SourceBudgetState,
    context: &'a ContextInput,
}

impl SourceJsonHooks<'_> {
    fn path_is(path: &[PathSegment], keys: &[&str]) -> bool {
        path.len() == keys.len()
            && path.iter().zip(keys).all(|(part, key)| {
                matches!(part, PathSegment::Key(actual) if actual == key)
                    || (*key == "*" && matches!(part, PathSegment::Index(_)))
            })
    }

    fn path_starts_with(path: &[PathSegment], keys: &[&str]) -> bool {
        path.len() >= keys.len() && Self::path_is(&path[..keys.len()], keys)
    }
}

impl BoundedJsonHooks for SourceJsonHooks<'_> {
    fn object_start(&mut self, path: &[PathSegment]) -> Result<Option<BudgetLimit>, BudgetHit> {
        let assessment_ref = matches!(self.kind, SourceJsonKind::Assessment)
            && (Self::path_is(path, &["units", "*", "source"])
                || Self::path_is(path, &["units", "*", "result", "value", "source"])
                || Self::path_is(path, &["units", "*", "result", "target"])
                || Self::path_is(path, &["joins", "*", "left"])
                || Self::path_is(path, &["joins", "*", "right"]));
        let results_ref = matches!(self.kind, SourceJsonKind::IncidentResults)
            && Self::path_is(path, &["results", "*", "source"]);
        if assessment_ref || results_ref {
            self.budget.charge_reference(self.context)?;
        }
        Ok(None)
    }

    fn item_start(
        &mut self,
        path: &[PathSegment],
        _index: usize,
    ) -> Result<Option<BudgetLimit>, BudgetHit> {
        let is_record = matches!(self.kind, SourceJsonKind::Inspector) && path.is_empty()
            || matches!(self.kind, SourceJsonKind::IncidentContext)
                && Self::path_is(path, &["entries"])
            || matches!(self.kind, SourceJsonKind::IncidentResults)
                && Self::path_is(path, &["results"]);
        if is_record {
            self.budget.charge_record(self.context)?;
            return Ok(Some(BudgetLimit {
                hard: ContextLimits::HARD.record_bytes,
                selected: self.context.limits.record_bytes,
            }));
        }
        let is_input =
            matches!(self.kind, SourceJsonKind::Inventory) && Self::path_is(path, &["inputs"]);
        if is_input {
            self.budget.charge_input(self.context)?;
        }
        let is_reference = (matches!(self.kind, SourceJsonKind::Inventory)
            && path.len() == 3
            && matches!(&path[0], PathSegment::Key(key) if key == "context")
            && matches!(&path[1], PathSegment::Key(_))
            && matches!(&path[2], PathSegment::Key(key) if key == "refs"))
            || (matches!(self.kind, SourceJsonKind::Assessment)
                && (Self::path_is(path, &["surfaces", "*", "basis_refs"])
                    || Self::path_is(path, &["claims", "*", "evidence_refs"])
                    || Self::path_is(path, &["joins", "*", "contract", "evidence_refs"])
                    || Self::path_is(path, &["resolved_results"])))
            || (matches!(self.kind, SourceJsonKind::IncidentContext)
                && Self::path_is(path, &["entries", "*", "source_refs"]));
        if is_reference {
            self.budget.charge_reference(self.context)?;
        }
        Ok(None)
    }

    fn string_class(&self, path: &[PathSegment]) -> StringClass {
        let proxy_payload = matches!(self.kind, SourceJsonKind::Proxy)
            && matches!(path.first(), Some(PathSegment::Key(key)) if key == "data");
        let kernel_payload = matches!(self.kind, SourceJsonKind::Kernel)
            && matches!(path, [PathSegment::Key(key)] if key == "value");
        let canonical_payload = matches!(self.kind, SourceJsonKind::Canonical)
            && matches!(path.first(), Some(PathSegment::Key(key)) if key == "data");
        let inspector_payload = matches!(self.kind, SourceJsonKind::Inspector)
            && (Self::path_starts_with(path, &["*", "message", "params", "arguments"])
                || Self::path_starts_with(path, &["*", "message", "result"])
                || Self::path_starts_with(path, &["*", "message", "error"])
                || Self::path_starts_with(path, &["*", "response", "result"])
                || Self::path_starts_with(path, &["*", "response", "error"]));
        let inventory_payload = matches!(self.kind, SourceJsonKind::Inventory)
            && path.len() >= 3
            && matches!(&path[0], PathSegment::Key(key) if key == "context")
            && matches!(&path[2], PathSegment::Key(key) if key == "value");
        let assessment_payload = matches!(self.kind, SourceJsonKind::Assessment)
            && Self::path_starts_with(path, &["units", "*", "result", "value", "value"]);
        let results_payload = matches!(self.kind, SourceJsonKind::IncidentResults)
            && path.len() >= 3
            && Self::path_is(&path[..3], &["results", "*", "value"]);
        let dsse_payload =
            matches!(self.kind, SourceJsonKind::Dsse) && Self::path_starts_with(path, &["payload"]);
        if proxy_payload
            || kernel_payload
            || canonical_payload
            || inspector_payload
            || inventory_payload
            || assessment_payload
            || results_payload
            || dsse_payload
        {
            StringClass::Payload
        } else {
            StringClass::NonPayload
        }
    }
}

fn bounded_json_limits(context: &ContextInput) -> BoundedJsonLimits {
    BoundedJsonLimits {
        depth: BudgetLimit {
            hard: ContextLimits::HARD.json_depth,
            selected: context.limits.json_depth,
        },
        key_bytes: BudgetLimit {
            hard: ContextLimits::HARD.json_key_bytes,
            selected: context.limits.json_key_bytes,
        },
        non_payload_string_bytes: BudgetLimit {
            hard: ContextLimits::HARD.non_payload_string_bytes,
            selected: context.limits.non_payload_string_bytes,
        },
        payload_string_bytes: BudgetLimit {
            hard: ContextLimits::HARD.payload_string_bytes,
            selected: context.limits.payload_string_bytes,
        },
    }
}

fn map_bounded_json_error(error: BoundedJsonError) -> ResolveRefError {
    match error {
        BoundedJsonError::Strict(_) => ResolveRefError::InputShape,
        BoundedJsonError::Budget(BudgetHit {
            tier: BudgetTier::Hard,
            ..
        }) => ResolveRefError::ResourceHard,
        BoundedJsonError::Budget(BudgetHit {
            tier: BudgetTier::Selected,
            ..
        }) => ResolveRefError::ResourceSelected,
    }
}

fn preflight_json(
    text: &str,
    kind: SourceJsonKind,
    budget: &mut SourceBudgetState,
    context: &ContextInput,
) -> Result<(), ResolveRefError> {
    let mut hooks = SourceJsonHooks {
        kind,
        budget,
        context,
    };
    validate_json_strict_bounded(text, bounded_json_limits(context), &mut hooks)
        .map_err(map_bounded_json_error)
}

fn preflight_single_json_source(
    bytes: &[u8],
    format: &str,
    kind: SourceJsonKind,
    budget: &mut SourceBudgetState,
    context: &ContextInput,
) -> Result<(), ResolveRefError> {
    check_source_bytes(format, bytes.len() as u64, context)?;
    let text = std::str::from_utf8(bytes).map_err(|_| ResolveRefError::InputShape)?;
    preflight_json(text, kind, budget, context)
}

fn parse_bounded_context_json(
    bytes: &[u8],
    context: &ContextInput,
) -> Result<serde_json::Value, ResolveRefError> {
    check_source_bytes("incident-context-v1", bytes.len() as u64, context)?;
    let text = std::str::from_utf8(bytes).map_err(|_| ResolveRefError::InputShape)?;
    if let Err(error) =
        validate_json_strict_with_depth(text, ContextLimits::HARD.json_depth as usize)
    {
        return Err(if matches!(error, StrictJsonError::NestingTooDeep { .. }) {
            ResolveRefError::ResourceHard
        } else {
            ResolveRefError::InputShape
        });
    }
    if let Err(error) = validate_json_strict_with_depth(text, context.limits.json_depth as usize) {
        return Err(if matches!(error, StrictJsonError::NestingTooDeep { .. }) {
            ResolveRefError::ResourceSelected
        } else {
            ResolveRefError::InputShape
        });
    }
    serde_json::from_str(text).map_err(|_| ResolveRefError::InputShape)
}

fn validate_enforcement_health_v0(value: serde_json::Value) -> Result<(), ResolveRefError> {
    let health: EnforcementHealthV0Wire =
        serde_json::from_value(value).map_err(|_| ResolveRefError::InputShape)?;
    if health.schema != "assay.enforcement_health.v0" || health.scope != "ipv4_tcp_connect" {
        return Err(ResolveRefError::InputShape);
    }
    let active = health.network_enforcement == "active";
    let inactive = matches!(
        health.network_enforcement.as_str(),
        "absent" | "failed" | "not_applicable"
    );
    if (!active && !inactive)
        || (active && (!health.attach_confirmed || health.enforcement_class != "strong"))
        || (inactive
            && (health.attach_confirmed
                || health.enforcement_class != "basic"
                || health.blocked_count != 0
                || health.allowed_count != 0))
    {
        return Err(ResolveRefError::InputShape);
    }
    Ok(())
}

fn validate_enforcement_health_v1(value: serde_json::Value) -> Result<(), ResolveRefError> {
    let health: EnforcementHealthV1Wire =
        serde_json::from_value(value).map_err(|_| ResolveRefError::InputShape)?;
    if health.schema != "assay.enforcement_health.v1"
        || health.mechanism != "landlock"
        || health.scope != "tcp_connect_landlock_port"
        || health.policy_semantics != "allowlist"
    {
        return Err(ResolveRefError::InputShape);
    }
    let shedding_valid = health
        .landlock
        .restriction_shedding
        .as_deref()
        .is_none_or(|value| {
            matches!(
                value,
                "restrictions_held" | "restrictions_shed" | "inconclusive"
            )
        });
    let probe = if health.probe.is_null() {
        None
    } else {
        Some(
            serde_json::from_value::<EnforcementProbeV1Wire>(health.probe.clone())
                .map_err(|_| ResolveRefError::InputShape)?,
        )
    };
    let active = health.status == "active"
        && health.enforcement_class == "strong"
        && health.failure.is_none()
        && health.landlock.no_new_privs_confirmed
        && health.landlock.restrict_self_confirmed
        && health.landlock.handled_access_net.as_deref() == Some(&["connect_tcp".to_string()])
        && health.landlock.allowed_connect_tcp_ports.is_some()
        && health.landlock.net_connect_tcp_supported.is_none();
    let failed = health.status == "failed"
        && health.enforcement_class == "basic"
        && health.failure.as_ref().is_some_and(|failure| {
            matches!(
                failure.reason_code.as_str(),
                "landlock_abi_too_old"
                    | "landlock_disabled"
                    | "no_new_privs_failed"
                    | "restrict_self_failed"
                    | "ruleset_build_failed"
                    | "policy_not_expressible"
            ) && !failure.detail.trim().is_empty()
        })
        && !health.landlock.restrict_self_confirmed
        && health.landlock.net_connect_tcp_supported.is_some()
        && health.landlock.handled_access_net.is_none()
        && health.landlock.allowed_connect_tcp_ports.is_none()
        && health.landlock.restriction_shedding.is_none()
        && probe.is_none();
    if (!active && !failed) || !shedding_valid {
        return Err(ResolveRefError::InputShape);
    }
    if let Some(probe) = &probe {
        let _ = (
            &probe.kind,
            &probe.transport,
            &probe.blocked_action,
            probe.blocked_port,
            &probe.blocked_errno,
            probe.listener_reached,
        );
        if health.status != "active" {
            return Err(ResolveRefError::InputShape);
        }
    }
    let _ = (health.landlock.abi, &health.non_claims);
    Ok(())
}

fn incident_bundle_limits(context: &ContextInput, remaining_records: u64) -> VerifyLimits {
    VerifyLimits {
        max_bundle_bytes: context
            .limits
            .inner_compressed_bytes
            .min(ContextLimits::HARD.inner_compressed_bytes),
        max_decode_bytes: context
            .limits
            .decoded_bytes_total
            .min(ContextLimits::HARD.decoded_bytes_total),
        max_manifest_bytes: context
            .limits
            .inner_manifest_bytes
            .min(ContextLimits::HARD.inner_manifest_bytes),
        max_events_bytes: context
            .limits
            .inner_events_bytes
            .min(ContextLimits::HARD.inner_events_bytes),
        max_events: remaining_records.min(usize::MAX as u64) as usize,
        max_line_bytes: context
            .limits
            .record_bytes
            .min(ContextLimits::HARD.record_bytes) as usize,
        max_path_len: context
            .limits
            .inner_path_bytes
            .min(ContextLimits::HARD.inner_path_bytes) as usize,
        max_json_depth: context
            .limits
            .json_depth
            .min(ContextLimits::HARD.json_depth) as usize,
    }
}

fn parse_bundle_inputs(
    inventory: &InventoryDocument,
    object_map: &BTreeMap<&str, &[u8]>,
    context: &ContextInput,
    source_budget: &mut SourceBudgetState,
) -> Result<BTreeMap<String, ParsedBundle>, ResolveRefError> {
    let mut decoded_total = 0u64;
    let mut parsed = BTreeMap::new();
    for input in &inventory.inputs {
        let object_path = format!("objects/{}", input.sha256);
        let bytes = object_map
            .get(object_path.as_str())
            .ok_or(ResolveRefError::Locator)?;
        if input.format != "assay-bundle-v1" {
            preflight_non_bundle_source(input, bytes, source_budget, context)?;
            continue;
        }
        check_source_bytes(&input.format, bytes.len() as u64, context)?;
        let mut decoder = flate2::read::GzDecoder::new(Cursor::new(*bytes));
        let mut chunk = [0u8; 8192];
        loop {
            let read = decoder
                .read(&mut chunk)
                .map_err(|_| ResolveRefError::InputShape)?;
            if read == 0 {
                break;
            }
            decoded_total = decoded_total
                .checked_add(read as u64)
                .ok_or(ResolveRefError::ResourceHard)?;
            if decoded_total > ContextLimits::HARD.decoded_bytes_total {
                return Err(ResolveRefError::ResourceHard);
            }
            if decoded_total > context.limits.decoded_bytes_total {
                return Err(ResolveRefError::ResourceSelected);
            }
        }
        let mut json_preflight = |kind, text: &str| {
            let source_kind = match kind {
                BundleJsonKind::Manifest => SourceJsonKind::Metadata,
                BundleJsonKind::Event => SourceJsonKind::Canonical,
            };
            if kind == BundleJsonKind::Event {
                source_budget
                    .charge_record(context)
                    .map_err(|hit| map_bounded_json_error(BoundedJsonError::Budget(hit)))?;
            }
            preflight_json(text, source_kind, source_budget, context).map_err(anyhow::Error::new)
        };
        let reader = BundleReader::open_with_json_preflight(
            Cursor::new(*bytes),
            incident_bundle_limits(
                context,
                context.limits.records.min(ContextLimits::HARD.records),
            ),
            &mut json_preflight,
        )
        .map_err(|error| map_bundle_open_error(&error, context))?;
        let events_sha256 = reader
            .manifest()
            .files
            .get("events.ndjson")
            .ok_or(ResolveRefError::InputShape)?
            .sha256
            .clone();
        let original_events = std::str::from_utf8(reader.events_raw())
            .map_err(|_| ResolveRefError::InputShape)?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(|_| ResolveRefError::InputShape))
            .collect::<Result<Vec<serde_json::Value>, _>>()?;
        let typed_events = reader
            .events()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ResolveRefError::InputShape)?;
        if typed_events.len() != original_events.len() {
            return Err(ResolveRefError::InputShape);
        }
        let events = typed_events
            .into_iter()
            .zip(original_events)
            .map(|(typed, original)| ParsedCanonicalEvent { typed, original })
            .collect();
        parsed.insert(
            input.id.clone(),
            ParsedBundle {
                events_sha256,
                events,
            },
        );
    }
    Ok(parsed)
}

fn source_byte_limit(format: &str, limits: &ContextLimits) -> Option<BudgetLimit> {
    let field: fn(&ContextLimits) -> u64 = match format {
        "policy-bytes" => |l| l.policy_bytes,
        "ed25519-public-key-pem" => |l| l.key_bytes,
        "proxy-decision-ndjson"
        | "runner-kernel-ndjson-v0"
        | "runner-kernel-ndjson-v1"
        | "inspector-protocol-793d103" => |l| l.transcript_bytes,
        "assay-health-json" | "assay-coverage-json" | "incident-context-v1" => |l| l.health_bytes,
        "incident-results-v1" | "attestation-report" => |l| l.retained_report_bytes,
        "dsse-attestation" => |l| l.envelope_bytes,
        "assay-bundle-v1" => |l| l.inner_compressed_bytes,
        _ => return None,
    };
    Some(BudgetLimit {
        hard: field(&ContextLimits::HARD),
        selected: field(limits),
    })
}

fn check_source_bytes(
    format: &str,
    bytes: u64,
    context: &ContextInput,
) -> Result<(), ResolveRefError> {
    let limit = source_byte_limit(format, &context.limits).ok_or(ResolveRefError::InputShape)?;
    check_next(0, bytes, limit, BudgetDimension::EntryBytes)
        .map(|_| ())
        .map_err(|hit| map_bounded_json_error(BoundedJsonError::Budget(hit)))
}

// CR is a delimiter only as part of CRLF; preserve all record bytes otherwise.
fn kernel_record_line(raw_line: &str) -> Result<&str, ResolveRefError> {
    let line = match raw_line.strip_suffix('\n') {
        Some(line) => line.strip_suffix('\r').unwrap_or(line),
        None => raw_line,
    };
    if line.starts_with('\u{feff}') || line.trim().is_empty() || line.contains('\r') {
        return Err(ResolveRefError::InputShape);
    }
    Ok(line)
}

fn preflight_non_bundle_source(
    input: &InventoryInput,
    bytes: &[u8],
    source_budget: &mut SourceBudgetState,
    context: &ContextInput,
) -> Result<(), ResolveRefError> {
    let text = match input.format.as_str() {
        "policy-bytes" => {
            check_source_bytes(&input.format, bytes.len() as u64, context)?;
            return Ok(());
        }
        "ed25519-public-key-pem" => {
            check_source_bytes(&input.format, bytes.len() as u64, context)?;
            return Ok(());
        }
        "proxy-decision-ndjson"
        | "runner-kernel-ndjson-v0"
        | "runner-kernel-ndjson-v1"
        | "inspector-protocol-793d103" => {
            check_source_bytes(&input.format, bytes.len() as u64, context)?;
            std::str::from_utf8(bytes).map_err(|_| ResolveRefError::InputShape)?
        }
        "assay-health-json" | "assay-coverage-json" | "incident-context-v1" => {
            check_source_bytes(&input.format, bytes.len() as u64, context)?;
            std::str::from_utf8(bytes).map_err(|_| ResolveRefError::InputShape)?
        }
        "incident-results-v1" => {
            check_source_bytes(&input.format, bytes.len() as u64, context)?;
            std::str::from_utf8(bytes).map_err(|_| ResolveRefError::InputShape)?
        }
        _ => return Ok(()),
    };
    match input.format.as_str() {
        "proxy-decision-ndjson" => {
            for raw_line in text.split_inclusive('\n') {
                let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
                let line = line.strip_suffix('\r').unwrap_or(line);
                if line.trim().is_empty() {
                    return Err(ResolveRefError::InputShape);
                }
                source_budget
                    .charge_record(context)
                    .map_err(|hit| map_bounded_json_error(BoundedJsonError::Budget(hit)))?;
                if line.len() as u64 > ContextLimits::HARD.record_bytes {
                    return Err(ResolveRefError::ResourceHard);
                }
                if line.len() as u64 > context.limits.record_bytes {
                    return Err(ResolveRefError::ResourceSelected);
                }
                preflight_json(line, SourceJsonKind::Proxy, source_budget, context)?;
            }
        }
        "runner-kernel-ndjson-v0" | "runner-kernel-ndjson-v1" => {
            for raw_line in text.split_inclusive('\n') {
                let line = kernel_record_line(raw_line)?;
                source_budget
                    .charge_record(context)
                    .map_err(|hit| map_bounded_json_error(BoundedJsonError::Budget(hit)))?;
                if line.len() as u64 > ContextLimits::HARD.record_bytes {
                    return Err(ResolveRefError::ResourceHard);
                }
                if line.len() as u64 > context.limits.record_bytes {
                    return Err(ResolveRefError::ResourceSelected);
                }
                preflight_json(line, SourceJsonKind::Kernel, source_budget, context)?;
            }
        }
        "inspector-protocol-793d103" => {
            preflight_json(text, SourceJsonKind::Inspector, source_budget, context)?;
        }
        "incident-context-v1" => {
            preflight_json(
                text,
                SourceJsonKind::IncidentContext,
                source_budget,
                context,
            )?;
        }
        "incident-results-v1" => {
            preflight_json(
                text,
                SourceJsonKind::IncidentResults,
                source_budget,
                context,
            )?;
        }
        "assay-health-json" | "assay-coverage-json" => {
            source_budget
                .charge_record(context)
                .map_err(|hit| map_bounded_json_error(BoundedJsonError::Budget(hit)))?;
            if bytes.len() as u64 > ContextLimits::HARD.record_bytes {
                return Err(ResolveRefError::ResourceHard);
            }
            if bytes.len() as u64 > context.limits.record_bytes {
                return Err(ResolveRefError::ResourceSelected);
            }
            preflight_json(text, SourceJsonKind::Metadata, source_budget, context)?;
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn map_bundle_open_error(error: &anyhow::Error, context: &ContextInput) -> ResolveRefError {
    if let Some(error) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<ResolveRefError>())
    {
        return *error;
    }
    let Some(verify_error) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<VerifyError>())
    else {
        return ResolveRefError::InputShape;
    };
    if verify_error.class != ErrorClass::Limits {
        return ResolveRefError::InputShape;
    }
    let selected_is_lower = match verify_error.code {
        ErrorCode::LimitBundleBytes => {
            context.limits.inner_compressed_bytes < ContextLimits::HARD.inner_compressed_bytes
        }
        ErrorCode::LimitDecodeBytes => {
            context.limits.decoded_bytes_total < ContextLimits::HARD.decoded_bytes_total
        }
        ErrorCode::LimitFileSize => {
            let Some(cause) = error
                .chain()
                .find_map(|cause| cause.downcast_ref::<MemberLimitCause>())
            else {
                return ResolveRefError::InputShape;
            };
            let (selected, hard) = match cause.budget {
                MemberLimitBudget::Manifest => (
                    context.limits.inner_manifest_bytes,
                    ContextLimits::HARD.inner_manifest_bytes,
                ),
                MemberLimitBudget::Events => (
                    context.limits.inner_events_bytes,
                    ContextLimits::HARD.inner_events_bytes,
                ),
            };
            let effective = selected.min(hard);
            match cause.evidence {
                MemberLimitEvidence::DeclaredSize { actual, limit }
                    if limit == effective && actual > hard =>
                {
                    return ResolveRefError::ResourceHard;
                }
                MemberLimitEvidence::DeclaredSize { actual, limit }
                    if limit == effective && actual > selected =>
                {
                    return ResolveRefError::ResourceSelected;
                }
                MemberLimitEvidence::StreamingBoundary { limit } if limit == effective => {
                    return if selected < hard {
                        ResolveRefError::ResourceSelected
                    } else {
                        ResolveRefError::ResourceHard
                    };
                }
                _ => return ResolveRefError::InputShape,
            }
        }
        ErrorCode::LimitTotalEvents => context.limits.records < ContextLimits::HARD.records,
        ErrorCode::LimitLineBytes => context.limits.record_bytes < ContextLimits::HARD.record_bytes,
        ErrorCode::LimitPathLength => {
            context.limits.inner_path_bytes < ContextLimits::HARD.inner_path_bytes
        }
        ErrorCode::LimitJsonDepth => context.limits.json_depth < ContextLimits::HARD.json_depth,
        _ => false,
    };
    if selected_is_lower {
        ResolveRefError::ResourceSelected
    } else {
        ResolveRefError::ResourceHard
    }
}

#[cfg(test)]
mod member_limit_mapping_tests {
    use super::*;

    fn member_error(cause: MemberLimitCause) -> anyhow::Error {
        anyhow::Error::new(
            VerifyError::new(
                ErrorClass::Limits,
                ErrorCode::LimitFileSize,
                "member byte ceiling exceeded",
            )
            .with_source(cause),
        )
    }

    #[test]
    fn an_untyped_member_limit_is_not_guessed_from_an_equal_numeric_ceiling() {
        let mut context = ContextInput::default();
        context.limits.inner_manifest_bytes = ContextLimits::HARD.inner_manifest_bytes;
        context.limits.inner_events_bytes = ContextLimits::HARD.inner_manifest_bytes;
        let error = anyhow::Error::new(VerifyError::new(
            ErrorClass::Limits,
            ErrorCode::LimitFileSize,
            format!(
                "A declared member size exceeds the configured maximum of {} bytes",
                ContextLimits::HARD.inner_manifest_bytes
            ),
        ));

        assert_eq!(
            map_bundle_open_error(&error, &context),
            ResolveRefError::InputShape,
            "the same number can name the manifest hard ceiling or the selected events ceiling"
        );
    }

    #[test]
    fn a_declared_manifest_size_uses_the_manifest_hard_ceiling() {
        let mut context = ContextInput::default();
        context.limits.inner_manifest_bytes = 1;
        context.limits.inner_events_bytes = ContextLimits::HARD.inner_manifest_bytes;
        let error = member_error(MemberLimitCause::declared(
            MemberLimitBudget::Manifest,
            ContextLimits::HARD.inner_manifest_bytes + 1,
            1,
        ));

        assert_eq!(
            map_bundle_open_error(&error, &context),
            ResolveRefError::ResourceHard
        );
    }

    #[test]
    fn a_streaming_events_boundary_uses_the_budget_that_selected_the_guard() {
        let mut context = ContextInput::default();
        context.limits.inner_events_bytes = 17;
        let selected = member_error(MemberLimitCause::streaming(
            MemberLimitBudget::Events,
            17,
            std::io::Error::other("bounded read"),
        ));
        assert_eq!(
            map_bundle_open_error(&selected, &context),
            ResolveRefError::ResourceSelected
        );

        context.limits.inner_events_bytes = ContextLimits::HARD.inner_events_bytes;
        let hard = member_error(MemberLimitCause::streaming(
            MemberLimitBudget::Events,
            ContextLimits::HARD.inner_events_bytes,
            std::io::Error::other("bounded read"),
        ));
        assert_eq!(
            map_bundle_open_error(&hard, &context),
            ResolveRefError::ResourceHard
        );
    }
}

#[cfg(test)]
mod frozen_decision_wire_tests {
    #[test]
    fn frozen_decision_wire_rejects_a_wrong_typed_optional_field() {
        let value = serde_json::json!({
            "specversion": "1.0",
            "id": "decision-1",
            "type": "assay.tool.decision",
            "source": "urn:assay:test",
            "time": "2026-10-12T00:00:00Z",
            "data": {
                "tool": "list_files",
                "decision": "allow",
                "reason_code": "allowed",
                "tool_call_id": "call-1",
                "use_count": "one"
            }
        });

        assert!(crate::incident_package::decision_wire_v1::validate(&value).is_err());
    }
}

fn incident_surface_sources(inventory: &InventoryDocument, surface: &str) -> Vec<String> {
    inventory
        .inputs
        .iter()
        .filter(|input| input.surfaces.iter().any(|candidate| candidate == surface))
        .map(|input| input.id.clone())
        .collect()
}

fn incident_refusal_for_resolution(error: ResolveRefError) -> IncidentReason {
    match error {
        ResolveRefError::Locator => IncidentReason::Locator,
        ResolveRefError::InputShape => IncidentReason::InputShape,
        ResolveRefError::UnknownSchema => IncidentReason::UnknownSchema,
        ResolveRefError::Mapping => IncidentReason::Mapping,
        ResolveRefError::ResourceHard => IncidentReason::ResourceHard,
        ResolveRefError::ResourceSelected => IncidentReason::ResourceSelected,
    }
}

fn sort_dedup_refs(refs: &mut Vec<PackageRef>) {
    refs.sort_by(|a, b| {
        (&a.input_id, &a.locator, &a.sha256).cmp(&(&b.input_id, &b.locator, &b.sha256))
    });
    refs.dedup();
}

fn canonical_event_locator(run_id: &str, seq: u64) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(run_id.len());
    for byte in run_id.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[(byte >> 4) as usize]));
            encoded.push(char::from(HEX[(byte & 0x0f) as usize]));
        }
    }
    format!("event:{encoded}:{seq}")
}

fn canonical_index(locator: &str, prefix: &str) -> Option<usize> {
    let index = locator.strip_prefix(prefix)?;
    if index.is_empty()
        || !index.bytes().all(|byte| byte.is_ascii_digit())
        || (index.len() > 1 && index.starts_with('0'))
    {
        return None;
    }
    index.parse().ok()
}

fn original_ref_input<'a>(
    reference: &PackageRef,
    inputs_by_id: &BTreeMap<&str, &'a InventoryInput>,
    object_map: &BTreeMap<&str, &'a [u8]>,
    bundles: &BTreeMap<String, ParsedBundle>,
) -> Result<(&'a InventoryInput, &'a [u8]), ResolveRefError> {
    let input = inputs_by_id
        .get(reference.input_id.as_str())
        .copied()
        .ok_or(ResolveRefError::Locator)?;
    if input.sha256 != reference.sha256 {
        return Err(ResolveRefError::Locator);
    }
    let object_path = format!("objects/{}", input.sha256);
    let bytes = object_map
        .get(object_path.as_str())
        .copied()
        .ok_or(ResolveRefError::Locator)?;

    let present = match input.format.as_str() {
        "assay-bundle-v1" => bundles
            .get(&input.id)
            .ok_or(ResolveRefError::InputShape)?
            .events
            .iter()
            .any(|event| {
                reference.locator == canonical_event_locator(&event.typed.run_id, event.typed.seq)
            }),
        "proxy-decision-ndjson" | "runner-kernel-ndjson-v0" | "runner-kernel-ndjson-v1" => {
            let text = std::str::from_utf8(bytes).map_err(|_| ResolveRefError::InputShape)?;
            let mut offset = 0usize;
            let mut found = false;
            for (ordinal, raw_line) in text.split_inclusive('\n').enumerate() {
                let line = if kernel_event_schema_for_format(&input.format).is_some() {
                    kernel_record_line(raw_line)?
                } else {
                    let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
                    line.strip_suffix('\r').unwrap_or(line)
                };
                if reference.locator == format!("line:{ordinal}:{offset}:{}", line.len()) {
                    found = true;
                }
                offset += raw_line.len();
            }
            found
        }
        "inspector-protocol-793d103" => {
            let Some(index) = canonical_index(&reference.locator, "json:/") else {
                return Err(ResolveRefError::Locator);
            };
            parse_strict_json(bytes)?
                .as_array()
                .is_some_and(|entries| index < entries.len())
        }
        "incident-context-v1" => {
            let Some(index) = canonical_index(&reference.locator, "json:/entries/") else {
                return Err(ResolveRefError::Locator);
            };
            let document: IncidentContextDocument =
                serde_json::from_slice(bytes).map_err(|_| ResolveRefError::InputShape)?;
            document.schema == "assay.incident.context.v1" && index < document.entries.len()
        }
        "incident-results-v1" => {
            let Some(index) = canonical_index(&reference.locator, "json:/results/") else {
                return Err(ResolveRefError::Locator);
            };
            let document: IncidentResultsDocument =
                serde_json::from_slice(bytes).map_err(|_| ResolveRefError::InputShape)?;
            document.schema == "assay.incident.results.v1" && index < document.results.len()
        }
        "assay-health-json" | "assay-coverage-json" | "dsse-attestation" | "attestation-report" => {
            reference.locator == "json:"
        }
        "policy-bytes" | "ed25519-public-key-pem" => {
            reference.locator == format!("bytes:0:{}", bytes.len())
        }
        _ => false,
    };
    if !present {
        return Err(ResolveRefError::Locator);
    }
    Ok((input, bytes))
}

fn validate_context_source_refs(
    inventory: &InventoryDocument,
    inputs_by_id: &BTreeMap<&str, &InventoryInput>,
    object_map: &BTreeMap<&str, &[u8]>,
    bundles: &BTreeMap<String, ParsedBundle>,
) -> Result<(), ResolveRefError> {
    for input in &inventory.inputs {
        if input.format != "incident-context-v1" {
            continue;
        }
        let object_path = format!("objects/{}", input.sha256);
        let bytes = object_map
            .get(object_path.as_str())
            .copied()
            .ok_or(ResolveRefError::Locator)?;
        let document: IncidentContextDocument =
            serde_json::from_slice(bytes).map_err(|_| ResolveRefError::InputShape)?;
        if document.schema != "assay.incident.context.v1" {
            return Err(ResolveRefError::InputShape);
        }
        for entry in &document.entries {
            for reference in &entry.source_refs {
                let (target, _) = original_ref_input(reference, inputs_by_id, object_map, bundles)?;
                if matches!(
                    target.format.as_str(),
                    "incident-context-v1" | "incident-results-v1"
                ) {
                    return Err(ResolveRefError::Locator);
                }
            }
        }
    }
    for (kind, disclosure) in &inventory.context {
        for reference in &disclosure.refs {
            let (target, bytes) = original_ref_input(reference, inputs_by_id, object_map, bundles)?;
            if target.format == "incident-context-v1" {
                let index = canonical_index(&reference.locator, "json:/entries/")
                    .ok_or(ResolveRefError::Locator)?;
                let document: IncidentContextDocument =
                    serde_json::from_slice(bytes).map_err(|_| ResolveRefError::InputShape)?;
                if document.entries[index].kind != *kind {
                    return Err(ResolveRefError::Locator);
                }
            }
        }
    }
    Ok(())
}

fn resolve_applicability_ref(
    reference: &PackageRef,
    surface: &str,
    inputs_by_id: &BTreeMap<&str, &InventoryInput>,
    object_map: &BTreeMap<&str, &[u8]>,
    bundles: &BTreeMap<String, ParsedBundle>,
    context: &ContextInput,
) -> Result<(), ResolveRefError> {
    let (input, bytes) = original_ref_input(reference, inputs_by_id, object_map, bundles)?;

    match input.format.as_str() {
        "assay-health-json" => {
            if reference.locator != "json:" {
                return Err(ResolveRefError::Locator);
            }
            let value = parse_bounded_context_json(bytes, context)?;
            match value.get("schema").and_then(|schema| schema.as_str()) {
                Some(OBSERVATION_HEALTH_SCHEMA) => {
                    let health: ObservationHealth =
                        serde_json::from_value(value).map_err(|_| ResolveRefError::InputShape)?;
                    health.validate().map_err(|_| ResolveRefError::InputShape)?;
                }
                Some("assay.enforcement_health.v0") => validate_enforcement_health_v0(value)?,
                Some("assay.enforcement_health.v1") => validate_enforcement_health_v1(value)?,
                _ => return Err(ResolveRefError::Locator),
            }
            if !matches!(surface, "filesystem" | "network" | "process") {
                return Err(ResolveRefError::Locator);
            }
            Ok(())
        }
        "assay-coverage-json" => {
            if reference.locator != "json:" {
                return Err(ResolveRefError::Locator);
            }
            let descriptor: CoverageDescriptor =
                serde_json::from_value(parse_bounded_context_json(bytes, context)?)
                    .map_err(|_| ResolveRefError::InputShape)?;
            if descriptor.schema != COVERAGE_DESCRIPTOR_SCHEMA {
                return Err(ResolveRefError::InputShape);
            }
            let expected_surface = match descriptor.dimension {
                EffectDimension::Filesystem => "filesystem",
                EffectDimension::Network => "network",
                EffectDimension::Process => "process",
            };
            if surface != expected_surface {
                return Err(ResolveRefError::Locator);
            }
            Ok(())
        }
        "incident-context-v1" => {
            let idx = canonical_index(&reference.locator, "json:/entries/")
                .ok_or(ResolveRefError::Locator)?;
            let value = parse_bounded_context_json(bytes, context)?;
            if value.get("schema").and_then(|schema| schema.as_str())
                != Some("assay.incident.context.v1")
            {
                return Err(ResolveRefError::InputShape);
            }
            let entries = value
                .get("entries")
                .and_then(serde_json::Value::as_array)
                .ok_or(ResolveRefError::InputShape)?;
            if entries.len() as u64 > ContextLimits::HARD.records {
                return Err(ResolveRefError::ResourceHard);
            }
            if entries.len() as u64 > context.limits.records {
                return Err(ResolveRefError::ResourceSelected);
            }
            let ref_count = entries.iter().try_fold(0u64, |count, entry| {
                let refs = entry
                    .get("source_refs")
                    .and_then(serde_json::Value::as_array)?;
                count.checked_add(refs.len() as u64)
            });
            let Some(ref_count) = ref_count else {
                return Err(ResolveRefError::InputShape);
            };
            if ref_count > ContextLimits::HARD.references {
                return Err(ResolveRefError::ResourceHard);
            }
            if ref_count > context.limits.references {
                return Err(ResolveRefError::ResourceSelected);
            }
            let document: IncidentContextDocument =
                serde_json::from_value(value).map_err(|_| ResolveRefError::InputShape)?;
            let entry = document.entries.get(idx).ok_or(ResolveRefError::Locator)?;
            const BASIS_KINDS: &[&str] = &[
                "clock",
                "correlation",
                "context_provenance",
                "denominator",
                "missing_evidence",
                "resource_effects",
                "retention",
                "windows",
            ];
            if !BASIS_KINDS.contains(&entry.kind.as_str()) || entry.value.trim().is_empty() {
                return Err(ResolveRefError::Locator);
            }
            let _ = &entry.source_refs;
            Ok(())
        }
        "assay-bundle-v1" => {
            let event = bundles
                .get(&input.id)
                .ok_or(ResolveRefError::InputShape)?
                .events
                .iter()
                .find(|event| {
                    reference.locator
                        == canonical_event_locator(&event.typed.run_id, event.typed.seq)
                })
                .ok_or(ResolveRefError::Locator)?;
            const CONTEXT_EVENTS: &[&str] = &[
                "assay.profile.started",
                "assay.profile.finished",
                "assay.sandbox.summary",
                "assay.sandbox.degraded",
                "assay.coding_agent.evidence_pack.v0",
            ];
            if !CONTEXT_EVENTS.contains(&event.typed.type_.as_str()) {
                return Err(ResolveRefError::Locator);
            }
            let _ = (surface, bytes, context);
            Ok(())
        }
        _ => Err(ResolveRefError::Locator),
    }
}

fn coverage_state_ceiling(state: CodingAgentCoverageState) -> ObservationCeiling {
    match state {
        CodingAgentCoverageState::Observed => ObservationCeiling::Adequate,
        CodingAgentCoverageState::Partial | CodingAgentCoverageState::SelfReported => {
            ObservationCeiling::Limited
        }
        CodingAgentCoverageState::Absent | CodingAgentCoverageState::Unavailable => {
            ObservationCeiling::Unknown
        }
    }
}

fn retained_context_inventory(
    inventory: &InventoryDocument,
    object_map: &BTreeMap<&str, &[u8]>,
    bundles: &BTreeMap<String, ParsedBundle>,
    context: &ContextInput,
) -> Result<RetainedContextInventory, ResolveRefError> {
    let mut retained = RetainedContextInventory::default();
    for input in &inventory.inputs {
        let object_path = format!("objects/{}", input.sha256);
        let bytes = object_map
            .get(object_path.as_str())
            .ok_or(ResolveRefError::Locator)?;
        match input.format.as_str() {
            "assay-coverage-json" => {
                let descriptor: CoverageDescriptor =
                    serde_json::from_value(parse_bounded_context_json(bytes, context)?)
                        .map_err(|_| ResolveRefError::InputShape)?;
                if descriptor.schema != COVERAGE_DESCRIPTOR_SCHEMA {
                    return Err(ResolveRefError::InputShape);
                }
                let surface = match descriptor.dimension {
                    EffectDimension::Filesystem => "filesystem",
                    EffectDimension::Network => "network",
                    EffectDimension::Process => "process",
                };
                let ceiling = if descriptor.completeness == CoverageCompleteness::Full
                    && descriptor.known_blind_spots.is_empty()
                {
                    ObservationCeiling::Adequate
                } else {
                    ObservationCeiling::Limited
                };
                retained.reduce(surface, ceiling);
            }
            "assay-health-json" => {
                let value = parse_bounded_context_json(bytes, context)?;
                match value.get("schema").and_then(|schema| schema.as_str()) {
                    Some(OBSERVATION_HEALTH_SCHEMA) => {
                        let health: ObservationHealth = serde_json::from_value(value)
                            .map_err(|_| ResolveRefError::InputShape)?;
                        health.validate().map_err(|_| ResolveRefError::InputShape)?;
                        let ceiling = if health.capture_origin == CaptureOrigin::Unsupported
                            || health.kernel_layer == KernelLayerStatus::Absent
                        {
                            ObservationCeiling::Unknown
                        } else if health.kernel_layer == KernelLayerStatus::PartialRingbufDrops {
                            ObservationCeiling::Limited
                        } else {
                            ObservationCeiling::Adequate
                        };
                        for surface in ["filesystem", "network", "process"] {
                            retained.reduce(surface, ceiling);
                        }
                        if health.cgroup_correlation == CgroupCorrelationStatus::Partial {
                            retained.correspondence_ambiguous = true;
                        }
                    }
                    Some("assay.enforcement_health.v0") => {
                        let failed = value.get("network_enforcement").and_then(|v| v.as_str())
                            == Some("failed");
                        validate_enforcement_health_v0(value)?;
                        retained.enforcement_degraded |= failed;
                    }
                    Some("assay.enforcement_health.v1") => {
                        let failed = value.get("status").and_then(|v| v.as_str()) == Some("failed");
                        validate_enforcement_health_v1(value)?;
                        retained.enforcement_degraded |= failed;
                    }
                    _ => return Err(ResolveRefError::UnknownSchema),
                }
            }
            "assay-bundle-v1" => {
                let bundle = bundles.get(&input.id).ok_or(ResolveRefError::InputShape)?;
                for event in &bundle.events {
                    match event.typed.type_.as_str() {
                        "assay.coding_agent.evidence_pack.v0" => {
                            let payload: CodingAgentEvidencePayload =
                                serde_json::from_value(event.typed.payload.clone())
                                    .map_err(|_| ResolveRefError::InputShape)?;
                            for (surface, state) in [
                                ("filesystem", payload.coverage.files),
                                ("process", payload.coverage.commands),
                                ("network", payload.coverage.network),
                                ("tool_call", payload.coverage.mcp_tools),
                            ] {
                                retained.reduce(surface, coverage_state_ceiling(state));
                            }
                        }
                        "assay.sandbox.degraded" => retained.enforcement_degraded = true,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for input in &inventory.inputs {
        if kernel_event_schema_for_format(&input.format).is_some() {
            for surface in &input.surfaces {
                let static_surface = match surface.as_str() {
                    "filesystem" => Some("filesystem"),
                    "network" => Some("network"),
                    "process" => Some("process"),
                    _ => None,
                };
                if let Some(static_surface) = static_surface {
                    // This raw format does not bind a coverage descriptor to its records.
                    retained.reduce(static_surface, ObservationCeiling::Limited);
                }
            }
        }
    }
    Ok(retained)
}

fn inspector_id_is_valid(value: &serde_json::Value, allow_null: bool) -> bool {
    match value {
        serde_json::Value::String(value) => !value.is_empty(),
        serde_json::Value::Number(value) => value.is_i64() || value.is_u64(),
        serde_json::Value::Null => allow_null,
        _ => false,
    }
}

fn inspector_message_is_valid(message: &serde_json::Value, direction: &str) -> bool {
    let Some(object) = message.as_object() else {
        return false;
    };
    if object.get("jsonrpc").and_then(serde_json::Value::as_str) != Some("2.0")
        || object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "jsonrpc" | "id" | "method" | "params" | "result" | "error"
            )
        })
    {
        return false;
    }
    let method = object.get("method");
    let result = object.get("result");
    let error = object.get("error");
    match direction {
        "request" | "notification" => {
            if result.is_some()
                || error.is_some()
                || method
                    .and_then(serde_json::Value::as_str)
                    .is_none_or(|method| method.trim().is_empty())
                || object
                    .get("params")
                    .is_some_and(|params| !params.is_object() && !params.is_array())
            {
                return false;
            }
            match (direction, object.get("id")) {
                ("request", Some(id)) => inspector_id_is_valid(id, false),
                ("notification", None) => true,
                _ => false,
            }
        }
        "response" => {
            method.is_none()
                && object
                    .get("id")
                    .is_some_and(|id| inspector_id_is_valid(id, true))
                && (result.is_some() ^ error.is_some())
        }
        _ => false,
    }
}

fn inspector_entry_is_action(entry: &serde_json::Value) -> Result<bool, ResolveRefError> {
    let object = entry.as_object().ok_or(ResolveRefError::InputShape)?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "id" | "timestamp"
                | "direction"
                | "message"
                | "origin"
                | "response"
                | "duration"
                | "clientError"
        )
    }) || object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .is_none_or(|id| id.trim().is_empty())
    {
        return Err(ResolveRefError::InputShape);
    }
    let timestamp = object
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
        .filter(|timestamp| timestamp.offset().local_minus_utc() == 0)
        .ok_or(ResolveRefError::InputShape)?;
    let _ = timestamp;
    let direction = object
        .get("direction")
        .and_then(serde_json::Value::as_str)
        .ok_or(ResolveRefError::InputShape)?;
    if !matches!(direction, "request" | "response" | "notification")
        || object
            .get("origin")
            .is_some_and(|origin| !matches!(origin.as_str(), Some("client" | "server")))
        || object
            .get("duration")
            .is_some_and(|duration| duration.as_f64().is_none_or(|value| value < 0.0))
        || object
            .get("clientError")
            .is_some_and(|error| !error.is_string())
    {
        return Err(ResolveRefError::InputShape);
    }
    let message = object.get("message").ok_or(ResolveRefError::InputShape)?;
    if !inspector_message_is_valid(message, direction) {
        return Err(ResolveRefError::InputShape);
    }
    if let Some(response) = object.get("response") {
        if !inspector_message_is_valid(response, "response")
            || response.get("id") != message.get("id")
        {
            return Err(ResolveRefError::InputShape);
        }
    }
    let is_action = direction == "request"
        && object.get("origin").and_then(serde_json::Value::as_str) == Some("client")
        && message.get("method").and_then(serde_json::Value::as_str) == Some("tools/call");
    if is_action {
        let Some(params) = message.get("params").and_then(serde_json::Value::as_object) else {
            return Err(ResolveRefError::InputShape);
        };
        if params
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|name| name.trim().is_empty())
            || params
                .get("arguments")
                .is_some_and(|arguments| !arguments.is_object())
        {
            return Err(ResolveRefError::InputShape);
        }
    }
    Ok(is_action)
}

fn canonical_action_payload_is_valid(event: &ParsedCanonicalEvent) -> bool {
    let data = &event.typed.payload;
    match event.typed.type_.as_str() {
        "assay.tool.decision" => {
            crate::incident_package::decision_wire_v1::validate(&event.original).is_ok()
        }
        "assay.fs.access" | "assay.net.connect" | "assay.process.exec" => {
            let identity_field = match event.typed.type_.as_str() {
                "assay.fs.access" => "file",
                "assay.net.connect" => "host",
                _ => "cmd",
            };
            event
                .typed
                .subject
                .as_deref()
                .is_some_and(|subject| !subject.is_empty())
                && data.as_object().is_some_and(|object| {
                    object
                        .get("hits")
                        .and_then(serde_json::Value::as_u64)
                        .is_some()
                        && object
                            .get(identity_field)
                            .is_none_or(serde_json::Value::is_string)
                })
        }
        "assay.sandbox.fs" => data.as_object().is_some_and(|object| {
            ["op", "path", "backend"]
                .iter()
                .all(|field| object.get(*field).is_some_and(serde_json::Value::is_string))
                && object
                    .get("hits")
                    .and_then(serde_json::Value::as_u64)
                    .is_some()
        }),
        "assay.sandbox.exec" => data.as_object().is_some_and(|object| {
            object
                .get("argv0")
                .is_some_and(serde_json::Value::is_string)
                && object
                    .get("hits")
                    .and_then(serde_json::Value::as_u64)
                    .is_some()
        }),
        _ => false,
    }
}

/// The kernel event schema each runner kernel source format carries. A format admits
/// only its own schema, so one format token never stands for two label mappings.
fn kernel_event_schema_for_format(format: &str) -> Option<&'static str> {
    match format {
        "runner-kernel-ndjson-v0" => Some(KERNEL_EVENT_SCHEMA),
        "runner-kernel-ndjson-v1" => Some(KERNEL_EVENT_SCHEMA_V1),
        _ => None,
    }
}

fn validate_kernel_event_value(
    value: &serde_json::Value,
    format: &str,
) -> Result<&'static str, ResolveRefError> {
    let obj = value.as_object().ok_or(ResolveRefError::InputShape)?;
    let schema = obj
        .get("schema")
        .and_then(serde_json::Value::as_str)
        .ok_or(ResolveRefError::InputShape)?;
    if Some(schema) != kernel_event_schema_for_format(format) {
        return Err(ResolveRefError::UnknownSchema);
    }
    if obj.keys().any(|key| !is_known_kernel_event_key(key))
        || REQUIRED_KERNEL_EVENT_KEYS
            .iter()
            .any(|required| !obj.contains_key(*required))
        || obj
            .iter()
            .any(|(key, member)| key != "value" && member.is_null())
        || obj
            .get("operation_flags")
            .is_some_and(|flags| flags.as_array().is_none_or(Vec::is_empty))
    {
        return Err(ResolveRefError::InputShape);
    }
    let event: KernelLayerEvent =
        serde_json::from_value(value.clone()).map_err(|_| ResolveRefError::InputShape)?;
    let dimension = event.validate_emitted().map_err(|err| match err {
        KernelEventValidationError::UnknownSchema => ResolveRefError::UnknownSchema,
        KernelEventValidationError::InputShape => ResolveRefError::InputShape,
        _ => ResolveRefError::InputShape,
    })?;
    Ok(match dimension {
        EffectDimension::Filesystem => "filesystem",
        EffectDimension::Network => "network",
        EffectDimension::Process => "process",
    })
}

fn admitted_action_records(
    inventory: &InventoryDocument,
    object_map: &BTreeMap<&str, &[u8]>,
    bundles: &BTreeMap<String, ParsedBundle>,
) -> Result<Vec<AdmittedActionRecord>, ResolveRefError> {
    let mut records = Vec::new();
    for input in &inventory.inputs {
        let object_path = format!("objects/{}", input.sha256);
        let bytes = object_map
            .get(object_path.as_str())
            .ok_or(ResolveRefError::Locator)?;
        match input.format.as_str() {
            "assay-bundle-v1" => {
                let bundle = bundles.get(&input.id).ok_or(ResolveRefError::InputShape)?;
                for event in &bundle.events {
                    let surface = match event.typed.type_.as_str() {
                        "assay.tool.decision" => Some("tool_call"),
                        "assay.fs.access" | "assay.sandbox.fs" => Some("filesystem"),
                        "assay.net.connect" => Some("network"),
                        "assay.process.exec" | "assay.sandbox.exec" => Some("process"),
                        "assay.profile.started"
                        | "assay.profile.finished"
                        | "assay.sandbox.summary"
                        | "assay.sandbox.degraded"
                        | "assay.coding_agent.evidence_pack.v0" => None,
                        _ => return Err(ResolveRefError::InputShape),
                    };
                    let Some(surface) = surface else { continue };
                    let value = serde_json::json!({
                        "event": event.original,
                        "events_sha256": bundle.events_sha256,
                    });
                    records.push(AdmittedActionRecord {
                        reference: PackageRef {
                            input_id: input.id.clone(),
                            sha256: input.sha256.clone(),
                            locator: canonical_event_locator(&event.typed.run_id, event.typed.seq),
                        },
                        surface,
                        operation: input.format.clone(),
                        value,
                        timestamp: Some(event.typed.time.fixed_offset()),
                        examination_succeeded: canonical_action_payload_is_valid(event),
                    });
                }
            }
            "proxy-decision-ndjson" => {
                let text = std::str::from_utf8(bytes).map_err(|_| ResolveRefError::InputShape)?;
                let mut offset = 0usize;
                for (ordinal, raw_line) in text.split_inclusive('\n').enumerate() {
                    let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
                    let line = line.strip_suffix('\r').unwrap_or(line);
                    if line.trim().is_empty() {
                        return Err(ResolveRefError::InputShape);
                    }
                    validate_json_strict(line).map_err(|_| ResolveRefError::InputShape)?;
                    let value: serde_json::Value =
                        serde_json::from_str(line).map_err(|_| ResolveRefError::InputShape)?;
                    if value.get("specversion").and_then(|v| v.as_str()) != Some("1.0")
                        || value.get("type").and_then(|v| v.as_str()) != Some("assay.tool.decision")
                    {
                        return Err(ResolveRefError::InputShape);
                    }
                    crate::incident_package::decision_wire_v1::validate(&value)
                        .map_err(|_| ResolveRefError::InputShape)?;
                    records.push(AdmittedActionRecord {
                        reference: PackageRef {
                            input_id: input.id.clone(),
                            sha256: input.sha256.clone(),
                            locator: format!("line:{ordinal}:{offset}:{}", line.len()),
                        },
                        surface: "tool_call",
                        operation: input.format.clone(),
                        timestamp: value
                            .get("time")
                            .and_then(|v| v.as_str())
                            .and_then(|v| DateTime::parse_from_rfc3339(v).ok()),
                        value,
                        examination_succeeded: true,
                    });
                    offset += raw_line.len();
                }
            }
            "runner-kernel-ndjson-v0" | "runner-kernel-ndjson-v1" => {
                let text = std::str::from_utf8(bytes).map_err(|_| ResolveRefError::InputShape)?;
                let mut offset = 0usize;
                let mut observed_surfaces = BTreeSet::new();
                for (ordinal, raw_line) in text.split_inclusive('\n').enumerate() {
                    let line = kernel_record_line(raw_line)?;
                    validate_json_strict(line).map_err(|_| ResolveRefError::InputShape)?;
                    let value: serde_json::Value =
                        serde_json::from_str(line).map_err(|_| ResolveRefError::InputShape)?;
                    let surface = validate_kernel_event_value(&value, &input.format)?;
                    observed_surfaces.insert(surface);
                    records.push(AdmittedActionRecord {
                        reference: PackageRef {
                            input_id: input.id.clone(),
                            sha256: input.sha256.clone(),
                            locator: format!("line:{ordinal}:{offset}:{}", line.len()),
                        },
                        surface,
                        operation: input.format.clone(),
                        timestamp: None,
                        value,
                        examination_succeeded: true,
                    });
                    offset += raw_line.len();
                }
                if !text.is_empty() {
                    let expected_surfaces: Vec<&str> = ["filesystem", "network", "process"]
                        .into_iter()
                        .filter(|surface| observed_surfaces.contains(surface))
                        .collect();
                    if input.surfaces != expected_surfaces {
                        return Err(ResolveRefError::Mapping);
                    }
                }
            }
            "inspector-protocol-793d103" => {
                let value = parse_strict_json(bytes)?;
                let entries = value.as_array().ok_or(ResolveRefError::InputShape)?;
                for (index, entry) in entries.iter().enumerate() {
                    if inspector_entry_is_action(entry)? {
                        records.push(AdmittedActionRecord {
                            reference: PackageRef {
                                input_id: input.id.clone(),
                                sha256: input.sha256.clone(),
                                locator: format!("json:/{index}"),
                            },
                            surface: "transcript_join",
                            operation: input.format.clone(),
                            timestamp: entry
                                .get("timestamp")
                                .and_then(|v| v.as_str())
                                .and_then(|v| DateTime::parse_from_rfc3339(v).ok()),
                            value: entry.clone(),
                            examination_succeeded: true,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(records)
}

fn derive_surface_result(
    source_inputs: &[String],
    applicability: Option<&ContextApplicability>,
    retained: &RetainedContextInventory,
    surface: &str,
) -> DerivedSurfaceResult {
    let Some(applicability) = applicability.filter(|_| !source_inputs.is_empty()) else {
        return DerivedSurfaceResult {
            observation: "unknown".to_string(),
            basis_refs: Vec::new(),
            window: None,
        };
    };
    let mut basis_refs = applicability.input_refs.clone();
    sort_dedup_refs(&mut basis_refs);
    let window = applicability
        .window
        .as_ref()
        .map(|window| AssessmentWindow {
            start: window.start.clone(),
            end: window.end.clone(),
            clock_basis: window.clock_basis.clone(),
        });
    let zero_width = applicability.window.as_ref().is_some_and(|window| {
        match (
            DateTime::parse_from_rfc3339(&window.start),
            DateTime::parse_from_rfc3339(&window.end),
        ) {
            (Ok(start), Ok(end)) => start == end,
            _ => window.start == window.end,
        }
    });
    let mut observation = if basis_refs.is_empty()
        || zero_width
        || applicability.coverage == "unknown"
        || applicability.retention == "unknown"
        || applicability.correspondence == "unknown"
    {
        "unknown"
    } else if applicability.coverage == "adequate"
        && applicability.retention == "adequate"
        && applicability.correspondence == "unique"
    {
        "adequate"
    } else {
        "limited"
    };
    let mut retained_ceiling = retained.ceiling(surface);
    if retained.correspondence_ambiguous
        && matches!(surface, "filesystem" | "network" | "process")
        && applicability.correspondence == "unique"
    {
        retained_ceiling = retained_ceiling.max(ObservationCeiling::Limited);
    }
    observation = match (observation, retained_ceiling) {
        ("unknown", _) | (_, ObservationCeiling::Unknown) => "unknown",
        ("limited", _) | (_, ObservationCeiling::Limited) => "limited",
        _ => "adequate",
    };
    DerivedSurfaceResult {
        observation: observation.to_string(),
        basis_refs,
        window,
    }
}

fn fresh_examination_result(record: &AdmittedActionRecord) -> ExaminationResult {
    ExaminationResult {
        operation: record.operation.clone(),
        source: record.reference.clone(),
        value: record.value.clone(),
    }
}

fn examination_result_bytes(result: &ExaminationResult) -> Result<Vec<u8>, ()> {
    assay_canonical::jcs::to_vec(result).map_err(|_| ())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DerivedJoin {
    result: &'static str,
    join_grade: &'static str,
    join_value: Option<String>,
    unique_within_scope: bool,
    fallback_used: bool,
}

fn action_artifact_role(record: &AdmittedActionRecord) -> &'static str {
    if record.operation == "assay-bundle-v1" {
        "measured_run_archive"
    } else {
        "external_receipt"
    }
}

fn action_pointer<'a>(
    record: &'a AdmittedActionRecord,
    pointer: &str,
) -> Option<&'a serde_json::Value> {
    if record.operation == "assay-bundle-v1" {
        let nested = if pointer == "/assayrunid" {
            "/event/assayrunid".to_string()
        } else if let Some(rest) = pointer.strip_prefix("/data") {
            format!("/event/data{rest}")
        } else {
            return None;
        };
        record.value.pointer(&nested)
    } else {
        record.value.pointer(pointer)
    }
}

fn scalar_join_value(value: &serde_json::Value) -> Option<Option<String>> {
    match value {
        serde_json::Value::String(value) => Some(Some(value.clone())),
        serde_json::Value::Number(value) => Some(Some(value.to_string())),
        serde_json::Value::Bool(value) => Some(Some(value.to_string())),
        serde_json::Value::Null => Some(None),
        _ => None,
    }
}

fn strong_key_value<'a>(
    record: &'a AdmittedActionRecord,
    pointer: &str,
    contract: &JoinContract,
) -> Option<&'a str> {
    let eligible_path = match (contract.join_key.as_str(), contract.scope.as_str()) {
        ("tool_call_id", "tool_call") => match record.operation.as_str() {
            "assay-bundle-v1" => record.surface == "tool_call" && pointer == "/data/tool_call_id",
            "proxy-decision-ndjson" => pointer == "/data/tool_call_id",
            "inspector-protocol-793d103" => {
                pointer == "/message/params/arguments/_meta/tool_call_id"
            }
            _ => false,
        },
        ("run_id", "run") => record.operation == "assay-bundle-v1" && pointer == "/assayrunid",
        _ => false,
    };
    if !eligible_path {
        return None;
    }
    let value = action_pointer(record, pointer)?.as_str()?;
    if value.is_empty()
        || (record.operation == "proxy-decision-ndjson"
            && (value.starts_with("req_") || value.starts_with("gen_")))
    {
        return None;
    }
    Some(value)
}

fn within_window(record: &AdmittedActionRecord, window: Option<&AssessmentWindow>) -> bool {
    let Some(window) = window else { return false };
    let (Ok(start), Ok(end), Some(timestamp)) = (
        DateTime::parse_from_rfc3339(&window.start),
        DateTime::parse_from_rfc3339(&window.end),
        record.timestamp,
    ) else {
        return false;
    };
    timestamp >= start && timestamp <= end && start < end
}

fn derive_join(
    join: &AssessmentJoin,
    records: &[AdmittedActionRecord],
    surfaces: &BTreeMap<String, DerivedSurfaceResult>,
    retained: &RetainedContextInventory,
) -> Option<DerivedJoin> {
    let left = records
        .iter()
        .find(|record| record.reference == join.left)?;
    let surface = surfaces.get(&join.proposition.surface)?;
    let left_value = action_pointer(left, &join.proposition.left_pointer)?;
    let left_scalar = scalar_join_value(left_value)?;
    let left_unique = records
        .iter()
        .filter(|record| {
            within_window(record, surface.window.as_ref())
                && action_pointer(record, &join.proposition.left_pointer) == Some(left_value)
        })
        .count()
        == 1;
    let strong_left = left_scalar.as_deref().is_some_and(|value| {
        strong_key_value(left, &join.proposition.left_pointer, &join.contract) == Some(value)
    }) && within_window(left, surface.window.as_ref())
        && left_unique
        && !retained.correspondence_ambiguous;

    match join.proposition.kind.as_str() {
        "equal_retained_value" => {
            if !join.proposition.expected.is_null() {
                return None;
            }
            let right_ref = join.right.as_ref()?;
            let right = records
                .iter()
                .find(|record| &record.reference == right_ref)?;
            let right_value = action_pointer(right, &join.proposition.right_pointer)?;
            let right_scalar = scalar_join_value(right_value)?;
            let right_unique = records
                .iter()
                .filter(|record| {
                    record.surface == join.proposition.surface
                        && within_window(record, surface.window.as_ref())
                        && right_scalar.as_deref().is_some_and(|value| {
                            strong_key_value(
                                record,
                                &join.proposition.right_pointer,
                                &join.contract,
                            ) == Some(value)
                        })
                })
                .count()
                == 1;
            let strong = strong_left
                && within_window(right, surface.window.as_ref())
                && right_unique
                && right_scalar.as_deref().is_some_and(|value| {
                    strong_key_value(right, &join.proposition.right_pointer, &join.contract)
                        == Some(value)
                });
            let (grade, result) = if strong {
                (
                    "strong",
                    if left_value == right_value {
                        "confirmed"
                    } else {
                        "contradicted"
                    },
                )
            } else {
                (
                    if join.contract.scope == "diagnostic" {
                        "diagnostic"
                    } else {
                        "weak"
                    },
                    "unobservable",
                )
            };
            Some(DerivedJoin {
                result,
                join_grade: grade,
                join_value: left_scalar,
                unique_within_scope: strong_left,
                fallback_used: false,
            })
        }
        "counterpart_present" => {
            if &join.proposition.expected != left_value {
                return None;
            }
            let expected = left_scalar?;
            let matches = records
                .iter()
                .filter(|record| {
                    record.surface == join.proposition.surface
                        && within_window(record, surface.window.as_ref())
                        && strong_key_value(record, &join.proposition.right_pointer, &join.contract)
                            == Some(expected.as_str())
                })
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [right] if strong_left => {
                    if join.right.as_ref() != Some(&right.reference)
                        || strong_key_value(right, &join.proposition.right_pointer, &join.contract)
                            != Some(expected.as_str())
                    {
                        return None;
                    }
                    Some(DerivedJoin {
                        result: "confirmed",
                        join_grade: "strong",
                        join_value: Some(expected),
                        unique_within_scope: true,
                        fallback_used: false,
                    })
                }
                [] => {
                    if join.right.is_some() {
                        return None;
                    }
                    let contradiction = strong_left
                        && surface.observation == "adequate"
                        && surface
                            .window
                            .as_ref()
                            .is_some_and(|window| window.start != window.end);
                    Some(DerivedJoin {
                        result: if contradiction {
                            "contradicted"
                        } else {
                            "unobservable"
                        },
                        join_grade: "failed",
                        join_value: None,
                        unique_within_scope: left_unique,
                        fallback_used: false,
                    })
                }
                _ => Some(DerivedJoin {
                    result: "unobservable",
                    join_grade: "weak",
                    join_value: Some(expected),
                    unique_within_scope: false,
                    fallback_used: false,
                }),
            }
        }
        _ => None,
    }
}

fn join_matches_derivation(join: &AssessmentJoin, derived: &DerivedJoin) -> bool {
    join.result == derived.result
        && join.contract.join_grade == derived.join_grade
        && join.contract.join_value == derived.join_value
        && join.contract.unique_within_scope == derived.unique_within_scope
        && join.contract.fallback_used == derived.fallback_used
}

fn contract_ref_matches(reference: &str, record: &AdmittedActionRecord) -> bool {
    let Some((input_id, locator)) = reference.split_once('#') else {
        return false;
    };
    !input_id.is_empty()
        && !locator.is_empty()
        && !locator.contains('#')
        && record.reference.input_id == input_id
        && record.reference.locator == locator
}

fn validate_join_references(join: &AssessmentJoin, records: &[AdmittedActionRecord]) -> bool {
    let Some(left) = records.iter().find(|record| record.reference == join.left) else {
        return false;
    };
    let right = match &join.right {
        Some(reference) => match records.iter().find(|record| record.reference == *reference) {
            Some(record) => Some(record),
            None => return false,
        },
        None => None,
    };
    if join.contract.left_artifact_role != action_artifact_role(left)
        || right
            .is_some_and(|record| join.contract.right_artifact_role != action_artifact_role(record))
    {
        return false;
    }
    let mut resolved = Vec::new();
    for reference in &join.contract.evidence_refs {
        let matches = records
            .iter()
            .filter(|record| contract_ref_matches(reference, record))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return false;
        }
        resolved.push(matches[0]);
    }
    resolved
        .iter()
        .any(|record| record.reference == left.reference)
        && right.is_none_or(|right| {
            resolved
                .iter()
                .any(|record| record.reference == right.reference)
        })
}

fn cap1_disposition_name(disposition: Cap1Disposition) -> &'static str {
    match disposition {
        Cap1Disposition::NotApplicable => "not_applicable",
        Cap1Disposition::DisabledByPolicy => "disabled_by_policy",
        Cap1Disposition::UnsupportedInput => "unsupported_input",
        Cap1Disposition::ResourceExhausted => "resource_exhausted",
        Cap1Disposition::Failed => "failed",
        Cap1Disposition::Unavailable => "unavailable",
        Cap1Disposition::OutOfScope => "out_of_scope",
        Cap1Disposition::Withheld => "withheld",
    }
}

fn validate_cap1_package_association(
    cap1: &Cap1Document,
    units: &[AssessmentUnit],
    claims: &[AssessmentClaim],
) -> bool {
    if cap1.profile != "cap/1"
        || cap1.producer.is_some()
        || cap1.as_of.is_some()
        || cap1.subject.kind != "collection"
        || cap1.subject.reference != "incident-selected-inputs"
        || cap1.subject.digest.is_some()
        || cap1.integrity.statement != "accounting of selected supplied records"
    {
        return false;
    }
    for stratum in &cap1.strata {
        let surface_units = units
            .iter()
            .filter(|unit| unit.surface == stratum.id)
            .collect::<Vec<_>>();
        if stratum.population != "retained supplied records"
            || stratum.basis.kind != Cap1BasisKind::Enumeration
            || stratum.basis.catalogue_digest.is_some()
            || stratum.basis.catalogue_version.is_some()
            || stratum.basis.enumeration_method.as_deref()
                != Some("incident-v1-digest-locator-enumeration")
            || stratum.basis.note.is_some()
            || stratum.eligible != surface_units.len() as u64
            || stratum.examined
                != surface_units
                    .iter()
                    .filter(|unit| unit.disposition == "examined")
                    .count() as u64
            || stratum.unexamined.len()
                != surface_units
                    .iter()
                    .filter(|unit| unit.disposition != "examined")
                    .count()
            || stratum.supports.as_ref().is_none_or(|supports| {
                supports.iter().map(String::as_str).collect::<Vec<_>>()
                    != ["positive_presence", "bounded_negative", "exhaustive_set"]
            })
        {
            return false;
        }
        for unexamined in &stratum.unexamined {
            let Some(unit) = surface_units.iter().find(|unit| unit.id == unexamined.unit) else {
                return false;
            };
            if unit.disposition != cap1_disposition_name(unexamined.disposition)
                || unexamined.detail.is_some()
                || unit.withheld_digest != unexamined.withheld_digest
            {
                return false;
            }
        }
    }

    let mut assertion_counts = BTreeMap::<&str, usize>::new();
    for assertion in cap1.absence_assertions.as_deref().unwrap_or_default() {
        let Some(claim) = claims.iter().find(|claim| claim.id == assertion.assertion) else {
            return false;
        };
        if claim.kind != "bounded_negative"
            || assertion.stratum != claim.surface
            || claim.stratum != claim.surface
            || assertion.qualifier.is_some()
        {
            return false;
        }
        *assertion_counts.entry(claim.id.as_str()).or_default() += 1;
    }
    assertion_counts.values().all(|count| *count == 1)
        && cap1.integrity.uncapped_verdict.is_none()
        && cap1.integrity.unaccounted.is_none()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentDocument {
    schema: String,
    verification_context_sha256: String,
    units: Vec<AssessmentUnit>,
    surfaces: Vec<SurfaceResult>,
    cap1: Option<serde_json::Value>,
    joins: Vec<AssessmentJoin>,
    claims: Vec<AssessmentClaim>,
    resolved_results: Vec<PackageRef>,
}

/// Verify a local regular file, enforcing the outer byte ceiling before materialization.
///
/// The first crossed stream charge determines the resource reason. Unlike a supplied byte
/// slice, unread bytes are never inspected to discover a later hard-limit violation.
/// The caller must open without blocking on nonregular inputs (e.g. O_NONBLOCK on Unix).
/// Verification starts at byte zero, regardless of the file's incoming position.
pub fn verify_incident_package_file(
    mut file: std::fs::File,
    context: &ContextInput,
) -> IncidentVerifyReport {
    if let Err(reason) = validate_context_input(context) {
        return IncidentVerifyReport::refusal(reason);
    }
    match file.metadata() {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
        Err(_) => {
            let mut report = IncidentVerifyReport::refusal(IncidentReason::IoUnavailable);
            report.outcome = IncidentOutcome::VerificationUnavailable;
            return report;
        }
    }
    if file.rewind().is_err() {
        let mut report = IncidentVerifyReport::refusal(IncidentReason::IoUnavailable);
        report.outcome = IncidentOutcome::VerificationUnavailable;
        return report;
    }
    match read_package_source(file, context) {
        Ok(bytes) => verify_incident_package(&bytes, context),
        Err(report) => *report,
    }
}

// Kept private so callers cannot skip the file-type admission check.
fn read_package_source(
    reader: impl Read,
    context: &ContextInput,
) -> Result<Vec<u8>, Box<IncidentVerifyReport>> {
    if let Err(reason) = validate_context_input(context) {
        return Err(Box::new(IncidentVerifyReport::refusal(reason)));
    }
    let mut bytes = Vec::new();
    LimitReader::new(reader, context.limits.outer_bytes, LimitKind::SourceBytes)
        .read_to_end(&mut bytes)
        .map_err(|err| {
            if LimitExceeded::from_io(&err).is_some() {
                // Stop at the first crossed charge, never drain beyond a selected ceiling.
                return Box::new(IncidentVerifyReport::refusal(
                    if context.limits.outer_bytes == ContextLimits::HARD.outer_bytes {
                        IncidentReason::ResourceHard
                    } else {
                        IncidentReason::ResourceSelected
                    },
                ));
            }
            let mut report = IncidentVerifyReport::refusal(IncidentReason::IoUnavailable);
            report.outcome = IncidentOutcome::VerificationUnavailable;
            Box::new(report)
        })?;
    Ok(bytes)
}

#[cfg(test)]
mod source_tests {
    use super::*;

    #[test]
    fn file_verification_covers_the_whole_file_regardless_of_position() {
        use std::io::{Seek, SeekFrom, Write};

        let bytes = include_bytes!("../../tests/fixtures/incident_package/valid_present_empty.tar");
        let context = ContextInput::default();
        let expected = verify_incident_package(bytes, &context);
        assert_eq!(expected.outcome, IncidentOutcome::PackageVerified);
        for offset in [1, bytes.len() as u64] {
            let mut file = tempfile::tempfile().unwrap();
            file.write_all(bytes).unwrap();
            file.seek(SeekFrom::Start(offset)).unwrap();
            let actual = verify_incident_package_file(file, &context);
            assert_eq!(actual.outcome, expected.outcome);
            assert_eq!(actual.artifact_sha256, expected.artifact_sha256);
        }

        // A valid suffix must not hide a prefix from verification or its byte budget.
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"prefix").unwrap();
        file.write_all(bytes).unwrap();
        file.seek(SeekFrom::Start(6)).unwrap();
        let mut context = context;
        context.limits.outer_bytes = bytes.len() as u64;
        assert_eq!(
            verify_incident_package_file(file, &context).reason,
            Some(IncidentReason::ResourceSelected)
        );
    }

    #[test]
    fn source_stops_at_selected_limit_plus_one_including_short_reads() {
        struct Source {
            consumed: usize,
        }
        impl Read for Source {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                // Finite witness with bytes beyond both the selected boundary and probe.
                let n = out.len().min(3).min(100 - self.consumed);
                out[..n].fill(0);
                self.consumed += n;
                Ok(n)
            }
        }
        let mut context = ContextInput::default();
        context.limits.outer_bytes = 16;
        let mut source = Source { consumed: 0 };
        let result = read_package_source(&mut source, &context);
        assert_eq!(source.consumed, 17);
        assert_eq!(
            result.unwrap_err().reason,
            Some(IncidentReason::ResourceSelected)
        );
    }

    #[test]
    fn exact_selected_source_is_retained_and_invalid_context_reads_nothing() {
        let mut context = ContextInput::default();
        context.limits.outer_bytes = 16;
        assert_eq!(
            read_package_source(Cursor::new([0; 16]), &context)
                .unwrap()
                .len(),
            16
        );
        context.schema_pin = "invalid".into();
        let mut source = Cursor::new([0; 32]);
        assert_eq!(
            read_package_source(&mut source, &context)
                .unwrap_err()
                .reason,
            Some(IncidentReason::TrustInput)
        );
        assert_eq!(source.position(), 0);
    }

    #[test]
    fn hard_source_overflow_and_io_failure_remain_distinct() {
        // Exercises the actual immutable 256 MiB guard with no oversized input allocation.
        let source = std::io::repeat(0).take(268_435_457);
        assert_eq!(
            read_package_source(source, &ContextInput::default())
                .unwrap_err()
                .reason,
            Some(IncidentReason::ResourceHard)
        );
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("test input failure"))
            }
        }
        let report = read_package_source(Broken, &ContextInput::default()).unwrap_err();
        assert_eq!(report.reason, Some(IncidentReason::IoUnavailable));
        assert_eq!(report.outcome, IncidentOutcome::VerificationUnavailable);
    }
}

fn check_inventory_budget(
    inventory: &InventoryDocument,
    context: &ContextInput,
) -> Result<(), IncidentReason> {
    let total_object_bytes = inventory.inputs.iter().try_fold(0u64, |total, input| {
        total
            .checked_add(input.bytes)
            .ok_or(IncidentReason::ResourceHard)
    })?;
    if total_object_bytes > ContextLimits::HARD.object_bytes_total {
        return Err(IncidentReason::ResourceHard);
    }
    if total_object_bytes > context.limits.object_bytes_total {
        return Err(IncidentReason::ResourceSelected);
    }
    if inventory.inputs.len() as u64 > ContextLimits::HARD.inputs {
        return Err(IncidentReason::ResourceHard);
    }
    if inventory.inputs.len() as u64 > context.limits.inputs {
        return Err(IncidentReason::ResourceSelected);
    }

    Ok(())
}

// Shared admission keeps the verifier's existing phase ordering intact.
fn admit_metadata(
    inventory_bytes: &[u8],
    assessment_bytes: &[u8],
    context: &ContextInput,
) -> Result<(InventoryDocument, AssessmentDocument, SourceBudgetState), IncidentReason> {
    if inventory_bytes.len() as u64 > ContextLimits::HARD.inventory_bytes {
        return Err(IncidentReason::ResourceHard);
    }
    if inventory_bytes.len() as u64 > context.limits.inventory_bytes {
        return Err(IncidentReason::ResourceSelected);
    }

    if assessment_bytes.len() as u64 > ContextLimits::HARD.assessment_bytes {
        return Err(IncidentReason::ResourceHard);
    }
    if assessment_bytes.len() as u64 > context.limits.assessment_bytes {
        return Err(IncidentReason::ResourceSelected);
    }

    let mut source_budget = SourceBudgetState::default();

    // Phase 2: Canonical package JSON check (UTF-8 no BOM, strict JSON, JCS+LF equivalence)
    let inv_str = match std::str::from_utf8(inventory_bytes) {
        Ok(s) => s,
        Err(_) => return Err(IncidentReason::InputShape),
    };
    if inv_str.starts_with('\u{feff}') {
        return Err(IncidentReason::InputShape);
    }
    if let Err(error) = preflight_json(
        inv_str,
        SourceJsonKind::Inventory,
        &mut source_budget,
        context,
    ) {
        return Err(incident_refusal_for_resolution(error));
    }
    let inv_val: serde_json::Value = match serde_json::from_str(inv_str) {
        Ok(v) => v,
        Err(_) => return Err(IncidentReason::InputShape),
    };
    let mut expected_inv_jcs = match assay_canonical::jcs::to_vec(&inv_val) {
        Ok(b) => b,
        Err(_) => return Err(IncidentReason::InputShape),
    };
    expected_inv_jcs.push(b'\n');
    if expected_inv_jcs != inventory_bytes {
        return Err(IncidentReason::InputShape);
    }

    let ass_str = match std::str::from_utf8(assessment_bytes) {
        Ok(s) => s,
        Err(_) => return Err(IncidentReason::InputShape),
    };
    if ass_str.starts_with('\u{feff}') {
        return Err(IncidentReason::InputShape);
    }
    if let Err(error) = preflight_json(
        ass_str,
        SourceJsonKind::Assessment,
        &mut source_budget,
        context,
    ) {
        return Err(incident_refusal_for_resolution(error));
    }
    let ass_val: serde_json::Value = match serde_json::from_str(ass_str) {
        Ok(v) => v,
        Err(_) => return Err(IncidentReason::InputShape),
    };
    let mut expected_ass_jcs = match assay_canonical::jcs::to_vec(&ass_val) {
        Ok(b) => b,
        Err(_) => return Err(IncidentReason::InputShape),
    };
    expected_ass_jcs.push(b'\n');
    if expected_ass_jcs != assessment_bytes {
        return Err(IncidentReason::InputShape);
    }

    // Closed structural grammar deserialization
    let inventory: InventoryDocument = match serde_json::from_value(inv_val) {
        Ok(d) => d,
        Err(_) => return Err(IncidentReason::InputShape),
    };
    if !required_nullable_join_members_present(&ass_val) {
        return Err(IncidentReason::InputShape);
    }
    let assessment: AssessmentDocument = match serde_json::from_value(ass_val) {
        Ok(d) => d,
        Err(_) => return Err(IncidentReason::InputShape),
    };

    // Closed vocabulary and structural checks in Phase 2
    if !valid_inventory_context(&inventory.context) {
        return Err(IncidentReason::InputShape);
    }
    for s in &assessment.surfaces {
        if !VALID_SURFACES.contains(&s.name.as_str()) {
            return Err(IncidentReason::InputShape);
        }
        if !VALID_OBSERVATIONS.contains(&s.observation.as_str()) {
            return Err(IncidentReason::InputShape);
        }
        if let Some(w) = &s.window {
            let (Some(t_start), Some(t_end)) =
                (parse_utc_timestamp(&w.start), parse_utc_timestamp(&w.end))
            else {
                return Err(IncidentReason::InputShape);
            };
            if t_start > t_end {
                return Err(IncidentReason::InputShape);
            }
        }
    }

    const VALID_DISPOSITIONS: &[&str] = &[
        "examined",
        "not_applicable",
        "disabled_by_policy",
        "unsupported_input",
        "resource_exhausted",
        "failed",
        "unavailable",
        "out_of_scope",
        "withheld",
    ];
    for u in &assessment.units {
        if !VALID_SURFACES.contains(&u.surface.as_str()) {
            return Err(IncidentReason::InputShape);
        }
        if !VALID_DISPOSITIONS.contains(&u.disposition.as_str()) {
            return Err(IncidentReason::InputShape);
        }
    }
    if !validate_join_claim_shape(&assessment) {
        return Err(IncidentReason::InputShape);
    }

    // Binding shape checks in Phase 2
    for input in &inventory.inputs {
        if input.format != "dsse-attestation" && input.format != "attestation-report" {
            if input.binding.is_some() {
                return Err(IncidentReason::InputShape);
            }
        } else if input.format == "dsse-attestation" {
            let Some(b) = &input.binding else {
                return Err(IncidentReason::InputShape);
            };
            if b.attestation_input.is_some() {
                return Err(IncidentReason::InputShape);
            }
        } else if input.format == "attestation-report" {
            let Some(b) = &input.binding else {
                return Err(IncidentReason::InputShape);
            };
            if b.attestation_input.is_none() {
                return Err(IncidentReason::InputShape);
            }
        }
    }

    // Phase 3: unknown_format, unknown_schema, mapping
    if inventory.schema != "assay.incident.inventory.v1" {
        return Err(IncidentReason::UnknownSchema);
    }
    if assessment.schema != "assay.incident.assessment.v1" {
        return Err(IncidentReason::UnknownSchema);
    }

    for input in &inventory.inputs {
        if !ADMITTED_FORMATS.contains(&input.format.as_str()) {
            return Err(IncidentReason::UnknownFormat);
        }
        match input.format.as_str() {
            "proxy-decision-ndjson" => {
                if input.surfaces != ["tool_call"] {
                    return Err(IncidentReason::Mapping);
                }
            }
            "runner-kernel-ndjson-v0" | "runner-kernel-ndjson-v1" => {
                let allowed = ["filesystem", "network", "process"];
                let mut last_idx = None;
                for surface in &input.surfaces {
                    let Some(idx) = allowed.iter().position(|candidate| candidate == surface)
                    else {
                        return Err(IncidentReason::Mapping);
                    };
                    if last_idx.is_some_and(|prev| prev >= idx) {
                        return Err(IncidentReason::Mapping);
                    }
                    last_idx = Some(idx);
                }
            }
            "inspector-protocol-793d103" => {
                if input.surfaces != ["transcript_join"] {
                    return Err(IncidentReason::Mapping);
                }
            }
            "assay-bundle-v1" => {
                let allowed = ["tool_call", "filesystem", "network", "process"];
                if input
                    .surfaces
                    .iter()
                    .any(|s| !allowed.contains(&s.as_str()))
                {
                    return Err(IncidentReason::Mapping);
                }
            }
            _ => {
                if !input.surfaces.is_empty() {
                    return Err(IncidentReason::Mapping);
                }
            }
        }
    }

    Ok((inventory, assessment, source_budget))
}

/// Bounded offline verification entry point for Incident Package v1 outer archives.
///
/// Implements the 11 ordered phases of SPEC-Incident-Package-v1 section 10,
/// composing the container reader and CAP-1 verifier.
pub fn verify_incident_package(bytes: &[u8], context: &ContextInput) -> IncidentVerifyReport {
    // Phase 1: trust_input (context validation)
    if let Err(reason) = validate_context_input(context) {
        return IncidentVerifyReport::refusal(reason);
    }

    // Outer resource guard: outer_bytes check before processing
    if bytes.len() as u64 > ContextLimits::HARD.outer_bytes {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceHard);
    }
    if bytes.len() as u64 > context.limits.outer_bytes {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceSelected);
    }

    // Phase 2: input_shape (outer container framing & member allowlist)
    let container = match read_incident_container_with_limits(bytes, &context.limits) {
        Ok(c) => c,
        Err(report) => return report,
    };

    let (inventory, assessment, mut source_budget) = match admit_metadata(
        &container.inventory.bytes,
        &container.assessment.bytes,
        context,
    ) {
        Ok(metadata) => metadata,
        Err(reason) => return IncidentVerifyReport::refusal(reason),
    };

    // Phase 4: digest, policy_binding
    let actual_assessment_sha = hex::encode(Sha256::digest(&container.assessment.bytes));
    if inventory.assessment_sha256 != actual_assessment_sha {
        return IncidentVerifyReport::refusal(IncidentReason::Digest);
    }

    if let Err(reason) = check_inventory_budget(&inventory, context) {
        return IncidentVerifyReport::refusal(reason);
    }

    let mut object_map: BTreeMap<&str, &[u8]> = BTreeMap::new();
    for obj in &container.objects {
        object_map.insert(&obj.name, &obj.bytes);
    }

    let mut referenced_paths = BTreeSet::new();
    for input in &inventory.inputs {
        let expected_path = format!("objects/{}", input.sha256);
        let Some(obj_bytes) = object_map.get(expected_path.as_str()) else {
            return IncidentVerifyReport::refusal(IncidentReason::Digest);
        };
        if obj_bytes.len() as u64 != input.bytes {
            return IncidentVerifyReport::refusal(IncidentReason::Digest);
        }
        let actual_obj_sha = hex::encode(Sha256::digest(obj_bytes));
        if actual_obj_sha != input.sha256 {
            return IncidentVerifyReport::refusal(IncidentReason::Digest);
        }
        referenced_paths.insert(expected_path);
    }

    // All container objects must be referenced by at least one input
    for obj in &container.objects {
        if !referenced_paths.contains(&obj.name) {
            return IncidentVerifyReport::refusal(IncidentReason::Digest);
        }
    }

    let inputs_by_id: BTreeMap<&str, &InventoryInput> = inventory
        .inputs
        .iter()
        .map(|i| (i.id.as_str(), i))
        .collect();

    // Phase 4 policy references already present in the package documents. The
    // input occurrence designates the original bytes, not policy activation or
    // the semantic policy digest carried by a decision. Reuse the locator rule.
    let references = inventory
        .context
        .values()
        .flat_map(|entry| &entry.refs)
        .chain(
            assessment
                .surfaces
                .iter()
                .flat_map(|surface| &surface.basis_refs),
        )
        .chain(assessment.units.iter().flat_map(|unit| {
            std::iter::once(&unit.source).chain(unit.result.as_ref().map(|result| match result {
                UnitDisclosure::Inline { value } => &value.source,
                UnitDisclosure::Reference { target } => target,
            }))
        }))
        .chain(
            assessment
                .joins
                .iter()
                .flat_map(|join| std::iter::once(&join.left).chain(join.right.as_ref())),
        )
        .chain(
            assessment
                .claims
                .iter()
                .flat_map(|claim| &claim.evidence_refs),
        )
        .chain(&assessment.resolved_results);
    for reference in references {
        if inputs_by_id
            .get(reference.input_id.as_str())
            .is_some_and(|input| input.format == "policy-bytes")
            && original_ref_input(reference, &inputs_by_id, &object_map, &BTreeMap::new()).is_err()
        {
            return IncidentVerifyReport::refusal(IncidentReason::PolicyBinding);
        }
    }

    // Phase 5: locator, input_shape (locators and source object syntax)

    let mut derived_surfaces = BTreeMap::new();

    let parsed_bundles =
        match parse_bundle_inputs(&inventory, &object_map, context, &mut source_budget) {
            Ok(bundles) => bundles,
            Err(error) => {
                return IncidentVerifyReport::refusal(incident_refusal_for_resolution(error));
            }
        };

    if let Err(error) =
        validate_context_source_refs(&inventory, &inputs_by_id, &object_map, &parsed_bundles)
    {
        return IncidentVerifyReport::refusal(incident_refusal_for_resolution(error));
    }

    for unit in &assessment.units {
        let Some(input) = inputs_by_id.get(unit.source.input_id.as_str()) else {
            return IncidentVerifyReport::refusal(IncidentReason::Locator);
        };
        if input.sha256 != unit.source.sha256 {
            return IncidentVerifyReport::refusal(IncidentReason::Locator);
        }
        let obj_path = format!("objects/{}", input.sha256);
        let obj_bytes = object_map.get(obj_path.as_str()).expect("verified present");

        if input.format == "inspector-protocol-793d103" {
            if !unit.source.locator.starts_with("json:/") {
                return IncidentVerifyReport::refusal(IncidentReason::Locator);
            }
            let idx_str = &unit.source.locator[6..];
            let idx: usize = match idx_str.parse() {
                Ok(n) => n,
                Err(_) => return IncidentVerifyReport::refusal(IncidentReason::Locator),
            };
            if idx_str.len() > 1 && idx_str.starts_with('0') {
                return IncidentVerifyReport::refusal(IncidentReason::Locator);
            }
            let arr: Vec<serde_json::Value> = match serde_json::from_slice(obj_bytes) {
                Ok(a) => a,
                Err(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
            };
            if idx >= arr.len() {
                return IncidentVerifyReport::refusal(IncidentReason::Locator);
            }
        }
    }

    for surface in &assessment.surfaces {
        for reference in &surface.basis_refs {
            if let Err(error) = resolve_applicability_ref(
                reference,
                &surface.name,
                &inputs_by_id,
                &object_map,
                &parsed_bundles,
                context,
            ) {
                return IncidentVerifyReport::refusal(incident_refusal_for_resolution(error));
            }
        }
    }
    for surface in &assessment.surfaces {
        let applicability = context
            .applicability
            .iter()
            .find(|applicability| applicability.surface == surface.name);
        if let Some(applicability) = applicability {
            for reference in &applicability.input_refs {
                if let Err(error) = resolve_applicability_ref(
                    reference,
                    &surface.name,
                    &inputs_by_id,
                    &object_map,
                    &parsed_bundles,
                    context,
                ) {
                    return IncidentVerifyReport::refusal(incident_refusal_for_resolution(error));
                }
            }
        }
    }

    let retained_context =
        match retained_context_inventory(&inventory, &object_map, &parsed_bundles, context) {
            Ok(retained) => retained,
            Err(error) => {
                return IncidentVerifyReport::refusal(incident_refusal_for_resolution(error));
            }
        };
    let action_records = match admitted_action_records(&inventory, &object_map, &parsed_bundles) {
        Ok(records) => records,
        Err(error) => return IncidentVerifyReport::refusal(incident_refusal_for_resolution(error)),
    };
    for unit in &assessment.units {
        let Some(record) = action_records
            .iter()
            .find(|record| record.reference == unit.source)
        else {
            return IncidentVerifyReport::refusal(IncidentReason::Locator);
        };
        if record.surface != unit.surface {
            return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
        }
    }
    for join in &assessment.joins {
        if !validate_join_references(join, &action_records) {
            return IncidentVerifyReport::refusal(IncidentReason::Locator);
        }
    }
    for claim in &assessment.claims {
        for reference in &claim.evidence_refs {
            if action_records
                .iter()
                .any(|record| record.reference == *reference)
            {
                continue;
            }
            if let Err(error) = resolve_applicability_ref(
                reference,
                &claim.surface,
                &inputs_by_id,
                &object_map,
                &parsed_bundles,
                context,
            ) {
                return IncidentVerifyReport::refusal(incident_refusal_for_resolution(error));
            }
        }
    }

    for surface in &assessment.surfaces {
        let applicability = context
            .applicability
            .iter()
            .find(|applicability| applicability.surface == surface.name);
        let source_inputs = incident_surface_sources(&inventory, &surface.name);
        derived_surfaces.insert(
            surface.name.clone(),
            derive_surface_result(
                &source_inputs,
                applicability,
                &retained_context,
                &surface.name,
            ),
        );
    }

    // Validate non-attestation source bytes parsing
    for input in &inventory.inputs {
        let obj_path = format!("objects/{}", input.sha256);
        let obj_bytes = object_map.get(obj_path.as_str()).expect("verified present");
        if input.format == "inspector-protocol-793d103" {
            let Ok(text) = std::str::from_utf8(obj_bytes) else {
                return IncidentVerifyReport::refusal(IncidentReason::InputShape);
            };
            let arr: Vec<serde_json::Value> = match serde_json::from_str(text) {
                Ok(a) => a,
                Err(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
            };
            for entry in &arr {
                let Some(obj) = entry.as_object() else {
                    return IncidentVerifyReport::refusal(IncidentReason::InputShape);
                };
                if !obj.contains_key("id")
                    || !obj.contains_key("timestamp")
                    || !obj.contains_key("direction")
                    || !obj.contains_key("message")
                {
                    return IncidentVerifyReport::refusal(IncidentReason::InputShape);
                }
            }
        }
    }

    // Phase 6: trust_input, input_shape, attestation_verification
    let mut verified_attestations: Vec<serde_json::Value> = Vec::new();

    for input in &inventory.inputs {
        if input.format == "dsse-attestation" {
            let binding = input.binding.as_ref().expect("checked present in phase 2");
            let Some(bundle_input) = inputs_by_id.get(binding.bundle_input.as_str()) else {
                return IncidentVerifyReport::refusal(IncidentReason::Locator);
            };
            let Some(key_input) = inputs_by_id.get(binding.key_input.as_str()) else {
                return IncidentVerifyReport::refusal(IncidentReason::Locator);
            };

            if !context.keys.contains(&key_input.sha256) {
                let failing_row = serde_json::json!({
                    "input_id": input.id,
                    "key_sha256": key_input.sha256,
                    "status": "refused",
                    "signature_verified": false,
                    "subject_matched": false,
                    "artifact_sha256": null,
                    "extent_stated": false,
                    "extent": null,
                });
                let mut rep = IncidentVerifyReport::refusal(IncidentReason::TrustInput);
                rep.attestations = vec![failing_row];
                return rep;
            }

            if bundle_input.format != "assay-bundle-v1"
                || key_input.format != "ed25519-public-key-pem"
            {
                let failing_row = serde_json::json!({
                    "input_id": input.id,
                    "key_sha256": key_input.sha256,
                    "status": "refused",
                    "signature_verified": false,
                    "subject_matched": false,
                    "artifact_sha256": null,
                    "extent_stated": false,
                    "extent": null,
                });
                let mut rep = IncidentVerifyReport::refusal(IncidentReason::InputShape);
                rep.attestations = vec![failing_row];
                return rep;
            }

            let att_path = format!("objects/{}", input.sha256);
            let key_path = format!("objects/{}", key_input.sha256);
            let bundle_path = format!("objects/{}", bundle_input.sha256);

            let (Some(att_bytes), Some(key_bytes), Some(bundle_bytes)) = (
                object_map.get(att_path.as_str()),
                object_map.get(key_path.as_str()),
                object_map.get(bundle_path.as_str()),
            ) else {
                let failing_row = serde_json::json!({
                    "input_id": input.id,
                    "key_sha256": key_input.sha256,
                    "status": "unavailable",
                    "signature_verified": false,
                    "subject_matched": false,
                    "artifact_sha256": null,
                    "extent_stated": false,
                    "extent": null,
                });
                let rep = IncidentVerifyReport {
                    schema: INCIDENT_VERIFY_SCHEMA_V1.to_string(),
                    outcome: IncidentOutcome::VerificationUnavailable,
                    reason: Some(IncidentReason::IoUnavailable),
                    artifact_sha256: None,
                    inventory_sha256: None,
                    assessment_sha256: None,
                    expectation: IncidentExpectation::NotEvaluated,
                    attestations: vec![failing_row],
                    verification_context: None,
                    verification_context_sha256: None,
                    resolved_results: Vec::new(),
                    counts: None,
                    non_claims: NON_CLAIMS_DEFAULT.iter().map(|s| s.to_string()).collect(),
                };
                return rep;
            };

            let Ok(pem_str) = std::str::from_utf8(key_bytes) else {
                let failing_row = serde_json::json!({
                    "input_id": input.id,
                    "key_sha256": key_input.sha256,
                    "status": "refused",
                    "signature_verified": false,
                    "subject_matched": false,
                    "artifact_sha256": null,
                    "extent_stated": false,
                    "extent": null,
                });
                let mut rep = IncidentVerifyReport::refusal(IncidentReason::InputShape);
                rep.attestations = vec![failing_row];
                return rep;
            };

            let Ok(verifying_key) = VerifyingKey::from_public_key_pem(pem_str) else {
                let failing_row = serde_json::json!({
                    "input_id": input.id,
                    "key_sha256": key_input.sha256,
                    "status": "refused",
                    "signature_verified": false,
                    "subject_matched": false,
                    "artifact_sha256": null,
                    "extent_stated": false,
                    "extent": null,
                });
                let mut rep = IncidentVerifyReport::refusal(IncidentReason::InputShape);
                rep.attestations = vec![failing_row];
                return rep;
            };

            if let Err(error) = preflight_single_json_source(
                att_bytes,
                "dsse-attestation",
                SourceJsonKind::Dsse,
                &mut source_budget,
                context,
            ) {
                if matches!(
                    error,
                    ResolveRefError::ResourceHard | ResolveRefError::ResourceSelected
                ) {
                    return IncidentVerifyReport::refusal(incident_refusal_for_resolution(error));
                }
                let failing_row = serde_json::json!({
                    "input_id": input.id,
                    "key_sha256": key_input.sha256,
                    "status": "refused",
                    "signature_verified": false,
                    "subject_matched": false,
                    "artifact_sha256": null,
                    "extent_stated": false,
                    "extent": null,
                });
                let mut rep = IncidentVerifyReport::refusal(IncidentReason::InputShape);
                rep.attestations = vec![failing_row];
                return rep;
            }

            let Ok(envelope) = serde_json::from_slice::<DsseEnvelope>(att_bytes) else {
                let failing_row = serde_json::json!({
                    "input_id": input.id,
                    "key_sha256": key_input.sha256,
                    "status": "refused",
                    "signature_verified": false,
                    "subject_matched": false,
                    "artifact_sha256": null,
                    "extent_stated": false,
                    "extent": null,
                });
                let mut rep = IncidentVerifyReport::refusal(IncidentReason::InputShape);
                rep.attestations = vec![failing_row];
                return rep;
            };

            let verify_limits = incident_bundle_limits(
                context,
                context.limits.records.min(ContextLimits::HARD.records),
            );

            match verify_attestation_for_bundle_with_extent_and_limits(
                &envelope,
                &verifying_key,
                bundle_bytes,
                verify_limits,
            ) {
                Ok(checked) => {
                    let (verified, extent) = checked.into_parts();
                    let extent_stated = extent.is_some();
                    let extent_val = extent
                        .map(|e| serde_json::to_value(e).expect("serializable"))
                        .unwrap_or(serde_json::Value::Null);
                    let row = serde_json::json!({
                        "input_id": input.id,
                        "key_sha256": key_input.sha256,
                        "status": "verified",
                        "signature_verified": true,
                        "subject_matched": true,
                        "artifact_sha256": verified.artifact_sha256,
                        "predicate_type": verified.statement.predicate_type,
                        "subject_name": verified.statement.subject[0].name,
                        "extent_stated": extent_stated,
                        "extent": extent_val,
                    });
                    verified_attestations.push(row);
                }
                Err(_) => {
                    let failing_row = serde_json::json!({
                        "input_id": input.id,
                        "key_sha256": key_input.sha256,
                        "status": "refused",
                        "signature_verified": false,
                        "subject_matched": false,
                        "artifact_sha256": null,
                        "extent_stated": false,
                        "extent": null,
                    });
                    let mut rep =
                        IncidentVerifyReport::refusal(IncidentReason::AttestationVerification);
                    rep.attestations = vec![failing_row];
                    return rep;
                }
            }
        }
    }

    // Phase 7: disclosure (unit ID binding, duplicate check, ordering, reference resolution)
    for unit in &assessment.units {
        let arr = serde_json::json!([
            unit.source.input_id,
            unit.source.sha256,
            unit.source.locator
        ]);
        let jcs_bytes = match assay_canonical::jcs::to_vec(&arr) {
            Ok(b) => b,
            Err(_) => return IncidentVerifyReport::refusal(IncidentReason::Disclosure),
        };
        let computed_id = hex::encode(Sha256::digest(&jcs_bytes));
        if unit.id != computed_id {
            return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
        }
    }

    // Ordering and uniqueness of unit IDs
    for i in 1..assessment.units.len() {
        if assessment.units[i - 1].id >= assessment.units[i].id {
            return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
        }
    }

    // Dispositions & disclosure rules
    for unit in &assessment.units {
        let Some(record) = action_records
            .iter()
            .find(|record| record.reference == unit.source)
        else {
            return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
        };
        if !record.examination_succeeded {
            if unit.disposition != "failed"
                || unit.result.is_some()
                || unit.withheld_digest.is_some()
            {
                return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
            }
            continue;
        }
        let fresh = fresh_examination_result(record);
        let Ok(fresh_bytes) = examination_result_bytes(&fresh) else {
            return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
        };
        if fresh_bytes.len() as u64 > ContextLimits::HARD.fresh_result_bytes {
            return IncidentVerifyReport::refusal(IncidentReason::ResourceHard);
        }
        if fresh_bytes.len() as u64 > context.limits.fresh_result_bytes {
            return IncidentVerifyReport::refusal(IncidentReason::ResourceSelected);
        }
        if unit.disposition == "examined" {
            if unit.withheld_digest.is_some() {
                return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
            }
            match &unit.result {
                None => return IncidentVerifyReport::refusal(IncidentReason::Disclosure),
                Some(UnitDisclosure::Reference { target }) => {
                    let Some(res_input) = inputs_by_id.get(target.input_id.as_str()) else {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    };
                    if res_input.format != "incident-results-v1"
                        || res_input.sha256 != target.sha256
                    {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    }
                    let res_obj_path = format!("objects/{}", target.sha256);
                    let res_obj_bytes = object_map
                        .get(res_obj_path.as_str())
                        .expect("verified present");
                    if let Err(error) = check_source_bytes(
                        "incident-results-v1",
                        res_obj_bytes.len() as u64,
                        context,
                    ) {
                        return IncidentVerifyReport::refusal(incident_refusal_for_resolution(
                            error,
                        ));
                    }
                    let res_doc: IncidentResultsDocument =
                        match serde_json::from_slice(res_obj_bytes) {
                            Ok(v) => v,
                            Err(_) => {
                                return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                            }
                        };
                    if res_doc.schema != "assay.incident.results.v1" {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    }
                    if !target.locator.starts_with("json:/results/") {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    }
                    let idx_str = &target.locator[14..];
                    let idx: usize = match idx_str.parse() {
                        Ok(n) => n,
                        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::Disclosure),
                    };
                    let Some(target_res) = res_doc.results.get(idx) else {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    };
                    if target_res != &fresh {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    }
                }
                Some(UnitDisclosure::Inline { value }) => {
                    if value != &fresh {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    }
                }
            }
        } else if unit.disposition == "withheld" {
            if unit.result.is_some() || unit.withheld_digest.is_none() {
                return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
            }
            let expected_digest = hex::encode(Sha256::digest(&fresh_bytes));
            if unit.withheld_digest.as_deref() != Some(expected_digest.as_str()) {
                return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
            }
        } else if unit.result.is_some() || unit.withheld_digest.is_some() {
            return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
        }
    }

    // Complete enumeration of eligible units from admitted inputs.
    if assessment.units.len() != action_records.len() {
        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
    }

    // Phase 8: cap1_shape, cap1_rules
    let present_surfaces: Vec<&str> = assessment
        .surfaces
        .iter()
        .filter(|s| !s.source_inputs.is_empty())
        .map(|s| s.name.as_str())
        .collect();
    let mut verified_cap1 = None;

    if present_surfaces.is_empty() {
        if assessment.cap1.is_some() {
            return IncidentVerifyReport::refusal(IncidentReason::Cap1Rules);
        }
    } else {
        let Some(cap1_val) = &assessment.cap1 else {
            return IncidentVerifyReport::refusal(IncidentReason::Cap1Rules);
        };
        let cap1_bytes = match serde_json::to_vec(cap1_val) {
            Ok(b) => b,
            Err(_) => return IncidentVerifyReport::refusal(IncidentReason::Cap1Shape),
        };
        let cap1_limits = Cap1AdmissionLimits {
            max_bytes: context.limits.health_bytes as usize,
        };
        match verify_cap1_document(&cap1_bytes, &cap1_limits) {
            Err(refusal) => match refusal.stage() {
                Cap1Stage::Schema | Cap1Stage::Admission => {
                    return IncidentVerifyReport::refusal(IncidentReason::Cap1Shape);
                }
                Cap1Stage::Rules => {
                    return IncidentVerifyReport::refusal(IncidentReason::Cap1Rules);
                }
            },
            Ok(doc) => {
                if doc.strata.len() != present_surfaces.len() {
                    return IncidentVerifyReport::refusal(IncidentReason::Cap1Rules);
                }
                for (stratum, expected_name) in doc.strata.iter().zip(present_surfaces.iter()) {
                    if stratum.id != *expected_name {
                        return IncidentVerifyReport::refusal(IncidentReason::Cap1Rules);
                    }
                }
                verified_cap1 = Some(doc);
            }
        }
    }

    // Phase 9: claim_boundary
    if let Some(cap1) = verified_cap1.as_ref() {
        if !validate_cap1_package_association(cap1, &assessment.units, &assessment.claims) {
            return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
        }
    } else if !assessment.claims.is_empty() || !assessment.joins.is_empty() {
        return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
    }

    let mut derived_joins = BTreeMap::new();
    for join in &assessment.joins {
        let Some(derived) =
            derive_join(join, &action_records, &derived_surfaces, &retained_context)
        else {
            return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
        };
        if !join_matches_derivation(join, &derived) {
            return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
        }
        derived_joins.insert(join.id.as_str(), derived);
    }

    for claim in &assessment.claims {
        let Some(join) = assessment
            .joins
            .iter()
            .find(|join| join.id == claim.proposition)
        else {
            return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
        };
        let Some(derived_join) = derived_joins.get(join.id.as_str()) else {
            return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
        };
        let Some(surface) = derived_surfaces.get(&claim.surface) else {
            return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
        };
        let Some(cap1) = verified_cap1.as_ref() else {
            return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
        };
        let Some(stratum) = cap1
            .strata
            .iter()
            .find(|stratum| stratum.id == claim.stratum)
        else {
            return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
        };
        let support_present = claim.stratum == claim.surface
            && join.proposition.surface == claim.surface
            && stratum
                .supports
                .as_ref()
                .is_some_and(|supports| supports.iter().any(|support| support == &claim.kind));
        let assertion_count = cap1
            .absence_assertions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|assertion| assertion.assertion == claim.id)
            .count();
        let derived_decision = match claim.kind.as_str() {
            "positive_presence" => {
                if support_present && derived_join.result == "confirmed" {
                    "supported"
                } else {
                    "blocked"
                }
            }
            "bounded_negative" => {
                if support_present
                    && derived_join.result == "contradicted"
                    && join.proposition.kind == "counterpart_present"
                    && surface.observation == "adequate"
                    && !retained_context.enforcement_degraded
                    && assertion_count == 1
                {
                    "supported"
                } else {
                    "blocked"
                }
            }
            "exhaustive_set" => {
                if !support_present || retained_context.enforcement_degraded {
                    "blocked"
                } else if surface.observation == "adequate"
                    && cap1.integrity.complete
                    && assessment
                        .joins
                        .iter()
                        .filter(|candidate| candidate.proposition.surface == claim.surface)
                        .all(|candidate| {
                            derived_joins
                                .get(candidate.id.as_str())
                                .is_some_and(|derived| derived.result != "unobservable")
                        })
                {
                    "supported"
                } else {
                    "degraded"
                }
            }
            _ => {
                return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
            }
        };
        if claim.kind == "bounded_negative"
            && derived_decision != "supported"
            && assertion_count != 0
        {
            return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
        }
        if claim.decision != derived_decision {
            return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
        }
    }

    // Phase 10: expectation_mismatch
    for exp in &context.expectations {
        let Some(input) = inputs_by_id.get(exp.input_id.as_str()) else {
            let mut rep = IncidentVerifyReport::refusal(IncidentReason::ExpectationMismatch);
            rep.expectation = IncidentExpectation::Mismatched;
            return rep;
        };
        if input.sha256 != exp.sha256 {
            let mut rep = IncidentVerifyReport::refusal(IncidentReason::ExpectationMismatch);
            rep.expectation = IncidentExpectation::Mismatched;
            return rep;
        }
        if exp.require_disclosure {
            for unit in &assessment.units {
                if unit.source.input_id == exp.input_id && unit.disposition != "examined" {
                    let mut rep =
                        IncidentVerifyReport::refusal(IncidentReason::ExpectationMismatch);
                    rep.expectation = IncidentExpectation::Mismatched;
                    return rep;
                }
            }
        }
    }

    // Phase 11: stale_assessment
    let mut ctx_jcs = match assay_canonical::jcs::to_vec(context) {
        Ok(b) => b,
        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment),
    };
    ctx_jcs.push(b'\n');
    let expected_ctx_sha = hex::encode(Sha256::digest(&ctx_jcs));
    if assessment.verification_context_sha256 != expected_ctx_sha {
        return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
    }

    if assessment.surfaces.len() != 5 {
        return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
    }
    for (s, &expected_name) in assessment.surfaces.iter().zip(VALID_SURFACES.iter()) {
        if s.name != expected_name {
            return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
        }
        let expected_sources = incident_surface_sources(&inventory, &s.name);
        if s.source_inputs != expected_sources {
            return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
        }
        let Some(derived) = derived_surfaces.get(&s.name) else {
            return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
        };
        if s.observation != derived.observation
            || s.basis_refs != derived.basis_refs
            || s.window != derived.window
        {
            return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
        }
    }

    // Check resolved_results matching recomputed targets
    let mut expected_resolved: Vec<PackageRef> = Vec::new();
    for unit in &assessment.units {
        if unit.disposition == "examined" {
            if let Some(UnitDisclosure::Reference { target }) = &unit.result {
                if !expected_resolved.contains(target) {
                    expected_resolved.push(target.clone());
                }
            }
        }
    }
    expected_resolved.sort_by(|a, b| {
        a.input_id
            .cmp(&b.input_id)
            .then_with(|| a.locator.cmp(&b.locator))
            .then_with(|| a.sha256.cmp(&b.sha256))
    });
    if assessment.resolved_results != expected_resolved {
        return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
    }

    // Phase 11: compare retained attestation-report inputs against fresh canonical results
    for input in &inventory.inputs {
        if input.format == "attestation-report" {
            let Some(rep_b) = &input.binding else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            let Some(att_input_id) = &rep_b.attestation_input else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            let Some(dsse_input) = inputs_by_id.get(att_input_id.as_str()) else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            if dsse_input.format != "dsse-attestation" {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }
            let Some(dsse_b) = &dsse_input.binding else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            if dsse_b.bundle_input != rep_b.bundle_input || dsse_b.key_input != rep_b.key_input {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            let Some(canonical_row) = verified_attestations.iter().find(|r| {
                r.get("input_id").and_then(|v| v.as_str()) == Some(att_input_id.as_str())
            }) else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            if canonical_row.get("status").and_then(|v| v.as_str()) != Some("verified") {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            let rep_path = format!("objects/{}", input.sha256);
            let rep_bytes = object_map
                .get(rep_path.as_str())
                .expect("verified present in phase 4");

            if let Err(error) = preflight_single_json_source(
                rep_bytes,
                "attestation-report",
                SourceJsonKind::Metadata,
                &mut source_budget,
                context,
            ) {
                return IncidentVerifyReport::refusal(match error {
                    ResolveRefError::ResourceHard | ResolveRefError::ResourceSelected => {
                        incident_refusal_for_resolution(error)
                    }
                    _ => IncidentReason::StaleAssessment,
                });
            }

            let Ok(rep_str) = std::str::from_utf8(rep_bytes) else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            if validate_json_strict(rep_str).is_err() {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            let Ok(rep_val) = serde_json::from_slice::<serde_json::Value>(rep_bytes) else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            let Some(rep_obj) = rep_val.as_object() else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };

            for key in rep_obj.keys() {
                if !KNOWN_ATTESTATION_REPORT_FIELDS.contains(&key.as_str()) {
                    return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
                }
            }

            if rep_obj.get("schema").and_then(|v| v.as_str())
                != Some("assay.evidence.attestation.verify.v1")
            {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            if rep_obj.get("outcome").and_then(|v| v.as_str()) != Some("attestation_verified") {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            let Some(canonical_sig) = canonical_row
                .get("signature_verified")
                .and_then(|v| v.as_bool())
            else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            if rep_obj.get("signature_verified").and_then(|v| v.as_bool()) != Some(canonical_sig) {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            let Some(canonical_sub_matched) = canonical_row
                .get("subject_matched")
                .and_then(|v| v.as_bool())
            else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            if rep_obj.get("subject_matched").and_then(|v| v.as_bool())
                != Some(canonical_sub_matched)
            {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            let Some(canonical_art_sha) = canonical_row
                .get("artifact_sha256")
                .and_then(|v| v.as_str())
            else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            let Some(rep_art_sha) = rep_obj.get("artifact_sha256").and_then(|v| v.as_str()) else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            if canonical_art_sha != rep_art_sha {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            let Some(canonical_pred_type) =
                canonical_row.get("predicate_type").and_then(|v| v.as_str())
            else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            let Some(rep_pred_type) = rep_obj.get("predicate_type").and_then(|v| v.as_str()) else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            if canonical_pred_type != rep_pred_type {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            let Some(canonical_subj_name) =
                canonical_row.get("subject_name").and_then(|v| v.as_str())
            else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            let Some(rep_subj_name) = rep_obj.get("subject_name").and_then(|v| v.as_str()) else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            if canonical_subj_name != rep_subj_name {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            let Some(canonical_extent_stated) =
                canonical_row.get("extent_stated").and_then(|v| v.as_bool())
            else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            let Some(rep_extent_stated) = rep_obj.get("extent_stated").and_then(|v| v.as_bool())
            else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            if canonical_extent_stated != rep_extent_stated {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }

            let Some(rep_extent) = rep_obj.get("extent") else {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            };
            let canonical_extent = canonical_row
                .get("extent")
                .unwrap_or(&serde_json::Value::Null);
            if rep_extent != canonical_extent {
                return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
            }
        }
    }

    // Terminal Success
    let examined_count = assessment
        .units
        .iter()
        .filter(|u| u.disposition == "examined")
        .count() as u64;
    let withheld_count = assessment
        .units
        .iter()
        .filter(|u| u.disposition == "withheld")
        .count() as u64;

    IncidentVerifyReport {
        schema: INCIDENT_VERIFY_SCHEMA_V1.to_string(),
        outcome: IncidentOutcome::PackageVerified,
        reason: None,
        artifact_sha256: Some(hex::encode(Sha256::digest(bytes))),
        inventory_sha256: Some(hex::encode(Sha256::digest(&container.inventory.bytes))),
        assessment_sha256: Some(hex::encode(Sha256::digest(&container.assessment.bytes))),
        expectation: if context.expectations.is_empty() {
            IncidentExpectation::NotRequested
        } else {
            IncidentExpectation::Matched
        },
        attestations: verified_attestations,
        verification_context: Some(serde_json::to_value(context).expect("serializable")),
        verification_context_sha256: Some(expected_ctx_sha),
        resolved_results: assessment
            .resolved_results
            .iter()
            .map(|r| serde_json::to_value(r).expect("serializable"))
            .collect(),
        counts: Some(serde_json::json!({
            "inputs": inventory.inputs.len() as u64,
            "objects": container.objects.len() as u64,
            "units": assessment.units.len() as u64,
            "examined": examined_count,
            "withheld": withheld_count,
            "resolved_result_count": assessment.resolved_results.len() as u64,
        })),
        non_claims: NON_CLAIMS_DEFAULT.iter().map(|s| s.to_string()).collect(),
    }
}

#[cfg(test)]
mod join_eligibility_tests {
    use super::*;

    #[test]
    fn canonical_non_decision_event_cannot_supply_a_strong_tool_identity() {
        let record = AdmittedActionRecord {
            reference: PackageRef {
                input_id: "bundle".into(),
                sha256: "0".repeat(64),
                locator: "event:run:1".into(),
            },
            surface: "filesystem",
            operation: "assay-bundle-v1".into(),
            value: serde_json::json!({
                "event": {"data": {"tool_call_id": "call-1"}},
                "events_sha256": "0".repeat(64)
            }),
            timestamp: None,
            examination_succeeded: true,
        };
        let contract = JoinContract {
            schema: "assay.observability.join_result.v0".into(),
            join_key: "tool_call_id".into(),
            scope: "tool_call".into(),
            join_value: Some("call-1".into()),
            join_grade: "strong".into(),
            unique_within_scope: true,
            left_artifact_role: "external_receipt".into(),
            right_artifact_role: "external_receipt".into(),
            evidence_refs: Vec::new(),
            fallback_used: false,
            notes: Vec::new(),
        };
        assert_eq!(
            strong_key_value(&record, "/data/tool_call_id", &contract),
            None
        );
    }

    #[test]
    fn inspector_payload_classification_uses_only_admitted_paths() {
        let context = ContextInput::default();
        let mut budget = SourceBudgetState::default();
        let hooks = SourceJsonHooks {
            kind: SourceJsonKind::Inspector,
            budget: &mut budget,
            context: &context,
        };
        let index = PathSegment::Index(0);
        assert_eq!(
            hooks.string_class(&[
                index.clone(),
                PathSegment::Key("message".into()),
                PathSegment::Key("params".into()),
                PathSegment::Key("arguments".into()),
                PathSegment::Key("value".into()),
            ]),
            StringClass::Payload
        );
        assert_eq!(
            hooks.string_class(&[
                index,
                PathSegment::Key("metadata".into()),
                PathSegment::Key("arguments".into()),
            ]),
            StringClass::NonPayload
        );
    }

    #[test]
    fn inner_bundle_preflight_counts_unknown_utf8_keys_and_payload_strings_by_bytes() {
        fn check(
            json: String,
            kind: SourceJsonKind,
            key_limit: u64,
            payload_limit: u64,
        ) -> Result<(), ResolveRefError> {
            let mut context = ContextInput::default();
            context.limits.json_key_bytes = key_limit;
            context.limits.payload_string_bytes = payload_limit;
            preflight_json(&json, kind, &mut SourceBudgetState::default(), &context)
        }

        let utf8_key = "éé";
        let manifest = format!(r#"{{"{utf8_key}":0}}"#);
        assert_eq!(
            check(manifest.clone(), SourceJsonKind::Metadata, 4, 1),
            Ok(())
        );
        assert_eq!(
            check(manifest, SourceJsonKind::Metadata, 3, 1),
            Err(ResolveRefError::ResourceSelected)
        );
        assert_eq!(
            check(
                format!(r#"{{"{}":0}}"#, "k".repeat(257)),
                SourceJsonKind::Metadata,
                256,
                1,
            ),
            Err(ResolveRefError::ResourceHard)
        );

        let event = r#"{"data":{"unknown":"\u00e9\u00e9"}}"#.to_string();
        assert_eq!(
            check(event.clone(), SourceJsonKind::Canonical, 256, 4),
            Ok(())
        );
        assert_eq!(
            check(event, SourceJsonKind::Canonical, 256, 3),
            Err(ResolveRefError::ResourceSelected)
        );
        assert_eq!(
            check(
                format!(r#"{{"data":{{"unknown":"{}"}}}}"#, "x".repeat(1_048_577)),
                SourceJsonKind::Canonical,
                256,
                1_048_576,
            ),
            Err(ResolveRefError::ResourceHard)
        );
    }

    #[test]
    fn canonical_event_count_accepts_exact_limits_and_reports_the_winning_tier() {
        let mut selected_context = ContextInput::default();
        selected_context.limits.records = 2;
        let mut selected_budget = SourceBudgetState::default();
        assert_eq!(selected_budget.charge_record(&selected_context), Ok(()));
        assert_eq!(selected_budget.charge_record(&selected_context), Ok(()));
        assert_eq!(
            selected_budget.charge_record(&selected_context),
            Err(BudgetHit {
                dimension: BudgetDimension::Items,
                tier: BudgetTier::Selected,
            })
        );

        let mut hard_context = ContextInput::default();
        hard_context.limits.records = ContextLimits::HARD.records;
        let mut hard_budget = SourceBudgetState::default();
        for _ in 0..ContextLimits::HARD.records {
            hard_budget.charge_record(&hard_context).unwrap();
        }
        assert_eq!(
            hard_budget.charge_record(&hard_context),
            Err(BudgetHit {
                dimension: BudgetDimension::Items,
                tier: BudgetTier::Hard,
            })
        );
    }
}
