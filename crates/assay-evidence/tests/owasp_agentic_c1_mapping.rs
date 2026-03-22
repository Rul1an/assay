use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions, LintReportWithPacks};
use assay_evidence::lint::packs::loader::load_pack_from_file;
use assay_evidence::lint::packs::{CheckDefinition, LoadedPack, PackKind};
use assay_evidence::{BundleWriter, EvidenceEvent, VerifyLimits};
use chrono::{TimeZone, Utc};
use serde_json::json;
use std::io::Cursor;
use std::path::{Path, PathBuf};

fn fixture_pack(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("packs")
        .join(name)
}

fn load_probe_pack(pack_file: &str) -> LoadedPack {
    load_pack_from_file(&fixture_pack(pack_file)).expect("probe pack should load")
}

fn make_event(type_: &str, run_id: &str, seq: u64, payload: serde_json::Value) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(type_, "urn:assay:test:c1", run_id, seq, payload);
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

fn lint_with_pack(pack_file: &str, bundle: &[u8]) -> LintReportWithPacks {
    let pack = load_probe_pack(pack_file);
    let options = LintOptions {
        packs: vec![pack],
        max_results: Some(500),
        bundle_path: Some("c1-probe.tar.gz".to_string()),
    };
    lint_bundle_with_options(Cursor::new(bundle), VerifyLimits::default(), options)
        .expect("lint should succeed")
}

fn pack_findings<'a>(result: &'a LintReportWithPacks, pack_name: &str) -> Vec<&'a String> {
    let prefix = format!("{pack_name}@");
    result
        .report
        .findings
        .iter()
        .filter(|finding| finding.rule_id.starts_with(&prefix))
        .map(|finding| &finding.rule_id)
        .collect()
}

fn has_rule_finding(result: &LintReportWithPacks, pack_name: &str, rule_id: &str) -> bool {
    let prefix = format!("{pack_name}@");
    result
        .report
        .findings
        .iter()
        .any(|finding| finding.rule_id.starts_with(&prefix) && finding.rule_id.ends_with(rule_id))
}

fn find_rule<'a>(pack: &'a LoadedPack, rule_id: &str) -> &'a assay_evidence::lint::packs::PackRule {
    pack.definition
        .rules
        .iter()
        .find(|rule| rule.id == rule_id)
        .expect("expected rule to exist")
}

fn goal_governance_bundle() -> Vec<u8> {
    make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_goal_governance",
        0,
        json!({
            "tool": "search.documents",
            "decision": "deny",
            "reason_code": "P_TOOL_DENIED",
            "policy_deny": true,
            "fail_closed_deny": false,
            "approval_state": "not_required"
        }),
    )])
}

fn authorization_bundle(include_mandate: bool, include_delegation_fields: bool) -> Vec<u8> {
    let mut payload = json!({
        "tool": "tool.commit",
        "decision": "allow",
        "principal": "user:alice",
        "approval_state": "granted"
    });

    if include_mandate {
        payload["mandate_id"] = json!("mandate-123");
    }

    if include_delegation_fields {
        payload["delegated_from"] = json!("service-account:runner");
        payload["actor_chain"] = json!(["user:alice", "service-account:runner"]);
        payload["delegation_depth"] = json!(1);
    }

    make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_authorization",
        0,
        payload,
    )])
}

fn current_profile_baseline_bundle() -> Vec<u8> {
    make_bundle(vec![
        make_event(
            "assay.profile.started",
            "run_profile",
            0,
            json!({
                "profile_name": "c1-baseline",
                "profile_version": "1.0.0",
                "total_runs_aggregated": 1
            }),
        ),
        make_event(
            "assay.process.exec",
            "run_profile",
            1,
            json!({
                "hits": 1
            }),
        ),
        make_event(
            "assay.profile.finished",
            "run_profile",
            2,
            json!({
                "processes_count": 1,
                "integrity_scope": "observed"
            }),
        ),
    ])
}

#[test]
fn a1_probe_executes_without_unsupported_checks() {
    let pack = load_probe_pack("owasp-agentic-a1-probe.yaml");
    assert!(pack
        .definition
        .rules
        .iter()
        .all(|rule| rule.engine_min_version.is_none() && !rule.check.is_unsupported()));

    let result = lint_with_pack("owasp-agentic-a1-probe.yaml", &goal_governance_bundle());
    assert!(
        pack_findings(&result, "owasp-agentic-a1-probe").is_empty(),
        "expected no A1 pack findings, got {:?}",
        result.report.findings
    );
}

#[test]
fn a1_only_proves_goal_governance_control_evidence() {
    let pack = load_probe_pack("owasp-agentic-a1-probe.yaml");

    assert!(matches!(
        find_rule(&pack, "A1-001").check,
        CheckDefinition::EventTypeExists { .. }
    ));
    assert!(matches!(
        find_rule(&pack, "A1-002").check,
        CheckDefinition::EventFieldPresent { .. }
    ));
}

#[test]
fn a3_conditional_presence_rule_is_supported_in_engine_v1_1() {
    let pack = load_probe_pack("owasp-agentic-a3-probe.yaml");

    assert!(matches!(
        find_rule(&pack, "A3-001").check,
        CheckDefinition::EventFieldPresent { .. }
    ));

    let linkage_rule = find_rule(&pack, "A3-002");
    assert!(matches!(
        linkage_rule.check,
        CheckDefinition::Conditional { .. }
    ));
    assert_eq!(linkage_rule.engine_min_version.as_deref(), Some("1.1"));
    assert!(
        !linkage_rule.check.is_unsupported(),
        "typed conditional presence rule should be supported in engine v1.1"
    );
}

#[test]
fn a3_signal_gap_requires_fixture_or_evidenceflow_proof() {
    let result = lint_with_pack(
        "owasp-agentic-a3-probe.yaml",
        &authorization_bundle(true, false),
    );

    assert!(
        !has_rule_finding(&result, "owasp-agentic-a3-probe", "A3-001"),
        "presence rule should pass on current authz fixture"
    );
    assert!(
        has_rule_finding(&result, "owasp-agentic-a3-probe", "A3-003"),
        "delegation chain probe should fail against current authz fixture"
    );
}

#[test]
fn a5_process_exec_presence_is_yaml_only() {
    let result = lint_with_pack(
        "owasp-agentic-a5-probe.yaml",
        &current_profile_baseline_bundle(),
    );

    assert!(
        !has_rule_finding(&result, "owasp-agentic-a5-probe", "A5-001"),
        "process exec presence should pass on current baseline fixture"
    );
}

#[test]
fn a5_sandbox_rule_is_signal_gap_in_current_baseline_fixture() {
    let result = lint_with_pack(
        "owasp-agentic-a5-probe.yaml",
        &current_profile_baseline_bundle(),
    );

    assert!(
        has_rule_finding(&result, "owasp-agentic-a5-probe", "A5-002"),
        "sandbox degradation probe should fail against current baseline fixture"
    );
}

#[test]
fn a3_conditional_presence_rule_fails_without_mandate_context() {
    let pack = load_probe_pack("owasp-agentic-a3-probe.yaml");
    assert_eq!(pack.definition.kind, PackKind::Security);

    let result = lint_with_pack(
        "owasp-agentic-a3-probe.yaml",
        &authorization_bundle(false, true),
    );

    assert!(
        !has_rule_finding(&result, "owasp-agentic-a3-probe", "A3-001"),
        "authorization presence rule should pass"
    );
    assert!(
        !has_rule_finding(&result, "owasp-agentic-a3-probe", "A3-003"),
        "delegation signal rule should pass once delegation fields are present"
    );
    assert!(
        has_rule_finding(&result, "owasp-agentic-a3-probe", "A3-002"),
        "conditional presence rule should fail when allow decisions lack mandate context"
    );
}

#[test]
fn a3_conditional_presence_rule_passes_with_mandate_context() {
    let result = lint_with_pack(
        "owasp-agentic-a3-probe.yaml",
        &authorization_bundle(true, true),
    );

    assert!(
        !has_rule_finding(&result, "owasp-agentic-a3-probe", "A3-001"),
        "authorization presence rule should pass"
    );
    assert!(
        !has_rule_finding(&result, "owasp-agentic-a3-probe", "A3-003"),
        "delegation signal rule should pass once delegation fields are present"
    );
    assert!(
        !has_rule_finding(&result, "owasp-agentic-a3-probe", "A3-002"),
        "conditional presence rule should pass once mandate context is present"
    );
}
