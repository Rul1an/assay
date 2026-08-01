//! Hermetic OTLP MCP Corpus Validator Tests
//!
//! Validates the locked fixture corpus in `tests/fixtures/otel-mcp-ingest-v0/` without
//! using any unbounded production parsers. Every vendored file, generator file, and corpus
//! fixture must match its locked hash. Typed errors never include user values.

mod support;
use support::otel_validator::{validate_corpus_at_path, ValidationError};

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

const FIXTURE_ROOT: &str = "tests/fixtures/otel-mcp-ingest-v0";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct UpstreamLock {
    schema_version: String,
    locked_at: String,
    sdk: SdkInfo,
    exporter: ExporterInfo,
    upstream_sources: Vec<UpstreamSource>,
    generator: GeneratorInfo,
    corpus: Vec<CorpusEntry>,
    hostile_fixtures: Vec<HostileEntry>,
    provenance: Provenance,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct SdkInfo {
    package: String,
    version: String,
    integrity: String,
    resolved: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ExporterInfo {
    package: String,
    version: String,
    integrity: String,
    resolved: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct UpstreamSource {
    #[serde(rename = "type")]
    source_type: String,
    repository: String,
    tag: Option<String>,
    commit: Option<String>,
    files: Vec<VendoredFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct VendoredFile {
    source_path: String,
    vendored_path: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct GeneratorInfo {
    directory: String,
    script: String,
    package_json_sha256: String,
    package_lock_sha256: String,
    script_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct CorpusEntry {
    fixture: String,
    sidecar: String,
    sidecar_sha256: String,
    content_sha256: String,
    byte_count: usize,
    span_kind: String,
    mcp_method: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct HostileEntry {
    fixture: String,
    sha256: String,
    purpose: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Provenance {
    note: String,
}

fn root_path() -> PathBuf {
    PathBuf::from(FIXTURE_ROOT)
}

fn load_lock() -> Result<UpstreamLock, ValidationError> {
    let path = root_path().join("upstream.lock.json");
    let content = fs::read_to_string(&path).map_err(|_| ValidationError::LockFileMissing)?;
    serde_json::from_str(&content).map_err(|_| ValidationError::LockParseError)
}

#[test]
fn test_hermetic_corpus_validation() {
    let root = root_path();
    validate_corpus_at_path(&root).expect("corpus must be valid");
}

#[test]
fn test_lock_schema_version() {
    let lock = load_lock().unwrap();
    assert_eq!(lock.schema_version, "1");
}

#[test]
fn test_sdk_pinned() {
    let lock = load_lock().unwrap();
    assert_eq!(lock.sdk.package, "@opentelemetry/sdk-trace-node");
    assert_eq!(lock.sdk.version, "2.10.0");
}

#[test]
fn test_exporter_pinned() {
    let lock = load_lock().unwrap();
    assert_eq!(
        lock.exporter.package,
        "@opentelemetry/exporter-trace-otlp-http"
    );
    assert_eq!(lock.exporter.version, "0.221.0");
}

#[test]
fn test_provenance_claims() {
    let lock = load_lock().unwrap();
    assert!(lock.provenance.note.contains("Locally generated"));
    assert!(lock.provenance.note.contains("Not external deployment"));
}
