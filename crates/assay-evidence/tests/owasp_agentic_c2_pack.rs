use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions, LintReportWithPacks};
use assay_evidence::lint::packs::loader::{load_pack, load_pack_from_file};
use assay_evidence::lint::packs::{CheckDefinition, LoadedPack};
use assay_evidence::{BundleWriter, EvidenceEvent, VerifyLimits};
use chrono::{TimeZone, Utc};
use serde_json::json;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

fn open_pack_path() -> PathBuf {
    repo_root()
        .join("packs")
        .join("open")
        .join("owasp-agentic-control-evidence-baseline")
        .join("pack.yaml")
}

fn readme_path() -> PathBuf {
    repo_root()
        .join("packs")
        .join("open")
        .join("owasp-agentic-control-evidence-baseline")
        .join("README.md")
}

fn builtin_pack_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("packs")
        .join("owasp-agentic-control-evidence-baseline.yaml")
}

fn load_open_pack() -> LoadedPack {
    load_pack_from_file(&open_pack_path()).expect("open pack should load")
}

fn load_builtin_pack() -> LoadedPack {
    load_pack("owasp-agentic-control-evidence-baseline").expect("built-in pack should load")
}

fn normalize_text(input: &str) -> String {
    input.to_ascii_lowercase()
}

fn normalize_space_text(input: &str) -> String {
    input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn count_case_insensitive(haystack: &str, needle: &str) -> usize {
    normalize_text(haystack)
        .matches(&normalize_text(needle))
        .count()
}

fn canonical_rule_ids(pack: &LoadedPack) -> Vec<&str> {
    pack.definition
        .rules
        .iter()
        .map(|rule| rule.id.as_str())
        .collect()
}

fn make_event(type_: &str, run_id: &str, seq: u64, payload: serde_json::Value) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(type_, "urn:assay:test:c2", run_id, seq, payload);
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

fn compliant_bundle() -> Vec<u8> {
    make_bundle(vec![
        make_event(
            "assay.tool.decision",
            "run_c2",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "reason_code": "ALLOW_BY_POLICY",
                "principal": "user:alice",
                "approval_state": "granted"
            }),
        ),
        make_event(
            "assay.process.exec",
            "run_c2",
            1,
            json!({
                "hits": 1
            }),
        ),
    ])
}

fn bundle_missing_a1_fields() -> Vec<u8> {
    make_bundle(vec![
        make_event(
            "assay.tool.decision",
            "run_c2_a1_missing",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "principal": "user:alice"
            }),
        ),
        make_event(
            "assay.process.exec",
            "run_c2_a1_missing",
            1,
            json!({
                "hits": 1
            }),
        ),
    ])
}

fn bundle_a1_with_single_governance_field() -> Vec<u8> {
    make_bundle(vec![
        make_event(
            "assay.tool.decision",
            "run_c2_a1_single",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "approval_state": "granted",
                "principal": "user:alice"
            }),
        ),
        make_event(
            "assay.process.exec",
            "run_c2_a1_single",
            1,
            json!({
                "hits": 1
            }),
        ),
    ])
}

fn bundle_missing_a3_fields() -> Vec<u8> {
    make_bundle(vec![
        make_event(
            "assay.tool.decision",
            "run_c2_a3_missing",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "reason_code": "ALLOW_BY_POLICY"
            }),
        ),
        make_event(
            "assay.process.exec",
            "run_c2_a3_missing",
            1,
            json!({
                "hits": 1
            }),
        ),
    ])
}

fn bundle_a3_with_single_authz_field() -> Vec<u8> {
    make_bundle(vec![
        make_event(
            "assay.tool.decision",
            "run_c2_a3_single",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "reason_code": "ALLOW_BY_POLICY",
                "approval_state": "granted"
            }),
        ),
        make_event(
            "assay.process.exec",
            "run_c2_a3_single",
            1,
            json!({
                "hits": 1
            }),
        ),
    ])
}

fn bundle_missing_a5_event() -> Vec<u8> {
    make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_c2_a5_missing",
        0,
        json!({
            "tool": "tool.commit",
            "decision": "allow",
            "reason_code": "ALLOW_BY_POLICY",
            "principal": "user:alice",
            "approval_state": "granted"
        }),
    )])
}

fn lint_with_pack(pack: LoadedPack, bundle: &[u8]) -> LintReportWithPacks {
    let options = LintOptions {
        packs: vec![pack],
        max_results: Some(500),
        bundle_path: Some("c2-pack.tar.gz".to_string()),
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

#[test]
fn c2_loads_builtin_and_open_pack_with_three_rules() {
    let builtin = load_builtin_pack();
    let open = load_open_pack();

    assert_eq!(
        builtin.definition.name,
        "owasp-agentic-control-evidence-baseline"
    );
    assert_eq!(
        open.definition.name,
        "owasp-agentic-control-evidence-baseline"
    );
    assert_eq!(builtin.definition.rules.len(), 3);
    assert_eq!(open.definition.rules.len(), 3);
}

#[test]
fn c2_builtin_and_open_pack_are_exactly_equivalent() {
    let builtin_raw = fs::read_to_string(builtin_pack_path()).expect("read builtin yaml");
    let open_raw = fs::read_to_string(open_pack_path()).expect("read open yaml");
    assert_eq!(
        builtin_raw, open_raw,
        "open pack and built-in mirror must match exactly"
    );

    let builtin = load_builtin_pack();
    let open = load_open_pack();
    assert_eq!(builtin.digest, open.digest);
    assert_eq!(builtin.definition.name, open.definition.name);
    assert_eq!(builtin.definition.version, open.definition.version);
    assert_eq!(
        builtin.definition.kind.to_string(),
        open.definition.kind.to_string()
    );
    assert_eq!(builtin.definition.description, open.definition.description);
    assert_eq!(
        serde_json::to_value(&builtin.definition).expect("serialize builtin definition"),
        serde_json::to_value(&open.definition).expect("serialize open definition")
    );
}

#[test]
fn c2_pack_contains_only_c1_ship_yes_rules() {
    let pack = load_builtin_pack();
    assert_eq!(
        canonical_rule_ids(&pack),
        vec!["A1-002", "A3-001", "A5-001"]
    );
}

#[test]
fn c2_pack_contains_no_skip_prone_checks() {
    let pack = load_builtin_pack();

    for rule in &pack.definition.rules {
        assert!(
            rule.engine_min_version.is_none(),
            "rule {} must not require a future engine",
            rule.id
        );
        assert!(
            !rule.check.is_unsupported(),
            "rule {} must not use unsupported checks",
            rule.id
        );
        assert!(
            !matches!(rule.check, CheckDefinition::Conditional { .. }),
            "rule {} must not use conditional checks",
            rule.id
        );
    }
}

#[test]
fn c2_pack_contains_no_mandate_linkage_or_temporal_claims() {
    let text = normalize_text(
        &fs::read_to_string(open_pack_path()).expect("open pack yaml should be readable"),
    );

    for forbidden in [
        "mandate_id",
        "delegated_from",
        "actor_chain",
        "delegation_depth",
        "inherited_scopes",
        "temporal validity",
        "mandate linkage",
        "sandbox degradation",
    ] {
        assert!(
            !text.contains(forbidden),
            "open pack must not contain forbidden claim or signal: {forbidden}"
        );
    }
}

#[test]
fn c2_readme_explicitly_states_non_goals() {
    let readme = fs::read_to_string(readme_path()).expect("readme should be readable");
    assert!(readme.contains("## Non-Goals"));

    for phrase in [
        "goal hijack detection",
        "privilege abuse prevention",
        "mandate linkage enforcement",
        "temporal validity of approvals or mandates",
        "delegation-chain visibility",
        "sandbox degradation detection",
    ] {
        assert_eq!(
            count_case_insensitive(&readme, phrase),
            1,
            "README should mention non-goal phrase exactly once: {phrase}"
        );
    }

    assert!(normalize_space_text(&readme).contains(
        "this pack proves only that process-execution evidence is present in the baseline flow; it does not prove execution authorization, containment, or sandboxing."
    ));
}

#[test]
fn c2_pack_wording_stays_control_evidence_only() {
    let open_yaml = fs::read_to_string(open_pack_path()).expect("open pack yaml");
    let builtin_yaml = fs::read_to_string(builtin_pack_path()).expect("builtin pack yaml");
    let builtin = load_builtin_pack();

    let normalized_yaml = normalize_text(&(open_yaml + "\n" + &builtin_yaml));
    for forbidden in [
        "verifies privilege abuse",
        "privilege abuse prevention",
        "proves sandboxing",
        "sandbox degradation protection",
        "mandate linkage enforcement",
        "temporal validity enforcement",
    ] {
        assert!(
            !normalized_yaml.contains(forbidden),
            "pack yaml must not contain overclaim phrase: {forbidden}"
        );
    }

    for rule in &builtin.definition.rules {
        let normalized_description = normalize_text(&rule.description);
        assert!(
            !normalized_description.contains("detects goal hijack"),
            "rule descriptions must stay evidence-only"
        );
        assert!(
            !normalized_description.contains("verifies privilege abuse"),
            "rule descriptions must stay evidence-only"
        );
        assert!(
            !normalized_description.contains("proves sandboxing"),
            "rule descriptions must stay evidence-only"
        );
    }
}

#[test]
fn c2_compliant_control_evidence_bundle_has_no_pack_findings() {
    let result = lint_with_pack(load_builtin_pack(), &compliant_bundle());

    let findings: Vec<_> = result
        .report
        .findings
        .iter()
        .filter(|finding| {
            finding
                .rule_id
                .starts_with("owasp-agentic-control-evidence-baseline@")
        })
        .collect();

    assert!(
        findings.is_empty(),
        "expected no C2 pack findings for compliant bundle, got {:?}",
        result.report.findings
    );
}

#[test]
fn c2_missing_governance_rationale_fails_a1_002() {
    let result = lint_with_pack(load_builtin_pack(), &bundle_missing_a1_fields());
    assert!(has_rule_finding(
        &result,
        "owasp-agentic-control-evidence-baseline",
        "A1-002"
    ));
}

#[test]
fn c2_a1_any_of_fields_accept_single_governance_field() {
    let result = lint_with_pack(
        load_builtin_pack(),
        &bundle_a1_with_single_governance_field(),
    );
    assert!(
        !has_rule_finding(&result, "owasp-agentic-control-evidence-baseline", "A1-002"),
        "A1-002 should pass when one governance field is present"
    );
}

#[test]
fn c2_missing_authorization_context_fails_a3_001() {
    let result = lint_with_pack(load_builtin_pack(), &bundle_missing_a3_fields());
    assert!(has_rule_finding(
        &result,
        "owasp-agentic-control-evidence-baseline",
        "A3-001"
    ));
}

#[test]
fn c2_a3_any_of_fields_accept_single_authz_field() {
    let result = lint_with_pack(load_builtin_pack(), &bundle_a3_with_single_authz_field());
    assert!(
        !has_rule_finding(&result, "owasp-agentic-control-evidence-baseline", "A3-001"),
        "A3-001 should pass when one authorization field is present"
    );
}

#[test]
fn c2_missing_process_exec_fails_a5_001() {
    let result = lint_with_pack(load_builtin_pack(), &bundle_missing_a5_event());
    assert!(has_rule_finding(
        &result,
        "owasp-agentic-control-evidence-baseline",
        "A5-001"
    ));
}
