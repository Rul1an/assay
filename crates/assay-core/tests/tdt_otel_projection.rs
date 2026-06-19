//! Blessed snapshot for the EXPERIMENTAL tool-decision-truth OTel projection.
//!
//! Builds verified `TdtDecision`s from real carriers and pins the projected output byte-for-byte, so any
//! drift in the span shape or the `assay.tdt.*` attribute set is caught. The carriers are keyed with a
//! fixed test key, so the digests (and therefore the projection) are deterministic. Regenerate with
//! `BLESS=1 cargo test -p assay-core --test tdt_otel_projection`.

use assay_core::mcp::policy::McpPolicy;
use assay_core::mcp::tool_decision_truth::{self as tdt, DecisionEvidence};
use assay_core::otel::projection::{project_tool_decision_truth, TdtDecision};
use serde_json::{json, Value};
use std::path::PathBuf;

const KEY: &[u8] = b"tdt-otel-snapshot-key-v0";
const KID: &str = "fixture-kid-v0";

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tdt_otel_projection/expected.json")
}

fn policy() -> McpPolicy {
    serde_json::from_value(json!({
        "version": "1",
        "tools": {"allow": ["deploy"], "deny": ["delete_all"]},
        "schemas": {"deploy": {"type": "object", "required": ["env"],
            "properties": {"env": {"enum": ["staging", "prod"]}}}},
        "enforcement": {"unconstrained_tools": "warn"}
    }))
    .unwrap()
}

fn decision(tool: &str, args: Value, order: i64, call_id: &str) -> TdtDecision {
    let carrier = tdt::build_classified_record(
        &policy(),
        tool,
        &args,
        order,
        KEY,
        KID,
        "authoritative_boundary",
        call_id,
        "ok",
        "present",
        &DecisionEvidence::default(),
    )
    .unwrap();
    let carrier_content_digest = tdt::carrier_content_digest(&carrier).unwrap();
    let decision_identity_digest = tdt::decision_identity_digest(
        carrier["observed_input_digest"].as_str().unwrap(),
        carrier["declared_policy_digest"].as_str().unwrap(),
    )
    .unwrap();
    let run_verdict = carrier["decision_verdict"].as_str().unwrap().to_string();
    TdtDecision {
        carrier,
        carrier_content_digest,
        decision_identity_digest,
        run_verdict,
    }
}

fn decisions() -> Vec<TdtDecision> {
    vec![
        decision("deploy", json!({"env": "prod"}), 0, "c0"), // match
        decision("delete_all", json!({}), 1, "c1"),          // mismatch
    ]
}

#[test]
fn golden_projection_roundtrip() {
    let fresh = serde_json::to_value(project_tool_decision_truth(&decisions())).unwrap();
    let path = fixture_path();
    if std::env::var("BLESS").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&fresh).unwrap()),
        )
        .unwrap();
    }
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(&path).expect("expected.json present (regenerate with BLESS=1)"),
    )
    .unwrap();
    assert_eq!(
        fresh, expected,
        "TDT OTel projection drifted from the committed golden; regenerate with BLESS=1"
    );

    // Belt-and-suspenders: the golden carries the honesty contract and never raw args.
    assert_eq!(expected["lossy"], json!(true));
    assert_eq!(expected["source_of_truth"], json!("assay artifacts"));
    let serialized = serde_json::to_string(&expected).unwrap();
    assert!(!serialized.contains("\"arguments\""));
    for span in expected["spans"].as_array().unwrap() {
        assert_eq!(span["attributes"]["assay.claim_class"], json!("derived"));
        assert_eq!(span["attributes"]["openinference.span.kind"], json!("TOOL"));
    }
}
