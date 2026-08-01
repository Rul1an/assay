//! Hermetic OTLP MCP Corpus Validator Tests
//!
//! Validates the locked fixture corpus in `tests/fixtures/otel-mcp-ingest-v0/` without
//! using any unbounded production parsers. Every vendored file, generator file, and corpus
//! fixture must match its locked hash. Typed errors never include user values.

mod support;
use support::otel_validator::validate_corpus_at_path;

use std::path::PathBuf;

const FIXTURE_ROOT: &str = "tests/fixtures/otel-mcp-ingest-v0";

fn root_path() -> PathBuf {
    PathBuf::from(FIXTURE_ROOT)
}

/// Reads a dotted JSON path from the lock file after corpus validation.
/// Only used for pin-assertion tests; the validator owns schema correctness.
fn read_lock_value(path: &[&str]) -> serde_json::Value {
    let root = root_path();
    validate_corpus_at_path(&root).expect("corpus must be valid before reading lock values");
    let content =
        std::fs::read_to_string(root.join("upstream.lock.json")).expect("lock file readable");
    let mut val: serde_json::Value = serde_json::from_str(&content).expect("lock file valid JSON");
    for &key in path {
        val = val.get(key).cloned().unwrap_or(serde_json::Value::Null);
    }
    val
}

#[test]
fn test_hermetic_corpus_validation() {
    let root = root_path();
    validate_corpus_at_path(&root).expect("corpus must be valid");
}

#[test]
fn test_lock_schema_version() {
    assert_eq!(read_lock_value(&["schema_version"]), "1");
}

#[test]
fn test_sdk_pinned() {
    assert_eq!(
        read_lock_value(&["sdk", "package"]),
        "@opentelemetry/sdk-trace-node"
    );
    assert_eq!(read_lock_value(&["sdk", "version"]), "2.10.0");
}

#[test]
fn test_exporter_pinned() {
    assert_eq!(
        read_lock_value(&["exporter", "package"]),
        "@opentelemetry/exporter-trace-otlp-http"
    );
    assert_eq!(read_lock_value(&["exporter", "version"]), "0.221.0");
}

#[test]
fn test_provenance_claims() {
    let note = read_lock_value(&["provenance", "note"]);
    let note_str = note.as_str().expect("provenance note must be a string");
    assert!(note_str.contains("Locally generated"));
    assert!(note_str.contains("Not external deployment"));
}
