//! Parity guard for Incident Package v1's frozen DecisionEvent admission shape.

#[path = "../../assay-evidence/src/incident_package/decision_wire_v1.rs"]
mod frozen_wire;

use assay_core::mcp::decision::DecisionEvent;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn base() -> Value {
    json!({
        "specversion": "1.0",
        "id": "decision-1",
        "type": "assay.tool.decision",
        "source": "urn:assay:test",
        "time": "2026-10-12T00:00:00Z",
        "data": {
            "tool": "list_files",
            "decision": "allow",
            "reason_code": "allowed",
            "tool_call_id": "call-1"
        }
    })
}

fn verdict(value: Value) -> (bool, bool) {
    let retained: &'static Value = Box::leak(Box::new(value));
    (
        DecisionEvent::deserialize(retained).is_ok(),
        frozen_wire::validate(retained).is_ok(),
    )
}

fn assert_parity(value: Value, expected: bool, label: &str) {
    let (producer, frozen) = verdict(value);
    assert_eq!(producer, expected, "producer: {label}");
    assert_eq!(frozen, expected, "frozen: {label}");
}

#[test]
fn required_fields_and_unknown_fields_have_identical_admission() {
    assert_parity(base(), true, "minimal map");
    for field in ["specversion", "id", "type", "source", "time", "data"] {
        let mut missing = base();
        missing.as_object_mut().unwrap().remove(field);
        assert_parity(missing, false, &format!("missing top-level {field}"));

        let mut null = base();
        null[field] = Value::Null;
        assert_parity(null, false, &format!("null top-level {field}"));
    }
    for field in ["tool", "decision", "reason_code", "tool_call_id"] {
        let mut missing = base();
        missing["data"].as_object_mut().unwrap().remove(field);
        assert_parity(missing, false, &format!("missing data {field}"));

        let mut null = base();
        null["data"][field] = Value::Null;
        assert_parity(null, false, &format!("null data {field}"));
    }

    let mut unknown = base();
    unknown["future_top"] = json!({"nested": [1, true, null]});
    unknown["data"]["future_data"] = json!([{"x": 1}]);
    assert_parity(unknown, true, "unknown fields remain admitted");
}

#[test]
fn every_optional_and_defaulted_field_keeps_its_serde_category() {
    const OPTIONAL_STRINGS: &[&str] = &[
        "match_basis",
        "matched_rule",
        "policy_version",
        "policy_digest",
        "policy_snapshot_digest",
        "policy_snapshot_digest_alg",
        "policy_snapshot_canonicalization",
        "policy_snapshot_schema",
        "tool_definition_digest",
        "tool_definition_digest_alg",
        "tool_definition_canonicalization",
        "tool_definition_schema",
        "tool_definition_source",
        "approval_state",
        "approval_id",
        "approver",
        "issued_at",
        "expires_at",
        "scope",
        "approval_bound_tool",
        "approval_bound_resource",
        "approval_artifact_digest",
        "approval_artifact_digest_alg",
        "approval_artifact_digest_profile",
        "approval_retained_view",
        "approval_failure_reason",
        "scope_type",
        "scope_value",
        "scope_match_mode",
        "scope_evaluation_state",
        "scope_failure_reason",
        "restrict_scope_target",
        "restrict_scope_reason",
        "redaction_target",
        "redaction_mode",
        "redaction_scope",
        "redaction_applied_state",
        "redaction_reason",
        "redaction_failure_reason",
        "redact_args_target",
        "redact_args_mode",
        "redact_args_result",
        "redact_args_reason",
        "decision_basis_version",
        "replay_diff_reason",
        "decision_consumer_contract_version",
        "decision_context_contract_version",
        "deny_precedence_version",
        "deny_convergence_reason",
        "lane",
        "principal",
        "auth_context_summary",
        "auth_scheme",
        "auth_issuer",
        "delegated_from",
        "reason",
        "mandate_id",
        "use_id",
    ];
    const OPTIONAL_BOOLS: &[&str] = &[
        "restrict_scope_present",
        "restrict_scope_match",
        "redact_args_present",
        "compat_fallback_applied",
        "legacy_shape_detected",
        "consumer_fallback_applied",
        "policy_deny",
        "fail_closed_deny",
        "enforcement_deny",
        "deny_legacy_fallback_applied",
        "obligation_applied_present",
        "obligation_skipped_present",
        "obligation_error_present",
        "mandate_scope_match",
        "mandate_kind_match",
        "transaction_ref_match",
    ];
    const OPTIONAL_U32: &[&str] = &["delegation_depth", "use_count"];
    const OPTIONAL_U64: &[&str] = &["authz_latency_ms", "store_latency_ms"];
    const DEFAULT_STRING_VECS: &[&str] = &[
        "tool_classes",
        "matched_tool_classes",
        "required_consumer_fields",
        "required_context_fields",
        "missing_context_fields",
    ];

    for field in OPTIONAL_STRINGS {
        let mut valid = base();
        valid["data"][field] = json!("value");
        assert_parity(valid, true, field);
        let mut null = base();
        null["data"][field] = Value::Null;
        assert_parity(null, true, &format!("null {field}"));
        let mut wrong = base();
        wrong["data"][field] = json!(1);
        assert_parity(wrong, false, &format!("wrong {field}"));
    }
    for field in OPTIONAL_BOOLS {
        let mut valid = base();
        valid["data"][field] = json!(true);
        assert_parity(valid, true, field);
        let mut null = base();
        null["data"][field] = Value::Null;
        assert_parity(null, true, &format!("null {field}"));
        let mut wrong = base();
        wrong["data"][field] = json!("true");
        assert_parity(wrong, false, &format!("wrong {field}"));
    }
    for field in OPTIONAL_U32.iter().chain(OPTIONAL_U64) {
        let mut valid = base();
        valid["data"][field] = json!(1);
        assert_parity(valid, true, field);
        let mut null = base();
        null["data"][field] = Value::Null;
        assert_parity(null, true, &format!("null {field}"));
        for wrong_value in [json!(-1), json!(1.0), json!(true), json!("1")] {
            let mut wrong = base();
            wrong["data"][field] = wrong_value;
            assert_parity(wrong, false, &format!("wrong {field}"));
        }
    }
    for (field, value) in [
        ("use_count", json!(u32::MAX)),
        ("authz_latency_ms", json!(u64::MAX)),
    ] {
        let mut boundary = base();
        boundary["data"][field] = value;
        assert_parity(boundary, true, &format!("numeric width boundary {field}"));
    }
    let mut over_u32 = base();
    over_u32["data"]["use_count"] = json!(u64::from(u32::MAX) + 1);
    assert_parity(over_u32, false, "u32 width exceeded");
    for field in DEFAULT_STRING_VECS {
        let mut valid = base();
        valid["data"][field] = json!(["one"]);
        assert_parity(valid, true, field);
        let mut null = base();
        null["data"][field] = Value::Null;
        assert_parity(null, false, &format!("null {field}"));
        let mut wrong = base();
        wrong["data"][field] = json!([1]);
        assert_parity(wrong, false, &format!("wrong member {field}"));
    }

    let mut retained = base();
    retained["data"]["approval_retention_non_claims"] = json!(["bounded"]);
    assert_parity(retained, true, "optional string vec");
    let mut retained_wrong = base();
    retained_wrong["data"]["approval_retention_non_claims"] = json!([1]);
    assert_parity(retained_wrong, false, "optional string vec wrong member");

    for value in [
        json!(true),
        json!("id"),
        json!(1),
        json!([1]),
        json!({"x": 1}),
    ] {
        let mut request = base();
        request["data"]["request_id"] = value;
        assert_parity(request, true, "request_id arbitrary JSON");
    }
}

#[test]
fn nested_structs_and_all_enum_variants_match_the_producer() {
    let nested = [
        ("typed_decision", json!("allow")),
        ("approval_freshness", json!("fresh")),
        ("decision_outcome_kind", json!("policy_deny")),
        ("decision_origin", json!("policy_engine")),
        ("outcome_compat_state", json!("legacy_fields_preserved")),
        ("fulfillment_decision_path", json!("policy_allow")),
        ("classification_source", json!("converged_outcome")),
        ("consumer_read_path", json!("converged_decision")),
        ("consumer_payload_state", json!("converged")),
        ("context_payload_state", json!("complete_envelope")),
        ("deny_classification_source", json!("outcome_kind")),
    ];
    for (field, value) in nested {
        let mut candidate = base();
        candidate["data"][field] = value;
        assert_parity(candidate, true, field);
        let mut invalid = base();
        invalid["data"][field] = json!("not_a_variant");
        assert_parity(invalid, false, &format!("unknown {field}"));
    }

    let variants: &[(&str, &[&str])] = &[
        ("decision", &["allow", "deny", "error"]),
        (
            "typed_decision",
            &["allow", "allow_with_obligations", "deny", "deny_with_alert"],
        ),
        (
            "approval_freshness",
            &["fresh", "stale", "expired", "unknown"],
        ),
        (
            "decision_outcome_kind",
            &[
                "policy_deny",
                "fail_closed_deny",
                "enforcement_deny",
                "obligation_applied",
                "obligation_skipped",
                "obligation_error",
            ],
        ),
        (
            "decision_origin",
            &[
                "policy_engine",
                "fail_closed_matrix",
                "runtime_enforcement",
                "obligation_executor",
            ],
        ),
        ("outcome_compat_state", &["legacy_fields_preserved"]),
        (
            "fulfillment_decision_path",
            &[
                "policy_allow",
                "policy_deny",
                "fail_closed_deny",
                "decision_error",
            ],
        ),
        (
            "classification_source",
            &["converged_outcome", "fulfillment_path", "legacy_fallback"],
        ),
        (
            "consumer_read_path",
            &[
                "converged_decision",
                "compatibility_markers",
                "legacy_decision",
            ],
        ),
        (
            "consumer_payload_state",
            &["converged", "compatibility_fallback", "legacy_base"],
        ),
        (
            "context_payload_state",
            &["complete_envelope", "partial_envelope", "absent_envelope"],
        ),
        (
            "deny_classification_source",
            &[
                "outcome_kind",
                "origin_context",
                "fulfillment_path",
                "legacy_decision",
                "not_deny",
            ],
        ),
    ];
    for (field, names) in variants {
        for name in *names {
            let mut string = base();
            string["data"][field] = json!(name);
            assert_parity(string, true, &format!("{field}={name}"));
            let mut map = base();
            map["data"][field] = json!({name.to_string(): null});
            assert_parity(map, true, &format!("{field} map={name}"));
        }
    }

    let mut obligations = base();
    obligations["data"]["obligations"] = json!([{
        "type": "restrict_scope",
        "detail": null,
        "restrict_scope": {"scope_type":"tool", "scope_value":"x", "scope_match_mode":"exact"},
        "redact_args": {"redaction_target":"arguments", "redaction_mode":"remove", "redaction_scope":"all"}
    }]);
    obligations["data"]["obligation_outcomes"] = json!([{
        "type":"restrict_scope", "status":"applied", "reason":null, "reason_code":null,
        "enforcement_stage":null, "normalization_version":null
    }]);
    obligations["data"]["fail_closed"] = json!({
        "tool_risk_class":"high_risk", "fail_closed_mode":"fail_closed",
        "fail_closed_trigger":"policy_engine_unavailable", "fail_closed_applied":true,
        "fail_closed_error_code":null
    });
    assert_parity(obligations, true, "complete nested structs");

    for status in ["applied", "skipped", "error"] {
        let mut candidate = base();
        candidate["data"]["obligation_outcomes"] = json!([{
            "type": "restrict_scope", "status": status
        }]);
        assert_parity(candidate, true, &format!("obligation status={status}"));
        let mut map = base();
        let mut status_map = serde_json::Map::new();
        status_map.insert(status.to_string(), Value::Null);
        map["data"]["obligation_outcomes"] = json!([{
            "type": "restrict_scope", "status": Value::Object(status_map)
        }]);
        assert_parity(map, true, &format!("obligation status map={status}"));
    }
    for risk in ["high_risk", "low_risk_read", "default"] {
        for mode in ["fail_closed", "degrade_read_only", "fail_safe_allow"] {
            for trigger in [
                "policy_engine_unavailable",
                "context_provider_unavailable",
                "runtime_dependency_error",
            ] {
                let mut candidate = base();
                candidate["data"]["fail_closed"] = json!({
                    "tool_risk_class": risk,
                    "fail_closed_mode": mode,
                    "fail_closed_trigger": trigger,
                    "fail_closed_applied": true,
                    "fail_closed_error_code": null
                });
                assert_parity(
                    candidate,
                    true,
                    &format!("fail_closed {risk}/{mode}/{trigger}"),
                );
            }
        }
    }
    for (field, variants) in [
        (
            "tool_risk_class",
            &["high_risk", "low_risk_read", "default"][..],
        ),
        (
            "fail_closed_mode",
            &["fail_closed", "degrade_read_only", "fail_safe_allow"][..],
        ),
        (
            "fail_closed_trigger",
            &[
                "policy_engine_unavailable",
                "context_provider_unavailable",
                "runtime_dependency_error",
            ][..],
        ),
    ] {
        for variant in variants {
            let mut candidate = base();
            candidate["data"]["fail_closed"] = json!({
                "tool_risk_class": "high_risk",
                "fail_closed_mode": "fail_closed",
                "fail_closed_trigger": "policy_engine_unavailable",
                "fail_closed_applied": true,
                "fail_closed_error_code": null
            });
            candidate["data"]["fail_closed"][field] = json!({variant.to_string(): null});
            assert_parity(
                candidate,
                true,
                &format!("fail_closed {field} map={variant}"),
            );
        }
    }
}

#[test]
fn nested_required_fields_types_unknowns_and_positional_forms_match() {
    let nested_cases = [
        (
            "obligation",
            "obligations",
            json!({"type":"restrict_scope", "restrict_scope":{"scope_type":"tool","scope_value":"x","scope_match_mode":"exact"}}),
            "type",
        ),
        (
            "outcome",
            "obligation_outcomes",
            json!({"type":"restrict_scope", "status":"applied"}),
            "status",
        ),
    ];
    for (label, field, valid, required) in nested_cases {
        let mut candidate = base();
        candidate["data"][field] = json!([valid.clone()]);
        assert_parity(candidate, true, &format!("valid {label}"));

        let mut unknown = valid.clone();
        unknown["future"] = json!({"nested":[1, null]});
        let mut candidate = base();
        candidate["data"][field] = json!([unknown]);
        assert_parity(candidate, true, &format!("unknown nested {label}"));

        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(required);
        let mut candidate = base();
        candidate["data"][field] = json!([missing]);
        assert_parity(
            candidate,
            false,
            &format!("missing nested {label} {required}"),
        );

        let mut wrong = valid;
        wrong[required] = json!(1);
        let mut candidate = base();
        candidate["data"][field] = json!([wrong]);
        assert_parity(
            candidate,
            false,
            &format!("wrong nested {label} {required}"),
        );
    }

    for required in ["scope_type", "scope_value", "scope_match_mode"] {
        let mut restrict =
            json!({"scope_type":"tool","scope_value":"x","scope_match_mode":"exact"});
        restrict.as_object_mut().unwrap().remove(required);
        let mut candidate = base();
        candidate["data"]["obligations"] =
            json!([{"type":"restrict_scope","restrict_scope":restrict}]);
        assert_parity(
            candidate,
            false,
            &format!("missing restrict_scope {required}"),
        );
    }
    for required in ["redaction_target", "redaction_mode", "redaction_scope"] {
        let mut redact = json!({"redaction_target":"arguments","redaction_mode":"remove","redaction_scope":"all"});
        redact.as_object_mut().unwrap().remove(required);
        let mut candidate = base();
        candidate["data"]["obligations"] = json!([{"type":"redact_args","redact_args":redact}]);
        assert_parity(candidate, false, &format!("missing redact_args {required}"));
    }
    for required in ["tool_risk_class", "fail_closed_mode", "fail_closed_applied"] {
        let mut fail_closed = json!({
            "tool_risk_class":"high_risk", "fail_closed_mode":"fail_closed",
            "fail_closed_trigger":null, "fail_closed_applied":true,
            "fail_closed_error_code":null
        });
        fail_closed.as_object_mut().unwrap().remove(required);
        let mut candidate = base();
        candidate["data"]["fail_closed"] = fail_closed;
        assert_parity(candidate, false, &format!("missing fail_closed {required}"));
    }

    let positional = [
        ("restrict_scope positional", json!(["tool", "x", "exact"])),
        (
            "redact_args positional",
            json!(["arguments", "remove", "all"]),
        ),
    ];
    for (label, value) in positional {
        let mut candidate = base();
        candidate["data"]["obligations"] = if label.starts_with("restrict") {
            json!([{"type":"restrict_scope", "restrict_scope": value}])
        } else {
            json!([{"type":"redact_args", "redact_args": value}])
        };
        assert_parity(candidate, true, label);
    }
    let mut outcome = base();
    outcome["data"]["obligation_outcomes"] =
        json!([["restrict_scope", "applied", null, null, null, null]]);
    assert_parity(outcome, true, "obligation outcome positional");
    let mut fail_closed = base();
    fail_closed["data"]["fail_closed"] = json!(["high_risk", "fail_closed", null, true, null]);
    assert_parity(fail_closed, true, "fail closed positional");

    let positional_failures = [
        (
            "restrict missing tail",
            "obligations",
            json!([{"type":"restrict_scope", "restrict_scope":["tool", "x"]}]),
        ),
        (
            "restrict surplus",
            "obligations",
            json!([{"type":"restrict_scope", "restrict_scope":["tool", "x", "exact", "extra"]}]),
        ),
        (
            "outcome missing tail",
            "obligation_outcomes",
            json!([["restrict_scope"]]),
        ),
        (
            "outcome surplus",
            "obligation_outcomes",
            json!([["restrict_scope", "applied", null, null, null, null, "extra"]]),
        ),
    ];
    for (label, field, value) in positional_failures {
        let mut candidate = base();
        candidate["data"][field] = value;
        assert_parity(candidate, false, label);
    }
}

#[test]
fn escaped_borrowed_headers_use_the_value_oracle_without_narrowing_strings() {
    let raw = r#"{"specversion":"1\u002e0","id":"decision-1","type":"assay\u002etool\u002edecision","source":"urn:assay:test","time":"2026-10-12T00:00:00Z","data":{"tool":"list_files","decision":"allow","reason_code":"allowed","tool_call_id":"call-1"}}"#;
    assert!(serde_json::from_str::<DecisionEvent>(raw).is_err());
    let parsed: Value = serde_json::from_str(raw).unwrap();
    assert_parity(parsed, true, "escaped headers through leaked Value");
}

#[test]
fn producer_source_inputs_remain_at_the_reviewed_mapping() {
    const SOURCES: &[(&str, &str)] = &[
        (
            include_str!("../src/mcp/decision_next/event_types.rs"),
            "2f5be2aee42eeb508f066ce5b050fc4ef5c9105e5997e7af675a35321ede9a4d",
        ),
        (
            include_str!("../src/mcp/policy/types.rs"),
            "45f4d001a2daec1f7ec0a7b4ebbfde10c99f383675ec9d124082d2ea2e6bb1d2",
        ),
        (
            include_str!("../src/mcp/decision/consumer_contract.rs"),
            "4beb2be0c30b579caf8b9308668923e6af4334712c4d089bcacb8044bad8cca1",
        ),
        (
            include_str!("../src/mcp/decision/context_contract.rs"),
            "26c0dd36951dd7f8e95b26d357e245890464329b16fbfb83077be19ee88adb9f",
        ),
        (
            include_str!("../src/mcp/decision/deny_convergence.rs"),
            "b66897155fe9035166d57eeaa3371d27851ce9ec3e91f77f7d875dc6b2aae3bc",
        ),
        (
            include_str!("../src/mcp/decision/outcome_convergence.rs"),
            "ad126734ed845c7e25b42047c600aca9034b0b8e2cc07c24c08af9eecfed5ad6",
        ),
        (
            include_str!("../src/mcp/decision/replay_compat.rs"),
            "4d1322623a19948cf4e76c0ea15a3ad264d7519e2ed93ce76b03e8c04ab8f7e0",
        ),
    ];
    for (source, expected) in SOURCES {
        assert_eq!(hex::encode(Sha256::digest(source.as_bytes())), *expected);
    }
}
