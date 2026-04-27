use crate::bundle::{BundleReader, VerifyLimits};
use crate::lint::engine::{lint_bundle_with_options, LintOptions, LintReportWithPacks};
use crate::types::EvidenceEvent;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

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

/// Diff two Trust Basis artifacts by stable claim identity.
///
/// The CLI rejects duplicate claim IDs before calling this function. Library
/// callers still get deterministic output if duplicate IDs are present: the
/// first claim for each ID wins, and duplicate IDs are not repeated in diff
/// arrays.
pub fn diff_trust_basis(baseline: &TrustBasis, candidate: &TrustBasis) -> TrustBasisDiffReport {
    let baseline_by_id = first_claim_by_id(baseline);
    let candidate_by_id = first_claim_by_id(candidate);

    let mut regressed_claims = Vec::new();
    let mut improved_claims = Vec::new();
    let mut removed_claims = Vec::new();
    let mut metadata_changes = Vec::new();
    let mut unchanged_claim_count = 0;
    let mut seen_candidate_ids = HashSet::new();

    for baseline_claim in baseline_by_id.values() {
        let Some(candidate_claim) = candidate_by_id.get(&baseline_claim.id).copied() else {
            removed_claims.push(presence_diff_removed(baseline_claim));
            continue;
        };
        seen_candidate_ids.insert(candidate_claim.id);

        let baseline_rank = trust_claim_level_rank(baseline_claim.level);
        let candidate_rank = trust_claim_level_rank(candidate_claim.level);
        if candidate_rank < baseline_rank {
            regressed_claims.push(TrustBasisClaimLevelDiff {
                diff_class: TrustBasisDiffClass::Regressed,
                claim_id: baseline_claim.id,
                baseline_level: baseline_claim.level,
                candidate_level: candidate_claim.level,
            });
        } else if candidate_rank > baseline_rank {
            improved_claims.push(TrustBasisClaimLevelDiff {
                diff_class: TrustBasisDiffClass::Improved,
                claim_id: baseline_claim.id,
                baseline_level: baseline_claim.level,
                candidate_level: candidate_claim.level,
            });
        } else if baseline_claim.source != candidate_claim.source
            || baseline_claim.boundary != candidate_claim.boundary
            || baseline_claim.note != candidate_claim.note
        {
            metadata_changes.push(TrustBasisClaimMetadataDiff {
                diff_class: TrustBasisDiffClass::MetadataChanged,
                claim_id: baseline_claim.id,
                baseline_level: baseline_claim.level,
                candidate_level: candidate_claim.level,
                baseline_source: baseline_claim.source,
                candidate_source: candidate_claim.source,
                baseline_boundary: baseline_claim.boundary,
                candidate_boundary: candidate_claim.boundary,
                note_changed: baseline_claim.note != candidate_claim.note,
            });
        } else {
            unchanged_claim_count += 1;
        }
    }

    let mut added_claims: Vec<TrustBasisClaimPresenceDiff> = candidate_by_id
        .values()
        .copied()
        .filter(|claim| !seen_candidate_ids.contains(&claim.id))
        .map(presence_diff_added)
        .collect();

    regressed_claims.sort_by_key(|diff| claim_id_sort_key(diff.claim_id));
    improved_claims.sort_by_key(|diff| claim_id_sort_key(diff.claim_id));
    removed_claims.sort_by_key(|diff| claim_id_sort_key(diff.claim_id));
    added_claims.sort_by_key(|diff| claim_id_sort_key(diff.claim_id));
    metadata_changes.sort_by_key(|diff| claim_id_sort_key(diff.claim_id));

    let summary = TrustBasisDiffSummary {
        regressed_claims: regressed_claims.len(),
        improved_claims: improved_claims.len(),
        removed_claims: removed_claims.len(),
        added_claims: added_claims.len(),
        metadata_changes: metadata_changes.len(),
        unchanged_claim_count,
        has_regressions: !regressed_claims.is_empty() || !removed_claims.is_empty(),
    };

    TrustBasisDiffReport {
        schema: TRUST_BASIS_DIFF_SCHEMA.to_string(),
        claim_identity: "claim.id".to_string(),
        level_order: vec![
            TrustClaimLevel::Absent,
            TrustClaimLevel::Inferred,
            TrustClaimLevel::SelfReported,
            TrustClaimLevel::Verified,
        ],
        summary,
        regressed_claims,
        improved_claims,
        removed_claims,
        added_claims,
        metadata_changes,
        unchanged_claim_count,
    }
}

fn first_claim_by_id(trust_basis: &TrustBasis) -> HashMap<TrustClaimId, &TrustBasisClaim> {
    let mut by_id = HashMap::new();
    for claim in &trust_basis.claims {
        by_id.entry(claim.id).or_insert(claim);
    }
    by_id
}

fn presence_diff_removed(claim: &TrustBasisClaim) -> TrustBasisClaimPresenceDiff {
    TrustBasisClaimPresenceDiff {
        diff_class: TrustBasisDiffClass::Removed,
        claim_id: claim.id,
        baseline_level: Some(claim.level),
        candidate_level: None,
        baseline_source: Some(claim.source),
        candidate_source: None,
        baseline_boundary: Some(claim.boundary),
        candidate_boundary: None,
        baseline_note: claim.note.clone(),
        candidate_note: None,
    }
}

fn presence_diff_added(claim: &TrustBasisClaim) -> TrustBasisClaimPresenceDiff {
    TrustBasisClaimPresenceDiff {
        diff_class: TrustBasisDiffClass::Added,
        claim_id: claim.id,
        baseline_level: None,
        candidate_level: Some(claim.level),
        baseline_source: None,
        candidate_source: Some(claim.source),
        baseline_boundary: None,
        candidate_boundary: Some(claim.boundary),
        baseline_note: None,
        candidate_note: claim.note.clone(),
    }
}

pub fn duplicate_trust_basis_claim_ids(trust_basis: &TrustBasis) -> Vec<TrustClaimId> {
    let mut seen = HashSet::new();
    let mut duplicates: Vec<TrustClaimId> = trust_basis
        .claims
        .iter()
        .filter_map(|claim| {
            if seen.insert(claim.id) {
                None
            } else {
                Some(claim.id)
            }
        })
        .collect();
    duplicates.sort_by_key(|id| claim_id_sort_key(*id));
    duplicates.dedup();
    duplicates
}

fn claim_id_sort_key(id: TrustClaimId) -> String {
    match serde_json::to_value(id).expect("TrustClaimId serialization should succeed") {
        serde_json::Value::String(value) => value,
        _ => unreachable!("TrustClaimId should serialize as a string"),
    }
}

fn trust_claim_level_rank(level: TrustClaimLevel) -> u8 {
    match level {
        TrustClaimLevel::Absent => 0,
        TrustClaimLevel::Inferred => 1,
        TrustClaimLevel::SelfReported => 2,
        TrustClaimLevel::Verified => 3,
    }
}

pub fn generate_trust_basis<R: Read>(
    reader: R,
    limits: VerifyLimits,
    options: TrustBasisOptions,
) -> Result<TrustBasis> {
    let mut bundle_data = Vec::new();
    let mut limited_reader = reader.take(limits.max_bundle_bytes.saturating_add(1));
    limited_reader.read_to_end(&mut bundle_data)?;
    if bundle_data.len() as u64 > limits.max_bundle_bytes {
        bail!(
            "trust basis bundle exceeds compressed input limit of {} bytes",
            limits.max_bundle_bytes
        );
    }

    let bundle_reader = BundleReader::open_with_limits(Cursor::new(&bundle_data), limits)?;
    let events = bundle_reader.events_vec()?;

    let lint_result = match options.lint {
        Some(lint_options) if !lint_options.packs.is_empty() => Some(lint_bundle_with_options(
            Cursor::new(&bundle_data),
            limits,
            lint_options,
        )?),
        _ => None,
    };

    Ok(TrustBasis {
        claims: vec![
            TrustBasisClaim {
                id: TrustClaimId::BundleVerified,
                level: TrustClaimLevel::Verified,
                source: TrustClaimSource::BundleVerification,
                boundary: TrustClaimBoundary::BundleWide,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::SigningEvidencePresent,
                level: classify_signing_evidence(&bundle_reader),
                source: TrustClaimSource::BundleProofSurface,
                boundary: TrustClaimBoundary::ProofSurfacesOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::ProvenanceBackedClaimsPresent,
                level: classify_provenance_evidence(&bundle_reader),
                source: TrustClaimSource::BundleProofSurface,
                boundary: TrustClaimBoundary::ProofSurfacesOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::DelegationContextVisible,
                level: classify_delegation_context(&events),
                source: TrustClaimSource::CanonicalDecisionEvidence,
                boundary: TrustClaimBoundary::SupportedDelegatedFlowsOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::AuthorizationContextVisible,
                level: classify_authorization_context(&events),
                source: TrustClaimSource::CanonicalDecisionEvidence,
                boundary: TrustClaimBoundary::SupportedAuthProjectedFlowsOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::ContainmentDegradationObserved,
                level: classify_containment_degradation(&events),
                source: TrustClaimSource::CanonicalEventPresence,
                boundary: TrustClaimBoundary::SupportedContainmentFallbackPathsOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                level: classify_external_eval_receipt_boundary(&events),
                source: TrustClaimSource::ExternalEvidenceReceipt,
                boundary: TrustClaimBoundary::SupportedExternalEvalReceiptEventsOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::AppliedPackFindingsPresent,
                level: classify_pack_findings(lint_result.as_ref()),
                source: TrustClaimSource::PackExecutionResults,
                boundary: TrustClaimBoundary::PackExecutionOnly,
                note: None,
            },
        ],
    })
}

pub fn to_canonical_json_bytes(trust_basis: &TrustBasis) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut output, formatter);
    trust_basis.serialize(&mut serializer)?;
    output.push(b'\n');
    Ok(output)
}

fn classify_signing_evidence(_bundle_reader: &BundleReader) -> TrustClaimLevel {
    // T1a v1 stays conservative: ordinary evidence bundles do not yet carry a
    // dedicated signed proof surface for runtime trust claims.
    TrustClaimLevel::Absent
}

fn classify_provenance_evidence(_bundle_reader: &BundleReader) -> TrustClaimLevel {
    // T1a v1 stays conservative: ordinary evidence bundles do not yet carry a
    // dedicated provenance-proof surface strong enough for this claim.
    TrustClaimLevel::Absent
}

fn classify_delegation_context(events: &[EvidenceEvent]) -> TrustClaimLevel {
    let has_supported_delegation = events.iter().any(|event| {
        event.type_ == "assay.tool.decision"
            && event
                .payload
                .get("delegated_from")
                .and_then(|value| value.as_str())
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
    });

    if has_supported_delegation {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

fn classify_authorization_context(events: &[EvidenceEvent]) -> TrustClaimLevel {
    if crate::g3_authorization_context::bundle_satisfies_g3_authorization_context_visible(events) {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

fn classify_containment_degradation(events: &[EvidenceEvent]) -> TrustClaimLevel {
    if events
        .iter()
        .any(|event| event.type_ == "assay.sandbox.degraded")
    {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

const PROMPTFOO_RECEIPT_EVENT_TYPE: &str = "assay.receipt.promptfoo.assertion_component.v1";
const PROMPTFOO_RECEIPT_SCHEMA: &str = "assay.receipt.promptfoo.assertion-component.v1";
const PROMPTFOO_RECEIPT_SOURCE_SYSTEM: &str = "promptfoo";
const PROMPTFOO_RECEIPT_SOURCE_SURFACE: &str = "cli-jsonl.gradingResult.componentResults";
const PROMPTFOO_RECEIPT_REDUCER_PREFIX: &str = "assay-promptfoo-jsonl-component-result@";
const PROMPTFOO_MAX_REASON_CHARS: usize = 160;

fn classify_external_eval_receipt_boundary(events: &[EvidenceEvent]) -> TrustClaimLevel {
    if events.iter().any(is_supported_promptfoo_receipt) {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

fn is_supported_promptfoo_receipt(event: &EvidenceEvent) -> bool {
    if event.type_ != PROMPTFOO_RECEIPT_EVENT_TYPE {
        return false;
    }

    let Some(payload) = event.payload.as_object() else {
        return false;
    };
    let allowed_fields = [
        "schema",
        "source_system",
        "source_surface",
        "source_artifact_ref",
        "source_artifact_digest",
        "reducer_version",
        "imported_at",
        "assertion_type",
        "result",
    ];
    if payload
        .keys()
        .any(|key| !allowed_fields.contains(&key.as_str()))
    {
        return false;
    }

    string_field(payload, "schema") == Some(PROMPTFOO_RECEIPT_SCHEMA)
        && string_field(payload, "source_system") == Some(PROMPTFOO_RECEIPT_SOURCE_SYSTEM)
        && string_field(payload, "source_surface") == Some(PROMPTFOO_RECEIPT_SOURCE_SURFACE)
        && non_empty_string_field(payload, "source_artifact_ref")
        && string_field(payload, "source_artifact_digest")
            .map(is_sha256_digest)
            .unwrap_or(false)
        && string_field(payload, "reducer_version")
            .map(|value| value.starts_with(PROMPTFOO_RECEIPT_REDUCER_PREFIX))
            .unwrap_or(false)
        && string_field(payload, "imported_at")
            .map(is_utc_rfc3339)
            .unwrap_or(false)
        && string_field(payload, "assertion_type") == Some("equals")
        && payload
            .get("result")
            .and_then(|value| value.as_object())
            .map(is_supported_promptfoo_result)
            .unwrap_or(false)
}

fn string_field<'a>(
    payload: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    payload.get(key).and_then(|value| value.as_str())
}

fn non_empty_string_field(payload: &serde_json::Map<String, serde_json::Value>, key: &str) -> bool {
    string_field(payload, key)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn is_sha256_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_utc_rfc3339(value: &str) -> bool {
    let Ok(timestamp) = chrono::DateTime::parse_from_rfc3339(value) else {
        return false;
    };
    timestamp.offset().local_minus_utc() == 0
}

fn is_supported_promptfoo_result(result: &serde_json::Map<String, serde_json::Value>) -> bool {
    let allowed_fields = ["pass", "score", "reason"];
    if result
        .keys()
        .any(|key| !allowed_fields.contains(&key.as_str()))
    {
        return false;
    }

    if result
        .get("pass")
        .and_then(|value| value.as_bool())
        .is_none()
    {
        return false;
    }

    if !matches!(
        result.get("score").and_then(|value| value.as_i64()),
        Some(0 | 1)
    ) {
        return false;
    }

    match result.get("reason") {
        Some(value) => value.as_str().map(is_bounded_reason).unwrap_or(false),
        None => true,
    }
}

fn is_bounded_reason(reason: &str) -> bool {
    let trimmed = reason.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= PROMPTFOO_MAX_REASON_CHARS
        && !trimmed.contains('\n')
        && !trimmed.contains('\r')
        && !trimmed.contains('"')
        && !trimmed.contains('`')
        && !trimmed.contains('{')
        && !trimmed.contains('}')
}

fn classify_pack_findings(lint_result: Option<&LintReportWithPacks>) -> TrustClaimLevel {
    let Some(lint_result) = lint_result else {
        return TrustClaimLevel::Absent;
    };

    let Some(pack_meta) = lint_result.pack_meta.as_ref() else {
        return TrustClaimLevel::Absent;
    };

    let prefixes: Vec<String> = pack_meta
        .packs
        .iter()
        .map(|pack| format!("{}@{}:", pack.name, pack.version))
        .collect();

    let has_pack_finding = lint_result.report.findings.iter().any(|finding| {
        prefixes
            .iter()
            .any(|prefix| finding.rule_id.starts_with(prefix))
    });

    if has_pack_finding {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::BundleWriter;
    use crate::lint::packs::load_pack;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn make_event(
        type_: &str,
        run_id: &str,
        seq: u64,
        payload: serde_json::Value,
    ) -> EvidenceEvent {
        let mut event =
            EvidenceEvent::new(type_, "urn:assay:test:trust-basis", run_id, seq, payload);
        event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
        event
    }

    fn make_bundle(events: Vec<EvidenceEvent>) -> Vec<u8> {
        let mut buffer = Vec::new();
        let mut writer = BundleWriter::new(&mut buffer);
        for event in events {
            writer.add_event(event);
        }
        writer.finish().expect("bundle should finish");
        buffer
    }

    fn claim(trust_basis: &TrustBasis, id: TrustClaimId) -> &TrustBasisClaim {
        trust_basis
            .claims
            .iter()
            .find(|claim| claim.id == id)
            .expect("claim should exist")
    }

    fn trust_basis_claim(
        id: TrustClaimId,
        level: TrustClaimLevel,
        note: Option<&str>,
    ) -> TrustBasisClaim {
        TrustBasisClaim {
            id,
            level,
            source: TrustClaimSource::ExternalEvidenceReceipt,
            boundary: TrustClaimBoundary::SupportedExternalEvalReceiptEventsOnly,
            note: note.map(str::to_string),
        }
    }

    fn promptfoo_receipt_payload(extra: serde_json::Value) -> serde_json::Value {
        let mut payload = json!({
            "schema": "assay.receipt.promptfoo.assertion-component.v1",
            "source_system": "promptfoo",
            "source_surface": "cli-jsonl.gradingResult.componentResults",
            "source_artifact_ref": "results.jsonl",
            "source_artifact_digest": format!("sha256:{}", "a".repeat(64)),
            "reducer_version": "assay-promptfoo-jsonl-component-result@0.1.0",
            "imported_at": "2026-04-26T12:00:00Z",
            "assertion_type": "equals",
            "result": {
                "pass": true,
                "score": 1,
                "reason": "Assertion passed"
            }
        });
        if let Some(extra) = extra.as_object() {
            let obj = payload.as_object_mut().expect("payload object");
            for (key, value) in extra {
                obj.insert(key.clone(), value.clone());
            }
        }
        payload
    }

    #[test]
    fn g3_authorization_claim_is_after_delegation_before_containment() {
        let bundle = make_bundle(vec![make_event(
            "assay.process.exec",
            "run_g3_order",
            0,
            json!({ "hits": 1 }),
        )]);
        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis");
        let ids: Vec<_> = trust_basis.claims.iter().map(|c| c.id).collect();
        let pos = |id| ids.iter().position(|&x| x == id).expect("claim id");
        assert_eq!(ids.len(), 8);
        assert!(
            pos(TrustClaimId::DelegationContextVisible)
                < pos(TrustClaimId::AuthorizationContextVisible)
        );
        assert!(
            pos(TrustClaimId::AuthorizationContextVisible)
                < pos(TrustClaimId::ContainmentDegradationObserved)
        );
    }

    #[test]
    fn trust_basis_always_emits_all_frozen_claims() {
        let bundle = make_bundle(vec![make_event(
            "assay.process.exec",
            "run_all_claims",
            0,
            json!({ "hits": 1 }),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            trust_basis
                .claims
                .iter()
                .map(|claim| (claim.id, claim.source, claim.boundary))
                .collect::<Vec<_>>(),
            vec![
                (
                    TrustClaimId::BundleVerified,
                    TrustClaimSource::BundleVerification,
                    TrustClaimBoundary::BundleWide,
                ),
                (
                    TrustClaimId::SigningEvidencePresent,
                    TrustClaimSource::BundleProofSurface,
                    TrustClaimBoundary::ProofSurfacesOnly,
                ),
                (
                    TrustClaimId::ProvenanceBackedClaimsPresent,
                    TrustClaimSource::BundleProofSurface,
                    TrustClaimBoundary::ProofSurfacesOnly,
                ),
                (
                    TrustClaimId::DelegationContextVisible,
                    TrustClaimSource::CanonicalDecisionEvidence,
                    TrustClaimBoundary::SupportedDelegatedFlowsOnly,
                ),
                (
                    TrustClaimId::AuthorizationContextVisible,
                    TrustClaimSource::CanonicalDecisionEvidence,
                    TrustClaimBoundary::SupportedAuthProjectedFlowsOnly,
                ),
                (
                    TrustClaimId::ContainmentDegradationObserved,
                    TrustClaimSource::CanonicalEventPresence,
                    TrustClaimBoundary::SupportedContainmentFallbackPathsOnly,
                ),
                (
                    TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                    TrustClaimSource::ExternalEvidenceReceipt,
                    TrustClaimBoundary::SupportedExternalEvalReceiptEventsOnly,
                ),
                (
                    TrustClaimId::AppliedPackFindingsPresent,
                    TrustClaimSource::PackExecutionResults,
                    TrustClaimBoundary::PackExecutionOnly,
                ),
            ]
        );
        assert_eq!(
            trust_basis
                .claims
                .iter()
                .map(|claim| claim.level)
                .collect::<Vec<_>>(),
            vec![
                TrustClaimLevel::Verified,
                TrustClaimLevel::Absent,
                TrustClaimLevel::Absent,
                TrustClaimLevel::Absent,
                TrustClaimLevel::Absent,
                TrustClaimLevel::Absent,
                TrustClaimLevel::Absent,
                TrustClaimLevel::Absent,
            ]
        );
    }

    #[test]
    fn trust_basis_regeneration_is_byte_stable() {
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_stable",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "delegated_from": "agent:planner"
            }),
        )]);

        let first = generate_trust_basis(
            Cursor::new(&bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("first trust basis");
        let second = generate_trust_basis(
            Cursor::new(&bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("second trust basis");

        assert_eq!(
            to_canonical_json_bytes(&first).expect("first json"),
            to_canonical_json_bytes(&second).expect("second json")
        );
    }

    #[test]
    fn trust_basis_detects_supported_delegation_and_degradation() {
        let bundle = make_bundle(vec![
            make_event(
                "assay.tool.decision",
                "run_signals",
                0,
                json!({
                    "tool": "tool.commit",
                    "decision": "allow",
                    "delegated_from": "agent:planner"
                }),
            ),
            make_event(
                "assay.sandbox.degraded",
                "run_signals",
                1,
                json!({
                    "reason_code": "policy_conflict",
                    "degradation_mode": "audit_fallback",
                    "component": "landlock"
                }),
            ),
        ]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(&trust_basis, TrustClaimId::DelegationContextVisible).level,
            TrustClaimLevel::Verified
        );
        assert_eq!(
            claim(&trust_basis, TrustClaimId::ContainmentDegradationObserved).level,
            TrustClaimLevel::Verified
        );
        assert_eq!(
            claim(&trust_basis, TrustClaimId::AuthorizationContextVisible).level,
            TrustClaimLevel::Absent
        );
    }

    #[test]
    fn trust_basis_detects_supported_external_eval_receipt_boundary() {
        let bundle = make_bundle(vec![make_event(
            "assay.receipt.promptfoo.assertion_component.v1",
            "run_promptfoo_receipt",
            0,
            promptfoo_receipt_payload(json!({})),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(
                &trust_basis,
                TrustClaimId::ExternalEvalReceiptBoundaryVisible
            )
            .level,
            TrustClaimLevel::Verified
        );
    }

    #[test]
    fn trust_basis_rejects_promptfoo_receipt_boundary_when_raw_payload_leaks_in() {
        let bundle = make_bundle(vec![make_event(
            "assay.receipt.promptfoo.assertion_component.v1",
            "run_promptfoo_raw",
            0,
            promptfoo_receipt_payload(json!({
                "output": "raw model output should not be in the receipt"
            })),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(
                &trust_basis,
                TrustClaimId::ExternalEvalReceiptBoundaryVisible
            )
            .level,
            TrustClaimLevel::Absent
        );
    }

    #[test]
    fn trust_basis_rejects_promptfoo_receipt_boundary_when_import_time_is_not_utc_rfc3339() {
        let bundle = make_bundle(vec![make_event(
            "assay.receipt.promptfoo.assertion_component.v1",
            "run_promptfoo_bad_time",
            0,
            promptfoo_receipt_payload(json!({
                "imported_at": "not-a-timestamp"
            })),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(
                &trust_basis,
                TrustClaimId::ExternalEvalReceiptBoundaryVisible
            )
            .level,
            TrustClaimLevel::Absent
        );
    }

    #[test]
    fn trust_basis_detects_g3_authorization_context_when_all_fields_present() {
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_g3",
            0,
            json!({
                "tool": "t",
                "decision": "allow",
                "principal": "alice@example.com",
                "auth_scheme": "jwt_bearer",
                "auth_issuer": "https://issuer.example/"
            }),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(&trust_basis, TrustClaimId::AuthorizationContextVisible).level,
            TrustClaimLevel::Verified
        );
    }

    #[test]
    fn trust_basis_g3_absent_when_principal_whitespace_only() {
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_g3_ws",
            0,
            json!({
                "principal": "   \n\t  ",
                "auth_scheme": "jwt_bearer",
                "auth_issuer": "https://issuer.example/"
            }),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(&trust_basis, TrustClaimId::AuthorizationContextVisible).level,
            TrustClaimLevel::Absent
        );
    }

    #[test]
    fn trust_basis_g3_absent_when_auth_issuer_jws_shaped_or_principal_bearer() {
        let jws = "eyJxxxxxxxxxxxxxxxxxxxx.yyyyyyyyyyyyyyyyyyyyyyyy.zzzzzzzzzzzzzzzzzzzzzzzz";
        let bundle_jws_iss = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_g3_jws_iss",
            0,
            json!({
                "principal": "alice",
                "auth_scheme": "oauth2",
                "auth_issuer": jws
            }),
        )]);
        let tb1 = generate_trust_basis(
            Cursor::new(bundle_jws_iss),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis");
        assert_eq!(
            claim(&tb1, TrustClaimId::AuthorizationContextVisible).level,
            TrustClaimLevel::Absent
        );

        let bundle_bearer_princ = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_g3_bearer_p",
            0,
            json!({
                "principal": "Bearer leaked-token",
                "auth_scheme": "oauth2",
                "auth_issuer": "https://issuer.example/"
            }),
        )]);
        let tb2 = generate_trust_basis(
            Cursor::new(bundle_bearer_princ),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis");
        assert_eq!(
            claim(&tb2, TrustClaimId::AuthorizationContextVisible).level,
            TrustClaimLevel::Absent
        );
    }

    #[test]
    fn trust_basis_g3_absent_when_auth_issuer_exceeds_cap() {
        let huge_iss = "x".repeat(crate::g3_authorization_context::G3_MAX_AUTH_ISSUER_BYTES + 1);
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_g3_huge_iss",
            0,
            json!({
                "principal": "alice",
                "auth_scheme": "oauth2",
                "auth_issuer": huge_iss
            }),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis");
        assert_eq!(
            claim(&trust_basis, TrustClaimId::AuthorizationContextVisible).level,
            TrustClaimLevel::Absent
        );
    }

    #[test]
    fn trust_basis_keeps_signing_and_provenance_absent_despite_tempting_metadata() {
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_conservative",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "signature": "pretend",
                "provenance": { "claimed": true }
            }),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(&trust_basis, TrustClaimId::SigningEvidencePresent).level,
            TrustClaimLevel::Absent
        );
        assert_eq!(
            claim(&trust_basis, TrustClaimId::ProvenanceBackedClaimsPresent).level,
            TrustClaimLevel::Absent
        );
    }

    #[test]
    fn trust_basis_marks_pack_findings_only_when_explicit_pack_execution_finds_results() {
        let pack = load_pack("owasp-agentic-a3-a5-signal-followup").expect("pack should load");
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_pack_findings",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "principal": "user:alice"
            }),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions {
                lint: Some(LintOptions {
                    packs: vec![pack],
                    max_results: Some(500),
                    bundle_path: Some("trust-basis-pack.tar.gz".to_string()),
                }),
            },
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(&trust_basis, TrustClaimId::AppliedPackFindingsPresent).level,
            TrustClaimLevel::Verified
        );
    }

    #[test]
    fn trust_basis_diff_keys_by_claim_id_and_reports_note_metadata_nonblocking() {
        let baseline = TrustBasis {
            claims: vec![trust_basis_claim(
                TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                TrustClaimLevel::Verified,
                Some("old note"),
            )],
        };
        let candidate = TrustBasis {
            claims: vec![trust_basis_claim(
                TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                TrustClaimLevel::Verified,
                Some("new note"),
            )],
        };

        let report = diff_trust_basis(&baseline, &candidate);

        assert_eq!(report.schema, TRUST_BASIS_DIFF_SCHEMA);
        assert_eq!(report.claim_identity, "claim.id");
        assert!(!report.has_regressions());
        assert!(report.regressed_claims.is_empty());
        assert!(report.removed_claims.is_empty());
        assert_eq!(report.metadata_changes.len(), 1);
        assert_eq!(
            report.metadata_changes[0].claim_id,
            TrustClaimId::ExternalEvalReceiptBoundaryVisible
        );
        assert!(report.metadata_changes[0].note_changed);
    }

    #[test]
    fn trust_basis_diff_sorts_by_serialized_claim_id() {
        let baseline = TrustBasis { claims: vec![] };
        let candidate = TrustBasis {
            claims: vec![
                trust_basis_claim(
                    TrustClaimId::BundleVerified,
                    TrustClaimLevel::Verified,
                    None,
                ),
                trust_basis_claim(
                    TrustClaimId::AppliedPackFindingsPresent,
                    TrustClaimLevel::Verified,
                    None,
                ),
            ],
        };

        let report = diff_trust_basis(&baseline, &candidate);
        let added_ids: Vec<_> = report
            .added_claims
            .iter()
            .map(|diff| diff.claim_id)
            .collect();

        assert_eq!(
            added_ids,
            vec![
                TrustClaimId::AppliedPackFindingsPresent,
                TrustClaimId::BundleVerified,
            ]
        );
    }

    #[test]
    fn trust_basis_diff_dedupes_duplicate_claim_ids_for_library_consumers() {
        let baseline = TrustBasis {
            claims: vec![
                trust_basis_claim(
                    TrustClaimId::BundleVerified,
                    TrustClaimLevel::Verified,
                    None,
                ),
                trust_basis_claim(TrustClaimId::BundleVerified, TrustClaimLevel::Absent, None),
            ],
        };
        let candidate = TrustBasis {
            claims: vec![
                trust_basis_claim(
                    TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                    TrustClaimLevel::Verified,
                    None,
                ),
                trust_basis_claim(
                    TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                    TrustClaimLevel::Absent,
                    None,
                ),
            ],
        };

        let report = diff_trust_basis(&baseline, &candidate);

        assert_eq!(report.removed_claims.len(), 1);
        assert_eq!(
            report.removed_claims[0].claim_id,
            TrustClaimId::BundleVerified
        );
        assert_eq!(report.added_claims.len(), 1);
        assert_eq!(
            report.added_claims[0].claim_id,
            TrustClaimId::ExternalEvalReceiptBoundaryVisible
        );
    }

    #[test]
    fn duplicate_trust_basis_claim_ids_are_reported_deterministically() {
        let trust_basis = TrustBasis {
            claims: vec![
                trust_basis_claim(
                    TrustClaimId::BundleVerified,
                    TrustClaimLevel::Verified,
                    None,
                ),
                trust_basis_claim(
                    TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                    TrustClaimLevel::Verified,
                    None,
                ),
                trust_basis_claim(
                    TrustClaimId::BundleVerified,
                    TrustClaimLevel::Verified,
                    None,
                ),
                trust_basis_claim(
                    TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                    TrustClaimLevel::Verified,
                    None,
                ),
            ],
        };

        assert_eq!(
            duplicate_trust_basis_claim_ids(&trust_basis),
            vec![
                TrustClaimId::BundleVerified,
                TrustClaimId::ExternalEvalReceiptBoundaryVisible,
            ]
        );
    }

    #[test]
    fn trust_basis_respects_max_bundle_bytes_before_verification() {
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_size_limit",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow"
            }),
        )]);

        let err = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits {
                max_bundle_bytes: 8,
                ..VerifyLimits::default()
            },
            TrustBasisOptions::default(),
        )
        .expect_err("trust basis generation should fail when compressed input exceeds limit");

        assert!(
            err.to_string()
                .contains("trust basis bundle exceeds compressed input limit"),
            "unexpected error: {err}"
        );
    }
}
