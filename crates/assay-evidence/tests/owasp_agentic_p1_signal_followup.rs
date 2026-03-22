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
        .join("owasp-agentic-a3-a5-signal-followup")
        .join("pack.yaml")
}

fn readme_path() -> PathBuf {
    repo_root()
        .join("packs")
        .join("open")
        .join("owasp-agentic-a3-a5-signal-followup")
        .join("README.md")
}

fn builtin_pack_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("packs")
        .join("owasp-agentic-a3-a5-signal-followup.yaml")
}

fn load_open_pack() -> LoadedPack {
    load_pack_from_file(&open_pack_path()).expect("open pack should load")
}

fn load_builtin_pack() -> LoadedPack {
    load_pack("owasp-agentic-a3-a5-signal-followup").expect("built-in pack should load")
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
    let mut event = EvidenceEvent::new(type_, "urn:assay:test:p1", run_id, seq, payload);
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

fn supported_delegated_flow_bundle() -> Vec<u8> {
    make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_p1_a3_pass",
        0,
        json!({
            "tool": "tool.commit",
            "decision": "allow",
            "principal": "user:alice",
            "approval_state": "granted",
            "delegated_from": "agent:planner",
            "delegation_depth": 1
        }),
    )])
}

fn direct_authorization_bundle() -> Vec<u8> {
    make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_p1_a3_fail",
        0,
        json!({
            "tool": "tool.commit",
            "decision": "allow",
            "principal": "user:alice",
            "approval_state": "granted"
        }),
    )])
}

fn supported_degraded_bundle() -> Vec<u8> {
    make_bundle(vec![make_event(
        "assay.sandbox.degraded",
        "run_p1_a5_pass",
        0,
        json!({
            "reason_code": "policy_conflict",
            "degradation_mode": "audit_fallback",
            "component": "landlock"
        }),
    )])
}

fn clean_bundle() -> Vec<u8> {
    make_bundle(vec![make_event(
        "assay.process.exec",
        "run_p1_a5_fail",
        0,
        json!({
            "hits": 1
        }),
    )])
}

fn lint_with_pack(pack: LoadedPack, bundle: &[u8]) -> LintReportWithPacks {
    let options = LintOptions {
        packs: vec![pack],
        max_results: Some(500),
        bundle_path: Some("p1-pack.tar.gz".to_string()),
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
fn p1_loads_builtin_and_open_pack_with_two_rules() {
    let builtin = load_builtin_pack();
    let open = load_open_pack();

    assert_eq!(
        builtin.definition.name,
        "owasp-agentic-a3-a5-signal-followup"
    );
    assert_eq!(open.definition.name, "owasp-agentic-a3-a5-signal-followup");
    assert_eq!(builtin.definition.rules.len(), 2);
    assert_eq!(open.definition.rules.len(), 2);
}

#[test]
fn p1_builtin_and_open_pack_are_exactly_equivalent() {
    let builtin_raw = fs::read_to_string(builtin_pack_path()).expect("read builtin yaml");
    let open_raw = fs::read_to_string(open_pack_path()).expect("read open yaml");
    assert_eq!(
        builtin_raw, open_raw,
        "open pack and built-in mirror must match exactly"
    );

    let builtin = load_builtin_pack();
    let open = load_open_pack();
    assert_eq!(builtin.digest, open.digest);
    assert_eq!(
        serde_json::to_value(&builtin.definition).expect("serialize builtin definition"),
        serde_json::to_value(&open.definition).expect("serialize open definition")
    );
}

#[test]
fn p1_pack_contains_only_signal_aware_rules() {
    let pack = load_builtin_pack();
    assert_eq!(canonical_rule_ids(&pack), vec!["A3-003", "A5-002"]);
}

#[test]
fn p1_baseline_pack_remains_unchanged() {
    let baseline =
        load_pack("owasp-agentic-control-evidence-baseline").expect("baseline pack should load");
    assert_eq!(
        canonical_rule_ids(&baseline),
        vec!["A1-002", "A3-001", "A5-001"]
    );
}

#[test]
fn p1_pack_uses_only_supported_non_skip_prone_constructs() {
    let pack = load_builtin_pack();
    let a3 = pack
        .definition
        .rules
        .iter()
        .find(|rule| rule.id == "A3-003")
        .expect("A3-003 rule");
    let a5 = pack
        .definition
        .rules
        .iter()
        .find(|rule| rule.id == "A5-002")
        .expect("A5-002 rule");

    for rule in &pack.definition.rules {
        assert!(rule.engine_min_version.is_none());
        assert!(!rule.check.is_unsupported());
    }

    assert!(matches!(
        a3.check,
        CheckDefinition::EventFieldPresent { .. }
    ));
    assert_eq!(
        a3.event_types.as_ref().expect("A3-003 event types"),
        &vec!["assay.tool.decision".to_string()]
    );
    assert!(matches!(a5.check, CheckDefinition::EventTypeExists { .. }));
}

#[test]
fn p1_readme_explicitly_states_non_goals() {
    let readme = fs::read_to_string(readme_path()).expect("readme should be readable");
    assert!(readme.contains("## Non-Goals"));
    assert!(readme.contains("supported delegated flows"));
    assert!(readme.contains("supported containment fallback paths"));

    for phrase in [
        "delegation chain integrity",
        "delegation validity",
        "inherited-scope correctness",
        "temporal delegation correctness",
        "sandbox correctness",
        "all containment failures detected",
    ] {
        assert_eq!(
            count_case_insensitive(&readme, phrase),
            1,
            "README should mention non-goal phrase exactly once: {phrase}"
        );
    }

    assert!(normalize_space_text(&readme).contains(
        "this pack proves only signal-aware evidence for supported delegated flows and supported containment fallback paths. it does not validate delegation chains, cryptographic provenance, inherited scopes, temporal authorization, or overall containment guarantees."
    ));
}

#[test]
fn p1_pack_wording_stays_signal_only() {
    let open_yaml = fs::read_to_string(open_pack_path()).expect("open pack yaml");
    let readme = fs::read_to_string(readme_path()).expect("readme");
    let normalized_text = normalize_text(&(open_yaml + "\n" + &readme));

    for forbidden in [
        "verifies delegation",
        "guarantees chain integrity",
        "validates inherited scopes",
        "proves sandboxing",
        "all containment failures detected.",
    ] {
        assert!(
            !normalized_text.contains(forbidden),
            "companion pack wording must stay evidence-only: {forbidden}"
        );
    }
}

#[test]
fn p1_a3_003_passes_when_supported_delegation_fields_are_present() {
    let result = lint_with_pack(load_builtin_pack(), &supported_delegated_flow_bundle());
    assert!(
        !has_rule_finding(&result, "owasp-agentic-a3-a5-signal-followup", "A3-003"),
        "A3-003 should pass when delegated_from is present on supported decision evidence"
    );
}

#[test]
fn p1_a3_003_fails_when_supported_delegation_fields_are_absent() {
    let result = lint_with_pack(load_builtin_pack(), &direct_authorization_bundle());
    assert!(
        has_rule_finding(&result, "owasp-agentic-a3-a5-signal-followup", "A3-003"),
        "A3-003 should fail for direct flows without delegated_from"
    );
}

#[test]
fn p1_a5_002_passes_when_sandbox_degraded_event_is_present() {
    let result = lint_with_pack(load_builtin_pack(), &supported_degraded_bundle());
    assert!(
        !has_rule_finding(&result, "owasp-agentic-a3-a5-signal-followup", "A5-002"),
        "A5-002 should pass when assay.sandbox.degraded is present"
    );
}

#[test]
fn p1_a5_002_fails_when_supported_degradation_signal_is_absent() {
    let result = lint_with_pack(load_builtin_pack(), &clean_bundle());
    assert!(
        has_rule_finding(&result, "owasp-agentic-a3-a5-signal-followup", "A5-002"),
        "A5-002 should fail when the supported degradation signal is absent"
    );
}
