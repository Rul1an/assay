//! P2b `a2a-signal-followup` pack: built-in/open parity and presence rules (A2A-001..003).

use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions, LintReportWithPacks};
use assay_evidence::lint::packs::loader::{load_pack, load_pack_from_file};
use assay_evidence::lint::packs::LoadedPack;
use assay_evidence::{BundleWriter, EvidenceEvent};
use chrono::{TimeZone, Utc};
use serde_json::json;
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
        .join("a2a-signal-followup")
        .join("pack.yaml")
}

fn readme_path() -> PathBuf {
    repo_root()
        .join("packs")
        .join("open")
        .join("a2a-signal-followup")
        .join("README.md")
}

fn builtin_pack_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("packs")
        .join("a2a-signal-followup.yaml")
}

fn load_open_pack() -> LoadedPack {
    load_pack_from_file(&open_pack_path()).expect("open pack should load")
}

fn load_builtin_pack() -> LoadedPack {
    load_pack("a2a-signal-followup").expect("built-in pack should load")
}

fn make_event(type_: &str, run_id: &str, seq: u64, payload: serde_json::Value) -> EvidenceEvent {
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
        bundle_path: Some("a2a-pack.tar.gz".to_string()),
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

fn a2a_payload_cap() -> serde_json::Value {
    json!({
        "adapter_id": "assay-adapter-a2a",
        "adapter_version": "0.0.1",
        "protocol": "a2a",
        "protocol_name": "a2a",
        "protocol_version": "0.2.0",
        "upstream_event_type": "agent.capabilities",
        "agent": {
            "id": "agent://planner",
            "capabilities": ["tasks.update"]
        }
    })
}

fn a2a_payload_task() -> serde_json::Value {
    json!({
        "adapter_id": "assay-adapter-a2a",
        "adapter_version": "0.0.1",
        "protocol": "a2a",
        "protocol_name": "a2a",
        "protocol_version": "0.2.0",
        "upstream_event_type": "task.updated",
        "agent": { "id": "agent://x" },
        "task": { "id": "task-1", "status": "running" }
    })
}

fn a2a_payload_artifact() -> serde_json::Value {
    json!({
        "adapter_id": "assay-adapter-a2a",
        "adapter_version": "0.0.1",
        "protocol": "a2a",
        "protocol_name": "a2a",
        "protocol_version": "0.3.1",
        "upstream_event_type": "artifact.shared",
        "agent": { "id": "agent://x" },
        "task": { "id": "task-1" },
        "artifact": { "id": "art-1", "name": "f.md", "media_type": "text/markdown" }
    })
}

/// All three canonical surfaces present — pack should pass.
fn full_a2a_bundle() -> Vec<u8> {
    make_bundle(vec![
        make_event(
            "assay.adapter.a2a.agent.capabilities",
            "a2a_full",
            0,
            a2a_payload_cap(),
        ),
        make_event(
            "assay.adapter.a2a.task.updated",
            "a2a_full",
            1,
            a2a_payload_task(),
        ),
        make_event(
            "assay.adapter.a2a.artifact.shared",
            "a2a_full",
            2,
            a2a_payload_artifact(),
        ),
    ])
}

#[test]
fn a2a_followup_loads_builtin_and_open_with_three_rules() {
    let builtin = load_builtin_pack();
    let open = load_open_pack();
    assert_eq!(builtin.definition.name, "a2a-signal-followup");
    assert_eq!(open.definition.name, "a2a-signal-followup");
    assert_eq!(builtin.definition.rules.len(), 3);
    assert_eq!(open.definition.rules.len(), 3);
}

#[test]
fn a2a_followup_builtin_and_open_pack_are_exactly_equivalent() {
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
fn a2a_all_rules_pass_when_all_signals_present() {
    let bundle = full_a2a_bundle();
    let result = lint_with_pack(load_builtin_pack(), &bundle);
    assert!(!has_rule_finding(&result, "a2a-signal-followup", "A2A-001"));
    assert!(!has_rule_finding(&result, "a2a-signal-followup", "A2A-002"));
    assert!(!has_rule_finding(&result, "a2a-signal-followup", "A2A-003"));
}

#[test]
fn a2a_001_fails_without_capabilities_event() {
    let bundle = make_bundle(vec![
        make_event(
            "assay.adapter.a2a.task.requested",
            "r",
            0,
            a2a_payload_task(),
        ),
        make_event(
            "assay.adapter.a2a.artifact.shared",
            "r",
            1,
            a2a_payload_artifact(),
        ),
    ]);
    let result = lint_with_pack(load_builtin_pack(), &bundle);
    assert!(has_rule_finding(&result, "a2a-signal-followup", "A2A-001"));
    assert!(!has_rule_finding(&result, "a2a-signal-followup", "A2A-002"));
    assert!(!has_rule_finding(&result, "a2a-signal-followup", "A2A-003"));
}

#[test]
fn a2a_002_accepts_task_requested() {
    let bundle = make_bundle(vec![
        make_event(
            "assay.adapter.a2a.agent.capabilities",
            "r",
            0,
            a2a_payload_cap(),
        ),
        make_event(
            "assay.adapter.a2a.task.requested",
            "r",
            1,
            a2a_payload_task(),
        ),
        make_event(
            "assay.adapter.a2a.artifact.shared",
            "r",
            2,
            a2a_payload_artifact(),
        ),
    ]);
    let result = lint_with_pack(load_builtin_pack(), &bundle);
    assert!(!has_rule_finding(&result, "a2a-signal-followup", "A2A-002"));
}

#[test]
fn a2a_003_fails_without_artifact_event() {
    let bundle = make_bundle(vec![
        make_event(
            "assay.adapter.a2a.agent.capabilities",
            "r",
            0,
            a2a_payload_cap(),
        ),
        make_event("assay.adapter.a2a.task.updated", "r", 1, a2a_payload_task()),
    ]);
    let result = lint_with_pack(load_builtin_pack(), &bundle);
    assert!(has_rule_finding(&result, "a2a-signal-followup", "A2A-003"));
}

#[test]
fn a2a_readme_lists_non_goals() {
    let readme = fs::read_to_string(readme_path()).expect("readme");
    assert!(readme.contains("## Non-Goals"));
    assert!(readme.contains("authorization validity"));
}
