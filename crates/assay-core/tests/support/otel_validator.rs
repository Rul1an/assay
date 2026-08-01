//! Hermetic OTLP MCP Corpus Validator
//!
//! Validates the locked fixture corpus in `tests/fixtures/otel-mcp-ingest-v0/` without
//! using any unbounded production parsers. Every vendored file, generator file, and corpus
//! fixture must match its locked hash. Typed errors never include user values.

use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    LockFileMissing,
    LockParseError,
    VendoredFileMissing,
    VendoredHashMismatch,
    VendoredDuplicateFile,
    GeneratorFileMissing,
    GeneratorHashMismatch,
    FixtureMissing,
    FixtureHashMismatch,
    FixtureDuplicatePath,
    FixtureSemanticMismatch,
    FixtureMissingRequiredAttribute,
    FixtureDuplicateAttribute,
    FixtureInvalidSpanKind,
    FixtureSpanCountMismatch,
    SidecarMissing,
    SidecarHashMismatch,
    SidecarParseError,
    SidecarSemanticMismatch,
    HostileMissing,
    HostileHashMismatch,
    HostileDuplicatePath,
    UnlistedFileInCorpus,
    ExternalDeploymentTrue,
    PathTraversal,
    PackageLockMismatch,
    UpstreamSourceDuplicate,
    SchemaVersionInvalid,
    ProvenanceMarkerInvalid,
    UpstreamSourceTypeInvalid,
    UpstreamSourceRepositoryInvalid,
    UpstreamSourceCardinalityInvalid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
struct SdkInfo {
    package: String,
    version: String,
    integrity: String,
    resolved: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExporterInfo {
    package: String,
    version: String,
    integrity: String,
    resolved: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
struct VendoredFile {
    source_path: String,
    vendored_path: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratorInfo {
    directory: String,
    script: String,
    package_json_sha256: String,
    package_lock_sha256: String,
    script_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
struct HostileEntry {
    fixture: String,
    sha256: String,
    purpose: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    note: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sidecar {
    schema_version: String,
    fixture_name: String,
    provenance: SidecarProvenance,
    content_sha256: String,
    byte_count: usize,
    generated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SidecarProvenance {
    generator: String,
    external_deployment: bool,
    sdk_version: String,
    exporter_version: String,
}

#[derive(Debug, Deserialize)]
struct OtlpTrace {
    #[serde(rename = "resourceSpans")]
    resource_spans: Vec<ResourceSpan>,
}

#[derive(Debug, Deserialize)]
struct ResourceSpan {
    #[serde(rename = "scopeSpans")]
    scope_spans: Vec<ScopeSpan>,
}

#[derive(Debug, Deserialize)]
struct ScopeSpan {
    spans: Vec<Span>,
}

#[derive(Debug, Deserialize)]
struct Span {
    name: String,
    kind: u32,
    attributes: Vec<Attribute>,
}

#[derive(Debug, Deserialize)]
struct Attribute {
    key: String,
    value: AttributeValue,
}

#[derive(Debug, Deserialize)]
struct AttributeValue {
    #[serde(rename = "stringValue")]
    string_value: Option<String>,
}

fn sha256_file(path: &Path) -> Result<String, ValidationError> {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(path).map_err(|_| ValidationError::VendoredFileMissing)?;
    let hash = Sha256::digest(&bytes);
    Ok(hex::encode(hash))
}

/// Validate that a path is safe and relative (no traversal, absolute paths, etc.)
fn validate_safe_relative_path(path_str: &str) -> Result<(), ValidationError> {
    let path = Path::new(path_str);

    for component in path.components() {
        match component {
            Component::Normal(_) => continue,
            Component::RootDir
            | Component::Prefix(_)
            | Component::ParentDir
            | Component::CurDir => {
                return Err(ValidationError::PathTraversal);
            }
        }
    }

    // Additional check: reject empty paths
    if path_str.is_empty() {
        return Err(ValidationError::PathTraversal);
    }

    Ok(())
}

/// Public entry point for validating the corpus at a given root path.
/// Used by mutation tests to verify that tampering is detected.
pub fn validate_corpus_at_path(root: &Path) -> Result<(), ValidationError> {
    let lock_path = root.join("upstream.lock.json");
    let content = fs::read_to_string(&lock_path).map_err(|_| ValidationError::LockFileMissing)?;

    // Test for duplicate fields by attempting to parse as raw JSON first
    let _raw: serde_json::Value =
        serde_json::from_str(&content).map_err(|_| ValidationError::LockParseError)?;

    let lock: UpstreamLock =
        serde_json::from_str(&content).map_err(|_| ValidationError::LockParseError)?;

    // Validate schema version
    if lock.schema_version != "1" {
        return Err(ValidationError::SchemaVersionInvalid);
    }

    // Validate locked_at is RFC3339
    if chrono::DateTime::parse_from_rfc3339(&lock.locked_at).is_err() {
        return Err(ValidationError::SchemaVersionInvalid);
    }

    // Validate provenance marker
    if !lock.provenance.note.contains("Locally generated")
        || !lock.provenance.note.contains("Not external deployment")
    {
        return Err(ValidationError::ProvenanceMarkerInvalid);
    }

    // Helper to build path from temp root
    let build_path = |rel: &str| -> Result<PathBuf, ValidationError> {
        validate_safe_relative_path(rel)?;
        Ok(root.join(rel))
    };

    // Validate upstream sources for duplicates and correct structure
    let mut seen_sources = HashSet::new();
    for source in &lock.upstream_sources {
        // Validate source type
        if source.source_type != "proto" && source.source_type != "semconv" {
            return Err(ValidationError::UpstreamSourceTypeInvalid);
        }

        // Validate repository URL
        if !source
            .repository
            .starts_with("https://github.com/open-telemetry/")
        {
            return Err(ValidationError::UpstreamSourceRepositoryInvalid);
        }

        // Validate exactly one of tag or commit
        match (&source.tag, &source.commit) {
            (Some(_), None) | (None, Some(_)) => {}
            _ => return Err(ValidationError::UpstreamSourceCardinalityInvalid),
        }

        let source_key = (&source.repository, &source.tag, &source.commit);
        if !seen_sources.insert(source_key) {
            return Err(ValidationError::UpstreamSourceDuplicate);
        }

        // Validate no duplicate files within source
        let mut seen_files = HashSet::new();
        for file in &source.files {
            validate_safe_relative_path(&file.source_path)?;
            validate_safe_relative_path(&file.vendored_path)?;

            if !seen_files.insert(&file.vendored_path) {
                return Err(ValidationError::VendoredDuplicateFile);
            }

            let path = build_path(&file.vendored_path)?;
            if !path.exists() {
                return Err(ValidationError::VendoredFileMissing);
            }
            let actual_hash = sha256_file(&path)?;
            if actual_hash != file.sha256 {
                return Err(ValidationError::VendoredHashMismatch);
            }
        }
    }

    // Validate generator
    validate_safe_relative_path(&lock.generator.directory)?;
    validate_safe_relative_path(&lock.generator.script)?;

    let gen_root = build_path(&lock.generator.directory)?;
    let pkg_path = gen_root.join("package.json");
    if !pkg_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    if sha256_file(&pkg_path)? != lock.generator.package_json_sha256 {
        return Err(ValidationError::GeneratorHashMismatch);
    }

    let pkg_lock_path = gen_root.join("package-lock.json");
    if !pkg_lock_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    if sha256_file(&pkg_lock_path)? != lock.generator.package_lock_sha256 {
        return Err(ValidationError::GeneratorHashMismatch);
    }

    let script_path = gen_root.join(&lock.generator.script);
    if !script_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    if sha256_file(&script_path)? != lock.generator.script_sha256 {
        return Err(ValidationError::GeneratorHashMismatch);
    }

    // Validate package-lock bindings
    let pkg_lock_content =
        fs::read_to_string(&pkg_lock_path).map_err(|_| ValidationError::GeneratorFileMissing)?;
    let pkg_lock_json: serde_json::Value = serde_json::from_str(&pkg_lock_content)
        .map_err(|_| ValidationError::GeneratorHashMismatch)?;

    if let Some(packages) = pkg_lock_json.get("packages").and_then(|p| p.as_object()) {
        let sdk_key = format!("node_modules/{}", lock.sdk.package);
        if let Some(sdk_pkg) = packages.get(&sdk_key) {
            if sdk_pkg.get("version").and_then(|v| v.as_str()) != Some(&lock.sdk.version)
                || sdk_pkg.get("resolved").and_then(|v| v.as_str()) != Some(&lock.sdk.resolved)
                || sdk_pkg.get("integrity").and_then(|v| v.as_str()) != Some(&lock.sdk.integrity)
            {
                return Err(ValidationError::PackageLockMismatch);
            }
        } else {
            return Err(ValidationError::PackageLockMismatch);
        }

        let exporter_key = format!("node_modules/{}", lock.exporter.package);
        if let Some(exp_pkg) = packages.get(&exporter_key) {
            if exp_pkg.get("version").and_then(|v| v.as_str()) != Some(&lock.exporter.version)
                || exp_pkg.get("resolved").and_then(|v| v.as_str()) != Some(&lock.exporter.resolved)
                || exp_pkg.get("integrity").and_then(|v| v.as_str())
                    != Some(&lock.exporter.integrity)
            {
                return Err(ValidationError::PackageLockMismatch);
            }
        } else {
            return Err(ValidationError::PackageLockMismatch);
        }
    } else {
        return Err(ValidationError::PackageLockMismatch);
    }

    // Validate corpus - check for duplicates first
    let mut seen_fixtures = HashSet::new();
    let mut seen_sidecars = HashSet::new();
    for entry in &lock.corpus {
        if !seen_fixtures.insert(&entry.fixture) {
            return Err(ValidationError::FixtureDuplicatePath);
        }
        if !seen_sidecars.insert(&entry.sidecar) {
            return Err(ValidationError::FixtureDuplicatePath);
        }
    }

    for entry in &lock.corpus {
        validate_safe_relative_path(&entry.fixture)?;
        validate_safe_relative_path(&entry.sidecar)?;

        let fixture_path = build_path(&entry.fixture)?;
        if !fixture_path.exists() {
            return Err(ValidationError::FixtureMissing);
        }
        if sha256_file(&fixture_path)? != entry.content_sha256 {
            return Err(ValidationError::FixtureHashMismatch);
        }

        let sidecar_path = build_path(&entry.sidecar)?;
        if !sidecar_path.exists() {
            return Err(ValidationError::SidecarMissing);
        }

        // Validate sidecar hash FIRST before parsing content
        if sha256_file(&sidecar_path)? != entry.sidecar_sha256 {
            return Err(ValidationError::SidecarHashMismatch);
        }

        let sidecar_content =
            fs::read_to_string(&sidecar_path).map_err(|_| ValidationError::SidecarMissing)?;
        let sidecar: Sidecar = serde_json::from_str(&sidecar_content)
            .map_err(|_| ValidationError::SidecarParseError)?;

        // Validate sidecar schema_version
        if sidecar.schema_version != "1" {
            return Err(ValidationError::SidecarSemanticMismatch);
        }

        // Validate sidecar fixture_name matches entry
        let expected_name = entry.fixture.trim_end_matches(".json");
        if sidecar.fixture_name != expected_name {
            return Err(ValidationError::SidecarSemanticMismatch);
        }

        // Validate sidecar generator
        if sidecar.provenance.generator != "locally_generated_official_sdk" {
            return Err(ValidationError::SidecarSemanticMismatch);
        }

        // Validate sidecar generated_at is RFC3339
        if chrono::DateTime::parse_from_rfc3339(&sidecar.generated_at).is_err() {
            return Err(ValidationError::SidecarSemanticMismatch);
        }

        if sidecar.content_sha256 != entry.content_sha256 {
            return Err(ValidationError::SidecarHashMismatch);
        }

        let actual_bytes = fs::metadata(&fixture_path)
            .map_err(|_| ValidationError::FixtureMissing)?
            .len() as usize;
        if actual_bytes != entry.byte_count || sidecar.byte_count != entry.byte_count {
            return Err(ValidationError::FixtureHashMismatch);
        }

        if sidecar.provenance.external_deployment {
            return Err(ValidationError::ExternalDeploymentTrue);
        }
        if sidecar.provenance.sdk_version != lock.sdk.version
            || sidecar.provenance.exporter_version != lock.exporter.version
        {
            return Err(ValidationError::SidecarHashMismatch);
        }

        // Validate benign fixture content semantics (test-only, small parser)
        let fixture_content =
            fs::read_to_string(&fixture_path).map_err(|_| ValidationError::FixtureMissing)?;
        let trace: OtlpTrace = serde_json::from_str(&fixture_content)
            .map_err(|_| ValidationError::FixtureSemanticMismatch)?;

        // Expect exactly one span
        let mut span_count = 0;
        let mut found_span: Option<&Span> = None;
        for rs in &trace.resource_spans {
            for ss in &rs.scope_spans {
                for span in &ss.spans {
                    span_count += 1;
                    found_span = Some(span);
                }
            }
        }

        if span_count != 1 {
            return Err(ValidationError::FixtureSpanCountMismatch);
        }

        let span = found_span.unwrap();

        // Validate span kind matches lock entry (CLIENT=3, SERVER=2)
        let expected_kind = match entry.span_kind.as_str() {
            "CLIENT" => 3,
            "SERVER" => 2,
            _ => return Err(ValidationError::FixtureInvalidSpanKind),
        };
        if span.kind != expected_kind {
            return Err(ValidationError::FixtureSemanticMismatch);
        }

        // Validate span name contains "tools/call"
        if !span.name.contains("tools/call") || !span.name.contains("read_file") {
            return Err(ValidationError::FixtureSemanticMismatch);
        }

        // Validate required attributes
        let mut seen_attr_keys = HashSet::new();
        let mut found_mcp_method = None;
        let required_attrs = [
            "mcp.method.name",
            "gen_ai.operation.name",
            "gen_ai.tool.name",
            "jsonrpc.request.id",
            "mcp.protocol.version",
        ];

        for attr in &span.attributes {
            // Check for duplicates
            if !seen_attr_keys.insert(attr.key.clone()) {
                return Err(ValidationError::FixtureDuplicateAttribute);
            }

            // Reject legacy mcp.tool.* keys
            if attr.key.starts_with("mcp.tool.") {
                return Err(ValidationError::FixtureSemanticMismatch);
            }

            // Reject sensitive attributes
            if attr.key == "gen_ai.tool.call.arguments" || attr.key == "gen_ai.tool.call.result" {
                return Err(ValidationError::FixtureSemanticMismatch);
            }

            if attr.key == "mcp.method.name" {
                found_mcp_method = attr.value.string_value.as_deref();
            }
        }

        for req in &required_attrs {
            if !seen_attr_keys.contains(*req) {
                return Err(ValidationError::FixtureMissingRequiredAttribute);
            }
        }

        // Validate mcp_method derived from actual attribute
        if found_mcp_method != Some(&entry.mcp_method) {
            return Err(ValidationError::FixtureSemanticMismatch);
        }
    }

    // Validate hostile fixtures - check for duplicates
    let mut seen_hostile = HashSet::new();
    for entry in &lock.hostile_fixtures {
        if !seen_hostile.insert(&entry.fixture) {
            return Err(ValidationError::HostileDuplicatePath);
        }
    }

    for entry in &lock.hostile_fixtures {
        validate_safe_relative_path(&entry.fixture)?;

        // Validate purpose is non-empty
        if entry.purpose.is_empty() {
            return Err(ValidationError::HostileHashMismatch);
        }

        let path = build_path(&entry.fixture)?;
        if !path.exists() {
            return Err(ValidationError::HostileMissing);
        }
        if sha256_file(&path)? != entry.sha256 {
            return Err(ValidationError::HostileHashMismatch);
        }
    }

    // Validate no unlisted files
    let mut expected: HashSet<String> = HashSet::new();
    expected.insert("upstream.lock.json".to_string());
    expected.insert("README.md".to_string());
    for entry in &lock.corpus {
        expected.insert(entry.fixture.clone());
        expected.insert(entry.sidecar.clone());
    }
    for entry in &lock.hostile_fixtures {
        expected.insert(entry.fixture.clone());
    }

    for entry in fs::read_dir(root).map_err(|_| ValidationError::LockFileMissing)? {
        let entry = entry.map_err(|_| ValidationError::UnlistedFileInCorpus)?;
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".json") && !expected.contains(name) {
                    return Err(ValidationError::UnlistedFileInCorpus);
                }
            }
        }
    }

    Ok(())
}
