use super::TrustClaimLevel;
use crate::bundle::BundleReader;
use crate::lint::engine::LintReportWithPacks;
use crate::types::EvidenceEvent;

pub(super) fn classify_signing_evidence(_bundle_reader: &BundleReader) -> TrustClaimLevel {
    // T1a v1 stays conservative: ordinary evidence bundles do not yet carry a
    // dedicated signed proof surface for runtime trust claims.
    TrustClaimLevel::Absent
}

pub(super) fn classify_provenance_evidence(_bundle_reader: &BundleReader) -> TrustClaimLevel {
    // T1a v1 stays conservative: ordinary evidence bundles do not yet carry a
    // dedicated provenance-proof surface strong enough for this claim.
    TrustClaimLevel::Absent
}

pub(super) fn classify_delegation_context(events: &[EvidenceEvent]) -> TrustClaimLevel {
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

pub(super) fn classify_authorization_context(events: &[EvidenceEvent]) -> TrustClaimLevel {
    if crate::g3_authorization_context::bundle_satisfies_g3_authorization_context_visible(events) {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

pub(super) fn classify_containment_degradation(events: &[EvidenceEvent]) -> TrustClaimLevel {
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
pub(super) const SOURCE_ARTIFACT_REF_MAX_CHARS: usize = 240;
const OPENFEATURE_DECISION_RECEIPT_EVENT_TYPE: &str =
    "assay.receipt.openfeature.evaluation_details.v1";
const OPENFEATURE_DECISION_RECEIPT_SCHEMA: &str = "assay.receipt.openfeature.evaluation_details.v1";
const OPENFEATURE_DECISION_RECEIPT_SOURCE_SYSTEM: &str = "openfeature";
const OPENFEATURE_DECISION_RECEIPT_SOURCE_SURFACE: &str = "evaluation_details.boolean";
const OPENFEATURE_DECISION_RECEIPT_REDUCER_PREFIX: &str = "assay-openfeature-evaluation-details@";
const DECISION_FLAG_KEY_MAX_CHARS: usize = 200;
const DECISION_BOUNDARY_STRING_MAX_CHARS: usize = 120;
const CYCLONEDX_MLBOM_MODEL_RECEIPT_EVENT_TYPE: &str =
    "assay.receipt.cyclonedx.mlbom_model_component.v1";
const CYCLONEDX_MLBOM_MODEL_RECEIPT_SCHEMA: &str =
    "assay.receipt.cyclonedx.mlbom-model-component.v1";
const CYCLONEDX_MLBOM_MODEL_RECEIPT_SOURCE_SYSTEM: &str = "cyclonedx";
const CYCLONEDX_MLBOM_MODEL_RECEIPT_SOURCE_SURFACE: &str =
    "bom.components[type=machine-learning-model]";
const CYCLONEDX_MLBOM_MODEL_RECEIPT_REDUCER_PREFIX: &str = "assay-cyclonedx-mlbom-model-component@";
const INVENTORY_BOUNDARY_STRING_MAX_CHARS: usize = 240;
const INVENTORY_REF_MAX_COUNT: usize = 32;

pub(super) fn classify_external_eval_receipt_boundary(events: &[EvidenceEvent]) -> TrustClaimLevel {
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
        && string_field(payload, "source_artifact_ref")
            .map(is_bounded_source_artifact_ref)
            .unwrap_or(false)
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

pub(super) fn classify_external_decision_receipt_boundary(
    events: &[EvidenceEvent],
) -> TrustClaimLevel {
    if events.iter().any(is_supported_openfeature_decision_receipt) {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

fn is_supported_openfeature_decision_receipt(event: &EvidenceEvent) -> bool {
    if event.type_ != OPENFEATURE_DECISION_RECEIPT_EVENT_TYPE {
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
        "decision",
    ];
    if payload
        .keys()
        .any(|key| !allowed_fields.contains(&key.as_str()))
    {
        return false;
    }

    string_field(payload, "schema") == Some(OPENFEATURE_DECISION_RECEIPT_SCHEMA)
        && string_field(payload, "source_system")
            == Some(OPENFEATURE_DECISION_RECEIPT_SOURCE_SYSTEM)
        && string_field(payload, "source_surface")
            == Some(OPENFEATURE_DECISION_RECEIPT_SOURCE_SURFACE)
        && string_field(payload, "source_artifact_ref")
            .map(is_bounded_source_artifact_ref)
            .unwrap_or(false)
        && string_field(payload, "source_artifact_digest")
            .map(is_sha256_digest)
            .unwrap_or(false)
        && string_field(payload, "reducer_version")
            .map(|value| value.starts_with(OPENFEATURE_DECISION_RECEIPT_REDUCER_PREFIX))
            .unwrap_or(false)
        && string_field(payload, "imported_at")
            .map(is_utc_rfc3339)
            .unwrap_or(false)
        && payload
            .get("decision")
            .and_then(|value| value.as_object())
            .map(is_supported_openfeature_decision)
            .unwrap_or(false)
}

pub(super) fn classify_external_inventory_receipt_boundary(
    events: &[EvidenceEvent],
) -> TrustClaimLevel {
    if events
        .iter()
        .any(is_supported_cyclonedx_mlbom_model_receipt)
    {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

fn is_supported_cyclonedx_mlbom_model_receipt(event: &EvidenceEvent) -> bool {
    if event.type_ != CYCLONEDX_MLBOM_MODEL_RECEIPT_EVENT_TYPE {
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
        "model_component",
    ];
    if payload
        .keys()
        .any(|key| !allowed_fields.contains(&key.as_str()))
    {
        return false;
    }

    string_field(payload, "schema") == Some(CYCLONEDX_MLBOM_MODEL_RECEIPT_SCHEMA)
        && string_field(payload, "source_system")
            == Some(CYCLONEDX_MLBOM_MODEL_RECEIPT_SOURCE_SYSTEM)
        && string_field(payload, "source_surface")
            == Some(CYCLONEDX_MLBOM_MODEL_RECEIPT_SOURCE_SURFACE)
        && string_field(payload, "source_artifact_ref")
            .map(is_bounded_source_artifact_ref)
            .unwrap_or(false)
        && string_field(payload, "source_artifact_digest")
            .map(is_sha256_digest)
            .unwrap_or(false)
        && string_field(payload, "reducer_version")
            .map(|value| value.starts_with(CYCLONEDX_MLBOM_MODEL_RECEIPT_REDUCER_PREFIX))
            .unwrap_or(false)
        && string_field(payload, "imported_at")
            .map(is_utc_rfc3339)
            .unwrap_or(false)
        && payload
            .get("model_component")
            .and_then(|value| value.as_object())
            .map(is_supported_cyclonedx_model_component)
            .unwrap_or(false)
}

fn string_field<'a>(
    payload: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    payload.get(key).and_then(|value| value.as_str())
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

fn is_supported_openfeature_decision(
    decision: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    let allowed_fields = [
        "flag_key",
        "value_type",
        "value",
        "variant",
        "reason",
        "error_code",
    ];
    if decision
        .keys()
        .any(|key| !allowed_fields.contains(&key.as_str()))
    {
        return false;
    }

    string_field(decision, "flag_key")
        .map(|value| is_bounded_decision_string(value, DECISION_FLAG_KEY_MAX_CHARS))
        .unwrap_or(false)
        && string_field(decision, "value_type") == Some("boolean")
        && decision
            .get("value")
            .and_then(|value| value.as_bool())
            .is_some()
        && optional_bounded_decision_string_field(decision, "variant")
        && optional_bounded_decision_string_field(decision, "reason")
        && optional_bounded_decision_string_field(decision, "error_code")
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

fn optional_bounded_decision_string_field(
    payload: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> bool {
    match payload.get(key) {
        Some(value) => value
            .as_str()
            .map(|value| is_bounded_decision_string(value, DECISION_BOUNDARY_STRING_MAX_CHARS))
            .unwrap_or(false),
        None => true,
    }
}

fn is_bounded_decision_string(value: &str, max_chars: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed == value
        && value.chars().count() <= max_chars
        && !trimmed.contains('\n')
        && !trimmed.contains('\r')
        && !trimmed.contains('"')
        && !trimmed.contains('`')
        && !trimmed.contains('{')
        && !trimmed.contains('}')
}

fn is_supported_cyclonedx_model_component(
    component: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    let allowed_fields = [
        "bom_ref",
        "name",
        "version",
        "publisher",
        "purl",
        "dataset_refs",
        "model_card_refs",
    ];
    if component
        .keys()
        .any(|key| !allowed_fields.contains(&key.as_str()))
    {
        return false;
    }

    string_field(component, "bom_ref")
        .map(is_bounded_inventory_string)
        .unwrap_or(false)
        && string_field(component, "name")
            .map(is_bounded_inventory_string)
            .unwrap_or(false)
        && optional_bounded_inventory_string_field(component, "version")
        && optional_bounded_inventory_string_field(component, "publisher")
        && optional_bounded_inventory_string_field(component, "purl")
        && optional_inventory_ref_array_field(component, "dataset_refs")
        && optional_inventory_ref_array_field(component, "model_card_refs")
}

fn optional_bounded_inventory_string_field(
    payload: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> bool {
    match payload.get(key) {
        Some(value) => value
            .as_str()
            .map(is_bounded_inventory_string)
            .unwrap_or(false),
        None => true,
    }
}

fn optional_inventory_ref_array_field(
    payload: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> bool {
    match payload.get(key) {
        Some(serde_json::Value::Array(values)) => {
            values.len() <= INVENTORY_REF_MAX_COUNT
                && values.iter().all(|value| {
                    value
                        .as_str()
                        .map(is_bounded_inventory_string)
                        .unwrap_or(false)
                })
        }
        Some(_) => false,
        None => true,
    }
}

fn is_bounded_inventory_string(value: &str) -> bool {
    is_bounded_reviewer_string(value, INVENTORY_BOUNDARY_STRING_MAX_CHARS)
}

fn is_bounded_source_artifact_ref(value: &str) -> bool {
    is_bounded_reviewer_string(value, SOURCE_ARTIFACT_REF_MAX_CHARS)
}

fn is_bounded_reviewer_string(value: &str, max_chars: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed == value
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

pub(super) fn classify_pack_findings(lint_result: Option<&LintReportWithPacks>) -> TrustClaimLevel {
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
