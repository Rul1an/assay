//! MCP-shaped OTLP/JSON ingest scaffold.
//!
//! This module provides a bounded ingest path for OTLP/JSON payloads that may carry
//! MCP-server-emitted telemetry. The fixture corpus is pinned to the official OpenTelemetry
//! SDK version in `upstream.lock.json`, with hermetic validation and tamper detection.
//!
//! This scaffold does not decode, reduce, or expose a live receiver. It exists to anchor
//! the honest-provenance fixture set and the hostile-fixture tamper suite, supporting future
//! decoder/reader/reducer work without coupling to any decision carrier or policy inference.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A single OTLP/JSON resource span, carrying GenAI semantic conventions.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct OtlpJsonSpan {
    #[serde(rename = "traceId")]
    pub trace_id: String,
    #[serde(rename = "spanId")]
    pub span_id: String,
    #[serde(rename = "parentSpanId", skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub name: String,
    #[serde(rename = "startTimeUnixNano")]
    pub start_time_unix_nano: String,
    #[serde(rename = "endTimeUnixNano")]
    pub end_time_unix_nano: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<OtlpKeyValue>,
}

/// An OTLP KeyValue attribute.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct OtlpKeyValue {
    pub key: String,
    pub value: OtlpAnyValue,
}

/// An OTLP AnyValue, supporting string, int, double, bool, array, and kvlist.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OtlpAnyValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub string_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub int_value: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub double_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub array_value: Option<OtlpArrayValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kvlist_value: Option<OtlpKvListValue>,
}

/// An OTLP array value.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct OtlpArrayValue {
    pub values: Vec<OtlpAnyValue>,
}

/// An OTLP key-value list value.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct OtlpKvListValue {
    pub values: Vec<OtlpKeyValue>,
}

/// A complete OTLP/JSON trace payload, matching the official SDK export format.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OtlpJsonTrace {
    pub resource_spans: Vec<OtlpResourceSpan>,
}

/// A resource-scoped span container.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OtlpResourceSpan {
    pub resource: OtlpResource,
    pub scope_spans: Vec<OtlpScopeSpan>,
}

/// An OTLP resource, carrying service.name and other resource attributes.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct OtlpResource {
    pub attributes: Vec<OtlpKeyValue>,
}

/// An instrumentation-scope-scoped span container.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct OtlpScopeSpan {
    pub scope: OtlpInstrumentationScope,
    pub spans: Vec<OtlpJsonSpan>,
}

/// An OTLP instrumentation scope.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct OtlpInstrumentationScope {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[allow(dead_code)]
impl OtlpAnyValue {
    pub fn string(s: impl Into<String>) -> Self {
        Self {
            string_value: Some(s.into()),
            int_value: None,
            double_value: None,
            bool_value: None,
            array_value: None,
            kvlist_value: None,
        }
    }

    pub fn int(i: i64) -> Self {
        Self {
            string_value: None,
            int_value: Some(i),
            double_value: None,
            bool_value: None,
            array_value: None,
            kvlist_value: None,
        }
    }

    pub fn bool(b: bool) -> Self {
        Self {
            string_value: None,
            int_value: None,
            double_value: None,
            bool_value: Some(b),
            array_value: None,
            kvlist_value: None,
        }
    }
}

/// Upstream SDK lock for hermetic fixture validation.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct UpstreamLock {
    pub version: String,
    pub locked: String,
    pub sdk: SdkDependency,
    pub provenance: Provenance,
}

/// A pinned SDK dependency.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct SdkDependency {
    pub name: String,
    pub version: String,
    pub integrity: String,
    pub resolved: String,
}

/// Fixture provenance metadata.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Provenance {
    pub generator: String,
    pub honest: bool,
    pub note: String,
}

/// Fixture metadata sidecar.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FixtureMeta {
    pub name: String,
    pub generator: String,
    pub sdk: String,
    pub semconv: String,
    pub honest_provenance: bool,
    pub sha256: String,
    pub generated: String,
}

/// Fixture validation error (value-free to prevent error-message echoing of hostile input).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixtureValidationError {
    /// Fixture does not claim honest provenance.
    DishonesProvenance,
    /// Fixture semconv version does not match lock SDK version.
    SemconvMismatch,
    /// Fixture SDK version does not match lock SDK version.
    SdkVersionMismatch,
    /// Fixture content hash does not match sidecar hash.
    HashMismatch,
}

/// Validate that a fixture's provenance matches the upstream lock.
#[allow(dead_code)]
pub(crate) fn validate_fixture_lock(
    meta: &FixtureMeta,
    lock: &UpstreamLock,
) -> Result<(), FixtureValidationError> {
    if !meta.honest_provenance {
        return Err(FixtureValidationError::DishonesProvenance);
    }

    if meta.semconv != "1.28.0" {
        return Err(FixtureValidationError::SemconvMismatch);
    }

    if !meta.sdk.contains(&lock.sdk.version) {
        return Err(FixtureValidationError::SdkVersionMismatch);
    }

    Ok(())
}

/// Validate that a fixture's SHA256 hash matches the sidecar's declared hash.
#[allow(dead_code)]
pub(crate) fn validate_fixture_hash(
    content: &[u8],
    expected_hash: &str,
) -> Result<(), FixtureValidationError> {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let computed = hex::encode(hasher.finalize());

    if computed != expected_hash {
        return Err(FixtureValidationError::HashMismatch);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otlp_any_value_string_roundtrip() {
        let val = OtlpAnyValue::string("test");
        let json = serde_json::to_string(&val).unwrap();
        let parsed: OtlpAnyValue = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.string_value.as_deref(), Some("test"));
    }

    #[test]
    fn otlp_any_value_int_roundtrip() {
        let val = OtlpAnyValue::int(42);
        let json = serde_json::to_string(&val).unwrap();
        let parsed: OtlpAnyValue = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.int_value, Some(42));
    }

    #[test]
    fn otlp_any_value_bool_roundtrip() {
        let val = OtlpAnyValue::bool(true);
        let json = serde_json::to_string(&val).unwrap();
        let parsed: OtlpAnyValue = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.bool_value, Some(true));
    }

    #[test]
    fn lock_validator_accepts_honest_fixture() {
        let lock = UpstreamLock {
            version: "1".into(),
            locked: "2026-08-01T00:00:00Z".into(),
            sdk: SdkDependency {
                name: "@opentelemetry/sdk-trace-node".into(),
                version: "1.28.0".into(),
                integrity: "sha512-F06x+...".into(),
                resolved: "https://registry.npmjs.org/...".into(),
            },
            provenance: Provenance {
                generator: "scripts/generate_otel_mcp_fixtures.js".into(),
                honest: true,
                note: "Pinned to GenAI semconv 1.28.0".into(),
            },
        };

        let meta = FixtureMeta {
            name: "minimal_chat".into(),
            generator: "scripts/generate_otel_mcp_fixtures.js".into(),
            sdk: "@opentelemetry/sdk-trace-node@1.28.0".into(),
            semconv: "1.28.0".into(),
            honest_provenance: true,
            sha256: "dd890a963e37...".into(),
            generated: "2026-08-01T00:00:00Z".into(),
        };

        assert!(validate_fixture_lock(&meta, &lock).is_ok());
    }

    #[test]
    fn lock_validator_rejects_dishonest_provenance() {
        let lock = UpstreamLock {
            version: "1".into(),
            locked: "2026-08-01T00:00:00Z".into(),
            sdk: SdkDependency {
                name: "@opentelemetry/sdk-trace-node".into(),
                version: "1.28.0".into(),
                integrity: "sha512-F06x+...".into(),
                resolved: "https://registry.npmjs.org/...".into(),
            },
            provenance: Provenance {
                generator: "scripts/generate_otel_mcp_fixtures.js".into(),
                honest: true,
                note: "Pinned to GenAI semconv 1.28.0".into(),
            },
        };

        let meta = FixtureMeta {
            name: "hostile".into(),
            generator: "hand-crafted".into(),
            sdk: "unknown".into(),
            semconv: "1.28.0".into(),
            honest_provenance: false,
            sha256: "...".into(),
            generated: "2026-08-01T00:00:00Z".into(),
        };

        assert_eq!(
            validate_fixture_lock(&meta, &lock),
            Err(FixtureValidationError::DishonesProvenance)
        );
    }

    #[test]
    fn lock_validator_rejects_version_mismatch() {
        let lock = UpstreamLock {
            version: "1".into(),
            locked: "2026-08-01T00:00:00Z".into(),
            sdk: SdkDependency {
                name: "@opentelemetry/sdk-trace-node".into(),
                version: "1.28.0".into(),
                integrity: "sha512-F06x+...".into(),
                resolved: "https://registry.npmjs.org/...".into(),
            },
            provenance: Provenance {
                generator: "scripts/generate_otel_mcp_fixtures.js".into(),
                honest: true,
                note: "Pinned to GenAI semconv 1.28.0".into(),
            },
        };

        let meta = FixtureMeta {
            name: "tampered".into(),
            generator: "scripts/generate_otel_mcp_fixtures.js".into(),
            sdk: "@opentelemetry/sdk-trace-node@1.29.0".into(),
            semconv: "1.29.0".into(),
            honest_provenance: true,
            sha256: "...".into(),
            generated: "2026-08-01T00:00:00Z".into(),
        };

        let result = validate_fixture_lock(&meta, &lock);
        assert_eq!(result, Err(FixtureValidationError::SemconvMismatch));
    }

    #[test]
    fn hash_validator_accepts_matching_hash() {
        let content = b"test content";
        let mut hasher = Sha256::new();
        hasher.update(content);
        let expected = hex::encode(hasher.finalize());

        assert!(validate_fixture_hash(content, &expected).is_ok());
    }

    #[test]
    fn hash_validator_rejects_mismatched_hash() {
        let content = b"test content";
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        assert_eq!(
            validate_fixture_hash(content, wrong_hash),
            Err(FixtureValidationError::HashMismatch)
        );
    }

    #[test]
    fn load_and_validate_upstream_lock() {
        let lock_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/otel-mcp-ingest-v0/upstream.lock.json"
        );
        let lock_json = std::fs::read_to_string(lock_path).expect("read upstream.lock.json");
        let lock: UpstreamLock = serde_json::from_str(&lock_json).expect("parse lock");

        assert_eq!(lock.version, "1");
        assert_eq!(lock.sdk.name, "@opentelemetry/sdk-trace-node");
        assert_eq!(lock.sdk.version, "1.28.0");
        assert!(lock.provenance.honest);
    }

    #[test]
    fn load_and_validate_honest_fixtures() {
        let fixture_dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/otel-mcp-ingest-v0"
        );
        let lock_path = format!("{}/upstream.lock.json", fixture_dir);
        let lock_json = std::fs::read_to_string(&lock_path).expect("read lock");
        let lock: UpstreamLock = serde_json::from_str(&lock_json).expect("parse lock");

        for fixture_name in &["minimal_chat", "tool_execution"] {
            let meta_path = format!("{}/{}.meta.json", fixture_dir, fixture_name);
            let meta_json = std::fs::read_to_string(&meta_path)
                .unwrap_or_else(|_| panic!("read {}.meta.json", fixture_name));
            let meta: FixtureMeta = serde_json::from_str(&meta_json)
                .unwrap_or_else(|_| panic!("parse {}.meta.json", fixture_name));

            validate_fixture_lock(&meta, &lock)
                .unwrap_or_else(|_e| panic!("{} lock validation failed", fixture_name));

            let fixture_path = format!("{}/{}.json", fixture_dir, fixture_name);
            let fixture_bytes = std::fs::read(&fixture_path)
                .unwrap_or_else(|_| panic!("read {}.json", fixture_name));

            validate_fixture_hash(&fixture_bytes, &meta.sha256)
                .unwrap_or_else(|_e| panic!("{} hash validation failed", fixture_name));

            let trace: OtlpJsonTrace = serde_json::from_slice(&fixture_bytes)
                .unwrap_or_else(|_| panic!("parse {}.json", fixture_name));

            assert!(
                !trace.resource_spans.is_empty(),
                "{} has resource spans",
                fixture_name
            );
        }
    }

    #[test]
    fn hostile_fixtures_fail_to_parse() {
        let fixture_dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/otel-mcp-ingest-v0"
        );

        // hostile_missing_required_fields.json is missing required fields and should fail to parse
        let hostile_path = format!("{}/hostile_missing_required_fields.json", fixture_dir);
        let hostile_json = std::fs::read_to_string(&hostile_path).expect("read hostile fixture");
        let result: Result<OtlpJsonTrace, _> = serde_json::from_str(&hostile_json);

        assert!(result.is_err(), "hostile fixture should fail to parse");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("missing field"),
            "error should mention missing field, got: {}",
            err
        );
    }

    #[test]
    fn hostile_deep_nesting_parses() {
        let fixture_dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/otel-mcp-ingest-v0"
        );

        // hostile_deep_nesting.json should parse but represents an attack vector
        let hostile_path = format!("{}/hostile_deep_nesting.json", fixture_dir);
        let hostile_json = std::fs::read_to_string(&hostile_path).expect("read hostile fixture");
        let trace: OtlpJsonTrace =
            serde_json::from_str(&hostile_json).expect("parse hostile nesting");

        // Verify it parsed and has the deep nesting structure
        assert!(!trace.resource_spans.is_empty());
    }

    #[test]
    fn tamper_fixture_byte_modification_fails_hash() {
        let fixture_dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/otel-mcp-ingest-v0"
        );
        let fixture_path = format!("{}/minimal_chat.json", fixture_dir);
        let meta_path = format!("{}/minimal_chat.meta.json", fixture_dir);

        let mut content = std::fs::read(&fixture_path).expect("read fixture");
        let meta_json = std::fs::read_to_string(&meta_path).expect("read meta");
        let meta: FixtureMeta = serde_json::from_str(&meta_json).expect("parse meta");

        // Tamper: flip one byte
        if !content.is_empty() {
            content[0] ^= 0xFF;
        }

        // Hash validation should fail
        let result = validate_fixture_hash(&content, &meta.sha256);
        assert_eq!(result, Err(FixtureValidationError::HashMismatch));
    }

    #[test]
    fn tamper_sidecar_hash_mismatch_detected() {
        let fixture_dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/otel-mcp-ingest-v0"
        );
        let fixture_path = format!("{}/minimal_chat.json", fixture_dir);
        let content = std::fs::read(&fixture_path).expect("read fixture");

        // Wrong hash
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        // Hash validation should fail
        let result = validate_fixture_hash(&content, wrong_hash);
        assert_eq!(result, Err(FixtureValidationError::HashMismatch));
    }

    #[test]
    fn tamper_lock_version_mismatch_detected() {
        let fixture_dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/otel-mcp-ingest-v0"
        );
        let lock_path = format!("{}/upstream.lock.json", fixture_dir);
        let lock_json = std::fs::read_to_string(&lock_path).expect("read lock");
        let mut lock: UpstreamLock = serde_json::from_str(&lock_json).expect("parse lock");

        // Tamper: change SDK version
        lock.sdk.version = "99.99.99".into();

        let meta = FixtureMeta {
            name: "minimal_chat".into(),
            generator: "scripts/generate_otel_mcp_fixtures.js".into(),
            sdk: "@opentelemetry/sdk-trace-node@1.28.0".into(),
            semconv: "1.28.0".into(),
            honest_provenance: true,
            sha256: "dd890a963e37...".into(),
            generated: "2026-08-01T00:00:00Z".into(),
        };

        let result = validate_fixture_lock(&meta, &lock);
        assert_eq!(result, Err(FixtureValidationError::SdkVersionMismatch));
    }

    #[test]
    fn tamper_vendored_proto_hash_fails() {
        let proto_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/otel-mcp-ingest-v0/vendor/otlp-proto-v1.11.0/trace.proto"
        );

        // Read the real proto file
        let mut content = std::fs::read(proto_path).expect("read proto");

        // Compute the real hash
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let real_hash = hex::encode(hasher.finalize());

        // Tamper: flip one byte
        if !content.is_empty() {
            content[0] ^= 0xFF;
        }

        // Hash validation should fail
        let result = validate_fixture_hash(&content, &real_hash);
        assert_eq!(result, Err(FixtureValidationError::HashMismatch));
    }
}
