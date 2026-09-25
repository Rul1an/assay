//! Incident Package v1 verification engine.
//!
//! Enforces the 11 ordered phases of SPEC-Incident-Package-v1 section 10,
//! composing the container reader and the normative CAP-1 verifier.

use std::collections::{BTreeMap, BTreeSet};

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    read_incident_container, IncidentExpectation, IncidentOutcome, IncidentReason,
    IncidentVerifyReport, INCIDENT_VERIFY_SCHEMA_V1, NON_CLAIMS_DEFAULT,
};
use crate::attestation::{verify_attestation_for_bundle_with_extent_and_limits, DsseEnvelope};
use crate::bundle::VerifyLimits;
use crate::coverage_attestation::{
    verify_cap1_document, Cap1AdmissionLimits, Cap1Stage, CAP1_SCHEMA_SHA256,
};
use crate::json_strict::validate_json_strict;
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

const ADMITTED_FORMATS: [&str; 11] = [
    "assay-bundle-v1",
    "assay-health-json",
    "assay-coverage-json",
    "incident-context-v1",
    "incident-results-v1",
    "ed25519-public-key-pem",
    "proxy-decision-ndjson",
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
fn validate_context_input(ctx: &ContextInput) -> Result<(), IncidentReason> {
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
        if let Some(w) = &app.window {
            let w_start = parse_utc_timestamp(&w.start).ok_or(IncidentReason::TrustInput)?;
            let w_end = parse_utc_timestamp(&w.end).ok_or(IncidentReason::TrustInput)?;
            if w_start > w_end {
                return Err(IncidentReason::TrustInput);
            }
        }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryDocument {
    schema: String,
    assessment_sha256: String,
    inputs: Vec<InventoryInput>,
    context: serde_json::Value,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentDocument {
    schema: String,
    verification_context_sha256: String,
    units: Vec<AssessmentUnit>,
    surfaces: Vec<SurfaceResult>,
    cap1: Option<serde_json::Value>,
    joins: Vec<serde_json::Value>,
    claims: Vec<serde_json::Value>,
    resolved_results: Vec<PackageRef>,
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
    let container = match read_incident_container(bytes) {
        Ok(c) => c,
        Err(report) => return report,
    };

    let total_members = 2 + container.objects.len() as u64;
    if total_members > ContextLimits::HARD.members {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceHard);
    }
    if total_members > context.limits.members {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceSelected);
    }

    if container.inventory.bytes.len() as u64 > ContextLimits::HARD.inventory_bytes {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceHard);
    }
    if container.inventory.bytes.len() as u64 > context.limits.inventory_bytes {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceSelected);
    }

    if container.assessment.bytes.len() as u64 > ContextLimits::HARD.assessment_bytes {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceHard);
    }
    if container.assessment.bytes.len() as u64 > context.limits.assessment_bytes {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceSelected);
    }

    // Phase 2: Canonical package JSON check (UTF-8 no BOM, strict JSON, JCS+LF equivalence)
    let inv_str = match std::str::from_utf8(&container.inventory.bytes) {
        Ok(s) => s,
        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
    };
    if inv_str.starts_with('\u{feff}') {
        return IncidentVerifyReport::refusal(IncidentReason::InputShape);
    }
    if validate_json_strict(inv_str).is_err() {
        return IncidentVerifyReport::refusal(IncidentReason::InputShape);
    }
    let inv_val: serde_json::Value = match serde_json::from_str(inv_str) {
        Ok(v) => v,
        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
    };
    let mut expected_inv_jcs = match assay_canonical::jcs::to_vec(&inv_val) {
        Ok(b) => b,
        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
    };
    expected_inv_jcs.push(b'\n');
    if expected_inv_jcs != container.inventory.bytes {
        return IncidentVerifyReport::refusal(IncidentReason::InputShape);
    }

    let ass_str = match std::str::from_utf8(&container.assessment.bytes) {
        Ok(s) => s,
        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
    };
    if ass_str.starts_with('\u{feff}') {
        return IncidentVerifyReport::refusal(IncidentReason::InputShape);
    }
    if validate_json_strict(ass_str).is_err() {
        return IncidentVerifyReport::refusal(IncidentReason::InputShape);
    }
    let ass_val: serde_json::Value = match serde_json::from_str(ass_str) {
        Ok(v) => v,
        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
    };
    let mut expected_ass_jcs = match assay_canonical::jcs::to_vec(&ass_val) {
        Ok(b) => b,
        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
    };
    expected_ass_jcs.push(b'\n');
    if expected_ass_jcs != container.assessment.bytes {
        return IncidentVerifyReport::refusal(IncidentReason::InputShape);
    }

    // Closed structural grammar deserialization
    let inventory: InventoryDocument = match serde_json::from_value(inv_val) {
        Ok(d) => d,
        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
    };
    let assessment: AssessmentDocument = match serde_json::from_value(ass_val) {
        Ok(d) => d,
        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::InputShape),
    };

    // Closed vocabulary and structural checks in Phase 2
    const VALID_OBSERVATIONS: &[&str] = &["unknown", "limited", "adequate"];
    for s in &assessment.surfaces {
        if !VALID_SURFACES.contains(&s.name.as_str()) {
            return IncidentVerifyReport::refusal(IncidentReason::InputShape);
        }
        if !VALID_OBSERVATIONS.contains(&s.observation.as_str()) {
            return IncidentVerifyReport::refusal(IncidentReason::InputShape);
        }
        if let Some(w) = &s.window {
            let (Some(t_start), Some(t_end)) =
                (parse_utc_timestamp(&w.start), parse_utc_timestamp(&w.end))
            else {
                return IncidentVerifyReport::refusal(IncidentReason::InputShape);
            };
            if t_start > t_end {
                return IncidentVerifyReport::refusal(IncidentReason::InputShape);
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
            return IncidentVerifyReport::refusal(IncidentReason::InputShape);
        }
        if !VALID_DISPOSITIONS.contains(&u.disposition.as_str()) {
            return IncidentVerifyReport::refusal(IncidentReason::InputShape);
        }
    }

    // Binding shape checks in Phase 2
    for input in &inventory.inputs {
        if input.format != "dsse-attestation" && input.format != "attestation-report" {
            if input.binding.is_some() {
                return IncidentVerifyReport::refusal(IncidentReason::InputShape);
            }
        } else if input.format == "dsse-attestation" {
            let Some(b) = &input.binding else {
                return IncidentVerifyReport::refusal(IncidentReason::InputShape);
            };
            if b.attestation_input.is_some() {
                return IncidentVerifyReport::refusal(IncidentReason::InputShape);
            }
        } else if input.format == "attestation-report" {
            let Some(b) = &input.binding else {
                return IncidentVerifyReport::refusal(IncidentReason::InputShape);
            };
            if b.attestation_input.is_none() {
                return IncidentVerifyReport::refusal(IncidentReason::InputShape);
            }
        }
    }

    // Phase 3: unknown_format, unknown_schema, mapping
    if inventory.schema != "assay.incident.inventory.v1" {
        return IncidentVerifyReport::refusal(IncidentReason::UnknownSchema);
    }
    if assessment.schema != "assay.incident.assessment.v1" {
        return IncidentVerifyReport::refusal(IncidentReason::UnknownSchema);
    }

    for input in &inventory.inputs {
        if !ADMITTED_FORMATS.contains(&input.format.as_str()) {
            return IncidentVerifyReport::refusal(IncidentReason::UnknownFormat);
        }
        match input.format.as_str() {
            "proxy-decision-ndjson" => {
                if input.surfaces != ["tool_call"] {
                    return IncidentVerifyReport::refusal(IncidentReason::Mapping);
                }
            }
            "inspector-protocol-793d103" => {
                if input.surfaces != ["transcript_join"] {
                    return IncidentVerifyReport::refusal(IncidentReason::Mapping);
                }
            }
            "assay-bundle-v1" => {
                let allowed = ["tool_call", "filesystem", "network", "process"];
                if input
                    .surfaces
                    .iter()
                    .any(|s| !allowed.contains(&s.as_str()))
                {
                    return IncidentVerifyReport::refusal(IncidentReason::Mapping);
                }
            }
            _ => {
                if !input.surfaces.is_empty() {
                    return IncidentVerifyReport::refusal(IncidentReason::Mapping);
                }
            }
        }
    }

    // Phase 4: digest, policy_binding
    let actual_assessment_sha = hex::encode(Sha256::digest(&container.assessment.bytes));
    if inventory.assessment_sha256 != actual_assessment_sha {
        return IncidentVerifyReport::refusal(IncidentReason::Digest);
    }

    let total_object_bytes: u64 = inventory.inputs.iter().map(|i| i.bytes).sum();
    if total_object_bytes > ContextLimits::HARD.object_bytes_total {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceHard);
    }
    if total_object_bytes > context.limits.object_bytes_total {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceSelected);
    }
    if inventory.inputs.len() as u64 > ContextLimits::HARD.inputs {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceHard);
    }
    if inventory.inputs.len() as u64 > context.limits.inputs {
        return IncidentVerifyReport::refusal(IncidentReason::ResourceSelected);
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

    // Phase 5: locator, input_shape (locators and source object syntax)
    let inputs_by_id: BTreeMap<&str, &InventoryInput> = inventory
        .inputs
        .iter()
        .map(|i| (i.id.as_str(), i))
        .collect();

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

            let verify_limits = VerifyLimits {
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
                max_events: context.limits.records.min(ContextLimits::HARD.records) as usize,
                max_line_bytes: context
                    .limits
                    .record_bytes
                    .min(ContextLimits::HARD.record_bytes) as usize,
                max_path_len: context
                    .limits
                    .inner_path_bytes
                    .min(ContextLimits::HARD.inner_path_bytes)
                    as usize,
                max_json_depth: context
                    .limits
                    .json_depth
                    .min(ContextLimits::HARD.json_depth) as usize,
            };

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
                    let res_doc: serde_json::Value = match serde_json::from_slice(res_obj_bytes) {
                        Ok(v) => v,
                        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::Disclosure),
                    };
                    if !target.locator.starts_with("json:/results/") {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    }
                    let idx_str = &target.locator[14..];
                    let idx: usize = match idx_str.parse() {
                        Ok(n) => n,
                        Err(_) => return IncidentVerifyReport::refusal(IncidentReason::Disclosure),
                    };
                    let Some(results_arr) = res_doc.get("results").and_then(|r| r.as_array())
                    else {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    };
                    if idx >= results_arr.len() {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    }
                    let target_res: ExaminationResult =
                        match serde_json::from_value(results_arr[idx].clone()) {
                            Ok(r) => r,
                            Err(_) => {
                                return IncidentVerifyReport::refusal(IncidentReason::Disclosure)
                            }
                        };
                    if target_res.source != unit.source {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    }
                }
                Some(UnitDisclosure::Inline { value }) => {
                    if value.source != unit.source {
                        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
                    }
                }
            }
        } else if unit.disposition == "withheld" {
            if unit.result.is_some() || unit.withheld_digest.is_none() {
                return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
            }
        } else if unit.result.is_some() || unit.withheld_digest.is_some() {
            return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
        }
    }

    // Complete enumeration of eligible units from admitted inputs
    let mut total_eligible_units = 0usize;
    for input in &inventory.inputs {
        if input.format == "inspector-protocol-793d103" {
            let obj_path = format!("objects/{}", input.sha256);
            let obj_bytes = object_map.get(obj_path.as_str()).expect("verified present");
            if let Ok(arr) = serde_json::from_slice::<Vec<serde_json::Value>>(obj_bytes) {
                for entry in &arr {
                    if entry.get("direction").and_then(|d| d.as_str()) == Some("request")
                        && entry.get("origin").and_then(|o| o.as_str()) == Some("client")
                        && entry
                            .get("message")
                            .and_then(|m| m.get("method"))
                            .and_then(|m| m.as_str())
                            == Some("tools/call")
                    {
                        total_eligible_units += 1;
                    }
                }
            }
        }
    }
    if assessment.units.len() != total_eligible_units {
        return IncidentVerifyReport::refusal(IncidentReason::Disclosure);
    }

    // Phase 8: cap1_shape, cap1_rules
    let present_surfaces: Vec<&str> = assessment
        .surfaces
        .iter()
        .filter(|s| !s.source_inputs.is_empty())
        .map(|s| s.name.as_str())
        .collect();

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
            }
        }
    }

    // Phase 9: claim_boundary
    let surfaces_by_name: BTreeMap<&str, &SurfaceResult> = assessment
        .surfaces
        .iter()
        .map(|s| (s.name.as_str(), s))
        .collect();

    for claim in &assessment.claims {
        if claim.get("decision").and_then(|d| d.as_str()) == Some("supported") {
            let surface_name = claim.get("surface").and_then(|s| s.as_str()).unwrap_or("");
            let Some(surface) = surfaces_by_name.get(surface_name) else {
                return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
            };
            let kind = claim.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            if matches!(kind, "bounded_negative" | "exhaustive_set")
                && surface.observation != "adequate"
            {
                return IncidentVerifyReport::refusal(IncidentReason::ClaimBoundary);
            }
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
        let expected_sources: Vec<String> = inventory
            .inputs
            .iter()
            .filter(|i| i.surfaces.contains(&s.name))
            .map(|i| i.id.clone())
            .collect();
        if s.source_inputs != expected_sources {
            return IncidentVerifyReport::refusal(IncidentReason::StaleAssessment);
        }
        if s.source_inputs.is_empty()
            && (s.observation != "unknown" || !s.basis_refs.is_empty() || s.window.is_some())
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
