use assay_evidence::{BundleWriter, EvidenceEvent, TRUST_CARD_NON_GOALS};
use assert_cmd::Command;
use chrono::{TimeZone, Utc};
use serde_json::json;
use std::fs;

fn make_event(type_: &str, run_id: &str, seq: u64, payload: serde_json::Value) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(type_, "urn:assay:test:trustcard-cli", run_id, seq, payload);
    event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
    event
}

fn write_bundle(path: &std::path::Path, events: Vec<EvidenceEvent>) {
    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    for event in events {
        writer.add_event(event);
    }
    writer.finish().unwrap();
}

#[test]
fn trustcard_generate_writes_json_and_md_matching_trust_basis_claims() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("bundle.tar.gz");
    let out_dir = dir.path().join("out");
    write_bundle(
        &bundle,
        vec![make_event(
            "assay.tool.decision",
            "run_tc",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "delegated_from": "agent:planner"
            }),
        )],
    );

    let out = Command::cargo_bin("assay")
        .unwrap()
        .arg("trustcard")
        .arg("generate")
        .arg(&bundle)
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let basis_out = Command::cargo_bin("assay")
        .unwrap()
        .arg("trust-basis")
        .arg("generate")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(basis_out.status.success());
    let basis_json: serde_json::Value = serde_json::from_slice(&basis_out.stdout).unwrap();

    let card_json: serde_json::Value =
        serde_json::from_slice(&fs::read(out_dir.join("trustcard.json")).unwrap()).unwrap();

    assert_eq!(card_json["schema_version"], json!(5));
    assert_eq!(card_json["claims"], basis_json["claims"]);

    let claims = card_json["claims"].as_array().expect("claims array");
    assert_eq!(
        claims.len(),
        10,
        "trustcard must carry exactly ten frozen claims"
    );
    let expected_ids = [
        "bundle_verified",
        "signing_evidence_present",
        "provenance_backed_claims_present",
        "delegation_context_visible",
        "authorization_context_visible",
        "containment_degradation_observed",
        "external_eval_receipt_boundary_visible",
        "external_decision_receipt_boundary_visible",
        "external_inventory_receipt_boundary_visible",
        "applied_pack_findings_present",
    ];
    let ids: Vec<_> = claims
        .iter()
        .map(|c| c["id"].as_str().expect("id"))
        .collect();
    assert_eq!(
        ids,
        expected_ids.to_vec(),
        "claim id order must match T1a / generate_trust_basis"
    );

    let obj = card_json.as_object().expect("root object");
    let mut root_keys: Vec<_> = obj.keys().map(String::as_str).collect();
    root_keys.sort_unstable();
    assert_eq!(
        root_keys,
        vec!["claims", "non_goals", "schema_version"],
        "trustcard.json must not add hashes, section_id, summary fields, etc."
    );

    assert_eq!(
        card_json["non_goals"],
        serde_json::to_value(TRUST_CARD_NON_GOALS.as_slice()).unwrap()
    );

    let md = String::from_utf8(fs::read(out_dir.join("trustcard.md")).unwrap()).unwrap();
    assert!(md.contains("## Non-goals"));
    for line in TRUST_CARD_NON_GOALS {
        assert!(md.contains(&format!("- {line}")));
    }
    assert!(md.contains("| id | level | source | boundary | note |"));

    let html = String::from_utf8(fs::read(out_dir.join("trustcard.html")).unwrap()).unwrap();
    assert!(html.starts_with("<!doctype html>\n"));
    assert_eq!(
        html.matches("data-claim-id=").count(),
        claims.len(),
        "trustcard.html must render the same claim rows as trustcard.json"
    );
    for id in expected_ids {
        assert!(
            html.contains(&format!("data-claim-id=\"{id}\"")),
            "missing html claim row for {id}"
        );
    }
    for line in TRUST_CARD_NON_GOALS {
        assert!(html.contains(&format!("<li>{line}</li>")));
    }
    assert!(
        html.contains("Canonical source of truth: trustcard.json"),
        "HTML must keep JSON canonical"
    );
    assert!(
        !html.contains("<script") && !html.contains("https://") && !html.contains("http://"),
        "trustcard.html must be a static no-network projection"
    );
}

#[test]
fn trustcard_generate_pack_flag_matches_trust_basis_pack_classification() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("bundle-pack.tar.gz");
    let out_dir = dir.path().join("out-pack");
    write_bundle(
        &bundle,
        vec![make_event(
            "assay.tool.decision",
            "run_pack_tc",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "principal": "user:alice"
            }),
        )],
    );

    let out = Command::cargo_bin("assay")
        .unwrap()
        .arg("trustcard")
        .arg("generate")
        .arg(&bundle)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--pack")
        .arg("owasp-agentic-a3-a5-signal-followup")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let card_json: serde_json::Value =
        serde_json::from_slice(&fs::read(out_dir.join("trustcard.json")).unwrap()).unwrap();
    let claims = card_json["claims"].as_array().unwrap();
    let pack_claim = claims
        .iter()
        .find(|c| c["id"] == "applied_pack_findings_present")
        .expect("claim");
    assert_eq!(pack_claim["level"], "verified");
}
