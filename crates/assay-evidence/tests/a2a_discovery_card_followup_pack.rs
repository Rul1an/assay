//! P2c `a2a-discovery-card-followup` pack: built-in/open parity and G4-A discovery visibility (A2A-DC-001..002).

use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions, LintReportWithPacks};
use assay_evidence::lint::packs::loader::{load_pack, load_pack_from_file};
use assay_evidence::lint::packs::LoadedPack;
use assay_evidence::{BundleWriter, EvidenceEvent};
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn open_pack_path() -> PathBuf {
    repo_root()
        .join("packs")
        .join("open")
        .join("a2a-discovery-card-followup")
        .join("pack.yaml")
}

fn builtin_pack_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("packs")
        .join("a2a-discovery-card-followup.yaml")
}

fn load_open_pack() -> LoadedPack {
    load_pack_from_file(&open_pack_path()).expect("open pack should load")
}

fn load_builtin_pack() -> LoadedPack {
    load_pack("a2a-discovery-card-followup").expect("built-in pack should load")
}

fn make_event(type_: &str, run_id: &str, seq: u64, payload: Value) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(type_, "urn:assay:adapter:a2a", run_id, seq, payload);
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

fn lint_with_pack(pack: LoadedPack, bundle: &[u8]) -> LintReportWithPacks {
    let options = LintOptions {
        packs: vec![pack],
        max_results: Some(500),
        bundle_path: Some("p2c-pack.tar.gz".to_string()),
    };
    lint_bundle_with_options(
        Cursor::new(bundle),
        assay_evidence::VerifyLimits::default(),
        options,
    )
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

/// Minimal A2A payload matching adapter shape + G4-A `discovery` object on `data`.
fn a2a_payload_with_discovery(agent_card_visible: bool, extended_visible: bool) -> Value {
    json!({
        "adapter_id": "assay-adapter-a2a",
        "adapter_version": "0.0.1",
        "protocol": "a2a",
        "protocol_name": "a2a",
        "protocol_version": "0.2.0",
        "upstream_event_type": "agent.capabilities",
        "agent": { "id": "agent://planner", "capabilities": ["tasks.update"] },
        "discovery": {
            "agent_card_visible": agent_card_visible,
            "agent_card_source_kind": "attributes",
            "extended_card_access_visible": extended_visible,
            "signature_material_visible": false
        },
        "unmapped_fields_count": 0
    })
}

fn a2a_payload_no_discovery() -> Value {
    json!({
        "adapter_id": "assay-adapter-a2a",
        "adapter_version": "0.0.1",
        "protocol": "a2a",
        "protocol_name": "a2a",
        "protocol_version": "0.2.0",
        "upstream_event_type": "agent.capabilities",
        "agent": { "id": "agent://planner", "capabilities": ["tasks.update"] },
        "unmapped_fields_count": 0
    })
}

#[test]
fn a2a_discovery_followup_loads_builtin_and_open_with_two_rules() {
    let builtin = load_builtin_pack();
    let open = load_open_pack();
    assert_eq!(builtin.definition.name, "a2a-discovery-card-followup");
    assert_eq!(open.definition.name, "a2a-discovery-card-followup");
    assert_eq!(builtin.definition.rules.len(), 2);
    assert_eq!(open.definition.rules.len(), 2);
}

#[test]
fn a2a_discovery_builtin_and_open_pack_are_exactly_equivalent() {
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
        serde_json::to_value(&builtin.definition).expect("serialize builtin"),
        serde_json::to_value(&open.definition).expect("serialize open")
    );
}

#[test]
fn a2a_dc_both_pass_when_discovery_flags_true() {
    let bundle = make_bundle(vec![make_event(
        "assay.adapter.a2a.agent.capabilities",
        "run1",
        0,
        a2a_payload_with_discovery(true, true),
    )]);
    let result = lint_with_pack(load_builtin_pack(), &bundle);
    assert!(!has_rule_finding(
        &result,
        "a2a-discovery-card-followup",
        "A2A-DC-001"
    ));
    assert!(!has_rule_finding(
        &result,
        "a2a-discovery-card-followup",
        "A2A-DC-002"
    ));
}

#[test]
fn a2a_dc_001_fails_when_agent_card_false_even_if_extended_true() {
    let bundle = make_bundle(vec![make_event(
        "assay.adapter.a2a.agent.capabilities",
        "run1",
        0,
        a2a_payload_with_discovery(false, true),
    )]);
    let result = lint_with_pack(load_builtin_pack(), &bundle);
    assert!(has_rule_finding(
        &result,
        "a2a-discovery-card-followup",
        "A2A-DC-001"
    ));
    assert!(!has_rule_finding(
        &result,
        "a2a-discovery-card-followup",
        "A2A-DC-002"
    ));
}

#[test]
fn a2a_dc_both_fail_when_discovery_missing() {
    let bundle = make_bundle(vec![make_event(
        "assay.adapter.a2a.agent.capabilities",
        "run1",
        0,
        a2a_payload_no_discovery(),
    )]);
    let result = lint_with_pack(load_builtin_pack(), &bundle);
    assert!(has_rule_finding(
        &result,
        "a2a-discovery-card-followup",
        "A2A-DC-001"
    ));
    assert!(has_rule_finding(
        &result,
        "a2a-discovery-card-followup",
        "A2A-DC-002"
    ));
}

#[test]
fn a2a_dc_002_fails_when_extended_false() {
    let bundle = make_bundle(vec![make_event(
        "assay.adapter.a2a.agent.capabilities",
        "run1",
        0,
        a2a_payload_with_discovery(true, false),
    )]);
    let result = lint_with_pack(load_builtin_pack(), &bundle);
    assert!(!has_rule_finding(
        &result,
        "a2a-discovery-card-followup",
        "A2A-DC-001"
    ));
    assert!(has_rule_finding(
        &result,
        "a2a-discovery-card-followup",
        "A2A-DC-002"
    ));
}
