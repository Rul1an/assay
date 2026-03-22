use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions, LintReportWithPacks};
use assay_evidence::lint::packs::{
    load_packs, CheckDefinition, LoadedPack, PackDefinition, PackKind, PackRequirements, PackRule,
    PackSource, Severity,
};
use assay_evidence::{BundleWriter, EvidenceEvent, VerifyLimits};
use chrono::{TimeZone, Utc};
use serde_json::json;
use std::io::Cursor;
use std::path::PathBuf;

fn make_event(type_: &str, run_id: &str, seq: u64, payload: serde_json::Value) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(type_, "urn:assay:test:e1", run_id, seq, payload);
    event.time = Utc.timestamp_opt(1_700_100_000 + seq as i64, 0).unwrap();
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

fn lint_with_pack(pack: LoadedPack, bundle: &[u8]) -> LintReportWithPacks {
    let options = LintOptions {
        packs: vec![pack],
        max_results: Some(500),
        bundle_path: Some("e1-conditional.tar.gz".to_string()),
    };
    lint_bundle_with_options(Cursor::new(bundle), VerifyLimits::default(), options)
        .expect("lint should succeed")
}

fn has_rule_finding(result: &LintReportWithPacks, pack_name: &str, rule_id: &str) -> bool {
    let prefix = format!("{pack_name}@");
    result
        .report
        .findings
        .iter()
        .any(|finding| finding.rule_id.starts_with(&prefix) && finding.rule_id.ends_with(rule_id))
}

fn finding_message<'a>(
    result: &'a LintReportWithPacks,
    pack_name: &str,
    rule_id: &str,
) -> Option<&'a str> {
    let prefix = format!("{pack_name}@");
    result
        .report
        .findings
        .iter()
        .find(|finding| finding.rule_id.starts_with(&prefix) && finding.rule_id.ends_with(rule_id))
        .map(|finding| finding.message.as_str())
}

fn make_test_pack(name: &str, kind: PackKind, rules: Vec<PackRule>) -> LoadedPack {
    LoadedPack {
        definition: PackDefinition {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            kind,
            description: "E1 conditional test pack".to_string(),
            author: "Assay Team".to_string(),
            license: "Apache-2.0".to_string(),
            source_url: None,
            disclaimer: None,
            requires: PackRequirements {
                assay_min_version: ">=0.0.0".to_string(),
                evidence_schema_version: None,
            },
            rules,
        },
        digest: "sha256:e1test".to_string(),
        source: PackSource::File(PathBuf::from("e1-test-pack.yaml")),
    }
}

fn conditional_rule_pack(kind: PackKind) -> LoadedPack {
    make_test_pack(
        "conditional-e1-pack",
        kind,
        vec![PackRule {
            id: "COND-001".to_string(),
            severity: Severity::Error,
            description: "allow decisions must include mandate context".to_string(),
            article_ref: None,
            help_markdown: None,
            check: CheckDefinition::Conditional {
                condition: Some(json!({
                    "all": [
                        {
                            "path": "/data/decision",
                            "equals": "allow"
                        }
                    ]
                })),
                then_check: Some(json!({
                    "type": "json_path_exists",
                    "paths": ["/data/mandate_id"]
                })),
            },
            engine_min_version: Some("1.1".to_string()),
            event_types: Some(vec!["assay.tool.decision".to_string()]),
        }],
    )
}

fn unsupported_conditional_pack(kind: PackKind) -> LoadedPack {
    make_test_pack(
        "unsupported-conditional-pack",
        kind,
        vec![PackRule {
            id: "COND-UNSUPPORTED".to_string(),
            severity: Severity::Error,
            description: "unsupported conditional shape".to_string(),
            article_ref: None,
            help_markdown: None,
            check: CheckDefinition::Conditional {
                condition: Some(json!({
                    "any": [
                        {
                            "path": "/data/decision",
                            "equals": "allow"
                        }
                    ]
                })),
                then_check: Some(json!({
                    "type": "json_path_exists",
                    "paths": ["/data/mandate_id"]
                })),
            },
            engine_min_version: Some("1.1".to_string()),
            event_types: Some(vec!["assay.tool.decision".to_string()]),
        }],
    )
}

fn event_field_present_pack(scoped: bool) -> LoadedPack {
    make_test_pack(
        "event-field-pack",
        PackKind::Security,
        vec![PackRule {
            id: "FIELD-001".to_string(),
            severity: Severity::Error,
            description: "mandate field should exist".to_string(),
            article_ref: None,
            help_markdown: None,
            check: CheckDefinition::EventFieldPresent {
                paths_any_of: Some(vec!["/data/mandate_id".to_string()]),
                any_of: None,
                in_data: false,
            },
            engine_min_version: None,
            event_types: scoped.then(|| vec!["assay.tool.decision".to_string()]),
        }],
    )
}

fn json_path_exists_pack(scoped: bool) -> LoadedPack {
    make_test_pack(
        "json-path-pack",
        PackKind::Security,
        vec![PackRule {
            id: "JSON-001".to_string(),
            severity: Severity::Error,
            description: "mandate field should exist".to_string(),
            article_ref: None,
            help_markdown: None,
            check: CheckDefinition::JsonPathExists {
                paths: vec!["/data/mandate_id".to_string()],
            },
            engine_min_version: None,
            event_types: scoped.then(|| vec!["assay.tool.decision".to_string()]),
        }],
    )
}

fn decision_allow(run_id: &str, seq: u64) -> EvidenceEvent {
    make_event(
        "assay.tool.decision",
        run_id,
        seq,
        json!({
            "decision": "allow",
            "tool": "write_file"
        }),
    )
}

fn decision_allow_with_mandate(run_id: &str, seq: u64) -> EvidenceEvent {
    make_event(
        "assay.tool.decision",
        run_id,
        seq,
        json!({
            "decision": "allow",
            "tool": "write_file",
            "mandate_id": "mandate-123"
        }),
    )
}

fn decision_deny(run_id: &str, seq: u64) -> EvidenceEvent {
    make_event(
        "assay.tool.decision",
        run_id,
        seq,
        json!({
            "decision": "deny",
            "tool": "write_file"
        }),
    )
}

fn unrelated_event_with_mandate(run_id: &str, seq: u64) -> EvidenceEvent {
    make_event(
        "assay.profile.started",
        run_id,
        seq,
        json!({
            "mandate_id": "mandate-foreign"
        }),
    )
}

#[test]
fn conditional_rule_passes_when_no_events_match_condition() {
    let result = lint_with_pack(
        conditional_rule_pack(PackKind::Security),
        &make_bundle(vec![decision_deny("run_conditional", 0)]),
    );

    assert!(
        !has_rule_finding(&result, "conditional-e1-pack", "COND-001"),
        "no matching allow decision should mean no finding"
    );
}

#[test]
fn conditional_rule_fails_when_matching_event_lacks_required_path() {
    let result = lint_with_pack(
        conditional_rule_pack(PackKind::Security),
        &make_bundle(vec![decision_allow("run_conditional", 0)]),
    );

    assert!(has_rule_finding(&result, "conditional-e1-pack", "COND-001"));
    let message = finding_message(&result, "conditional-e1-pack", "COND-001")
        .expect("expected conditional finding");
    assert!(
        message.contains("1 event matched the condition")
            && message.contains("1 matching event was missing required path: /data/mandate_id"),
        "unexpected conditional finding message: {message}"
    );
}

#[test]
fn conditional_rule_passes_when_matching_event_contains_required_path() {
    let result = lint_with_pack(
        conditional_rule_pack(PackKind::Security),
        &make_bundle(vec![decision_allow_with_mandate("run_conditional", 0)]),
    );

    assert!(
        !has_rule_finding(&result, "conditional-e1-pack", "COND-001"),
        "matching allow decision with mandate context should pass"
    );
}

#[test]
fn adding_unrelated_non_matching_events_does_not_change_conditional_result() {
    let base_result = lint_with_pack(
        conditional_rule_pack(PackKind::Security),
        &make_bundle(vec![decision_allow("run_conditional", 0)]),
    );
    let noisy_result = lint_with_pack(
        conditional_rule_pack(PackKind::Security),
        &make_bundle(vec![
            decision_allow("run_conditional", 0),
            unrelated_event_with_mandate("run_conditional", 1),
            decision_deny("run_conditional", 2),
        ]),
    );

    assert_eq!(
        has_rule_finding(&base_result, "conditional-e1-pack", "COND-001"),
        has_rule_finding(&noisy_result, "conditional-e1-pack", "COND-001"),
        "unrelated non-matching events should not change conditional outcome"
    );
}

#[test]
fn event_field_present_respects_event_types_filter() {
    let scoped = lint_with_pack(
        event_field_present_pack(true),
        &make_bundle(vec![
            unrelated_event_with_mandate("run_field", 0),
            decision_allow("run_field", 1),
        ]),
    );
    let unscoped = lint_with_pack(
        event_field_present_pack(false),
        &make_bundle(vec![
            unrelated_event_with_mandate("run_field", 0),
            decision_allow("run_field", 1),
        ]),
    );

    assert!(has_rule_finding(&scoped, "event-field-pack", "FIELD-001"));
    assert!(
        !has_rule_finding(&unscoped, "event-field-pack", "FIELD-001"),
        "without event_types the unrelated event still satisfies the field check"
    );
}

#[test]
fn json_path_exists_respects_event_types_filter() {
    let scoped = lint_with_pack(
        json_path_exists_pack(true),
        &make_bundle(vec![
            unrelated_event_with_mandate("run_json", 0),
            decision_allow("run_json", 1),
        ]),
    );
    let unscoped = lint_with_pack(
        json_path_exists_pack(false),
        &make_bundle(vec![
            unrelated_event_with_mandate("run_json", 0),
            decision_allow("run_json", 1),
        ]),
    );

    assert!(has_rule_finding(&scoped, "json-path-pack", "JSON-001"));
    assert!(
        !has_rule_finding(&unscoped, "json-path-pack", "JSON-001"),
        "without event_types the unrelated event still satisfies json_path_exists"
    );
}

#[test]
fn unsupported_conditional_shape_still_skips_for_security_pack() {
    let result = lint_with_pack(
        unsupported_conditional_pack(PackKind::Security),
        &make_bundle(vec![decision_allow("run_skip", 0)]),
    );

    assert!(
        !has_rule_finding(&result, "unsupported-conditional-pack", "COND-UNSUPPORTED"),
        "unsupported conditional shapes should continue to skip in security packs"
    );
}

#[test]
fn unsupported_conditional_shape_fails_for_compliance_pack() {
    let result = lint_with_pack(
        unsupported_conditional_pack(PackKind::Compliance),
        &make_bundle(vec![decision_allow("run_skip", 0)]),
    );

    let message = finding_message(&result, "unsupported-conditional-pack", "COND-UNSUPPORTED")
        .expect("compliance pack should fail on unsupported conditional shape");
    assert!(message.contains("Cannot execute rule"));
    assert!(message.contains("Unsupported conditional shape for engine v1.1"));
}

#[test]
fn mandate_001_fails_allow_decision_without_mandate_id() {
    let pack = load_packs(&["mandate-baseline".to_string()]).expect("pack should load");
    let result = lint_with_pack(
        pack.into_iter().next().expect("built-in pack should exist"),
        &make_bundle(vec![decision_allow("run_mandate", 0)]),
    );

    assert!(has_rule_finding(&result, "mandate-baseline", "MANDATE-001"));
}

#[test]
fn mandate_001_passes_allow_decision_with_mandate_id() {
    let pack = load_packs(&["mandate-baseline".to_string()]).expect("pack should load");
    let result = lint_with_pack(
        pack.into_iter().next().expect("built-in pack should exist"),
        &make_bundle(vec![decision_allow_with_mandate("run_mandate", 0)]),
    );

    assert!(
        !has_rule_finding(&result, "mandate-baseline", "MANDATE-001"),
        "allow decision with mandate_id should satisfy MANDATE-001"
    );
}

#[test]
fn mandate_001_ignores_non_decision_events_even_if_they_contain_mandate_id() {
    let pack = load_packs(&["mandate-baseline".to_string()]).expect("pack should load");
    let result = lint_with_pack(
        pack.into_iter().next().expect("built-in pack should exist"),
        &make_bundle(vec![
            decision_allow("run_mandate", 0),
            unrelated_event_with_mandate("run_mandate", 1),
        ]),
    );

    assert!(
        has_rule_finding(&result, "mandate-baseline", "MANDATE-001"),
        "event_types scoping should prevent unrelated events from satisfying MANDATE-001"
    );
}

#[test]
fn future_mandate_rules_remain_version_gated() {
    let pack = load_packs(&["mandate-baseline".to_string()]).expect("pack should load");
    let result = lint_with_pack(
        pack.into_iter().next().expect("built-in pack should exist"),
        &make_bundle(vec![decision_allow_with_mandate("run_mandate", 0)]),
    );

    for future_rule in ["MANDATE-002", "MANDATE-003", "MANDATE-004", "MANDATE-005"] {
        assert!(
            !has_rule_finding(&result, "mandate-baseline", future_rule),
            "{future_rule} should remain version-gated beyond engine v1.1"
        );
    }
}
