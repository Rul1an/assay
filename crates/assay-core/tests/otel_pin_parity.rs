//! Emit and ingest share one GenAI semconv pin: a commit, not a floating label.

use std::collections::BTreeSet;
use std::path::PathBuf;

use assay_core::model::{TestResultRow, TestStatus};
use assay_core::otel::projection::project;
use assay_core::otel::{export_jsonl, export_tool_spans_jsonl, OTelConfig, ToolObservation};
use serde_json::{json, Value};

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn semconv_source(lock: &Value) -> &Value {
    lock["upstream_sources"]
        .as_array()
        .expect("upstream_sources")
        .iter()
        .find(|s| s["type"] == "semconv")
        .expect("semconv source")
}

fn ingest_lock() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(fixture_dir("otel-mcp-ingest-v0").join("upstream.lock.json"))
            .expect("ingest lock"),
    )
    .expect("ingest lock json")
}

fn emit_lock() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(fixture_dir("otel_projection").join("upstream.lock.json"))
            .expect("emit lock"),
    )
    .expect("emit lock json")
}

fn sample_row() -> TestResultRow {
    TestResultRow {
        test_id: "pin-parity".into(),
        status: TestStatus::Pass,
        score: Some(1.0),
        cached: false,
        message: "ok".into(),
        details: json!({}),
        duration_ms: Some(1),
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }
}

fn collect_genai_keys(value: &Value, into: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                if k.starts_with("gen_ai.") {
                    into.insert(k.clone());
                }
                collect_genai_keys(v, into);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_genai_keys(item, into);
            }
        }
        _ => {}
    }
}

fn emitted_jsonl_genai_keys() -> BTreeSet<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    let jsonl = dir.path().join("otel.jsonl");
    let cfg = OTelConfig {
        jsonl_path: Some(jsonl.clone()),
        redact_prompts: false,
    };
    export_jsonl(&cfg, "suite", &[sample_row()]).expect("export_jsonl");
    export_tool_spans_jsonl(
        &cfg,
        "run",
        &[ToolObservation {
            tool_name: "search".into(),
            claim_class_outcome: "supported".into(),
            subject: None,
        }],
    )
    .expect("export_tool_spans");

    let mut keys = BTreeSet::new();
    for line in std::fs::read_to_string(&jsonl)
        .expect("jsonl")
        .lines()
        .filter(|l| !l.is_empty())
    {
        let row: Value = serde_json::from_str(line).expect("jsonl row");
        collect_genai_keys(&row, &mut keys);
    }
    let projection = serde_json::to_value(project(
        &json!({
            "mcp_tools": ["search"],
            "policy_decisions": []
        }),
        None,
        None,
    ))
    .expect("projection value");
    collect_genai_keys(&projection, &mut keys);
    keys
}

#[test]
fn emit_and_ingest_share_one_semconv_pin() {
    let projection = project(&json!({}), None, None);
    let ingest = ingest_lock();
    let emit = emit_lock();
    let ingest_commit = semconv_source(&ingest)["commit"]
        .as_str()
        .expect("ingest commit");
    let emit_commit = semconv_source(&emit)["commit"]
        .as_str()
        .expect("emit commit");

    assert_eq!(
        emit_commit, ingest_commit,
        "emit and ingest lockfiles must name one commit"
    );
    assert_eq!(
        projection.semconv.otel_genai,
        format!("open-telemetry/semantic-conventions-genai@{ingest_commit}"),
        "projection.semconv.otel_genai must be the ingest pin, not a floating label"
    );
}

#[test]
fn emitted_genai_keys_exist_at_pinned_revision() {
    let lock = emit_lock();
    let commit = semconv_source(&lock)["commit"].as_str().expect("commit");
    assert_eq!(
        commit.len(),
        40,
        "pin must be a commit, not a version label such as 1.37.0-development"
    );
    assert!(
        commit.chars().all(|c| c.is_ascii_hexdigit()),
        "pin commit must be hex"
    );
    assert_ne!(commit, "1.37.0-development");

    for file in semconv_source(&lock)["files"].as_array().expect("files") {
        let sha = file["sha256"].as_str().expect("sha256");
        assert_eq!(sha.len(), 64, "vendored file hash must be sha256 hex");
    }

    let registered: BTreeSet<String> = lock["extracted_genai_keys"]
        .as_array()
        .expect("extracted_genai_keys")
        .iter()
        .map(|v| v.as_str().expect("key").to_string())
        .collect();
    assert!(
        registered.contains("gen_ai.provider.name"),
        "pinned table must include gen_ai.provider.name"
    );
    assert!(
        !registered.contains("gen_ai.system"),
        "retired gen_ai.system must not be in the pinned table"
    );
    assert!(
        !registered.contains("gen_ai.response.completion_tokens"),
        "unregistered token key must not be in the pinned table"
    );

    for key in emitted_jsonl_genai_keys() {
        assert!(
            registered.contains(&key),
            "{key} is not in the vendored registry at the pinned commit"
        );
    }
}
