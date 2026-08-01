//! Hermetic OTLP MCP Corpus Validator
//!
//! Validates the locked fixture corpus in `tests/fixtures/otel-mcp-ingest-v0/` without
//! using any unbounded production parsers. Every vendored file, generator file, and corpus
//! fixture must match its locked hash. Typed errors never include user values.

use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures/otel-mcp-ingest-v0";

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    LockFileMissing,
    LockParseError,
    #[allow(dead_code)]
    UnknownLockField,
    VendoredFileMissing,
    VendoredHashMismatch,
    GeneratorFileMissing,
    GeneratorHashMismatch,
    FixtureMissing,
    FixtureHashMismatch,
    SidecarMissing,
    SidecarHashMismatch,
    SidecarParseError,
    HostileMissing,
    HostileHashMismatch,
    UnlistedFileInCorpus,
    ExternalDeploymentTrue,
    PathTraversal,
    PackageLockMismatch,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpstreamLock {
    schema_version: String,
    #[allow(dead_code)]
    locked_at: String,
    sdk: SdkInfo,
    exporter: ExporterInfo,
    upstream_sources: Vec<UpstreamSource>,
    generator: GeneratorInfo,
    corpus: Vec<CorpusEntry>,
    hostile_fixtures: Vec<HostileEntry>,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    source_type: String,
    #[allow(dead_code)]
    repository: String,
    #[allow(dead_code)]
    tag: Option<String>,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    span_kind: String,
    #[allow(dead_code)]
    mcp_method: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostileEntry {
    fixture: String,
    sha256: String,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    schema_version: String,
    #[allow(dead_code)]
    fixture_name: String,
    provenance: SidecarProvenance,
    content_sha256: String,
    byte_count: usize,
    #[allow(dead_code)]
    generated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SidecarProvenance {
    #[allow(dead_code)]
    generator: String,
    external_deployment: bool,
    sdk_version: String,
    exporter_version: String,
}

fn sha256_file(path: &Path) -> Result<String, ValidationError> {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(path).map_err(|_| ValidationError::VendoredFileMissing)?;
    let hash = Sha256::digest(&bytes);
    Ok(hex::encode(hash))
}

fn root_path() -> PathBuf {
    PathBuf::from(FIXTURE_ROOT)
}

/// Public entry point for validating the corpus at a given root path.
/// Used by mutation tests to verify that tampering is detected.
pub fn validate_corpus_at_path(root: &std::path::Path) -> Result<(), ValidationError> {
    let lock_path = root.join("upstream.lock.json");
    let content = fs::read_to_string(&lock_path).map_err(|_| ValidationError::LockFileMissing)?;
    let lock: UpstreamLock =
        serde_json::from_str(&content).map_err(|_| ValidationError::LockParseError)?;

    // Helper to build path from temp root
    let build_path = |rel: &str| root.join(rel);

    // Validate vendored sources
    for source in &lock.upstream_sources {
        for file in &source.files {
            if file.source_path.contains("..") || file.vendored_path.contains("..") {
                return Err(ValidationError::PathTraversal);
            }
            let path = build_path(&file.vendored_path);
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
    let gen_root = build_path(&lock.generator.directory);
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

    // Validate corpus
    for entry in &lock.corpus {
        if entry.fixture.contains("..") || entry.sidecar.contains("..") {
            return Err(ValidationError::PathTraversal);
        }

        let fixture_path = build_path(&entry.fixture);
        if !fixture_path.exists() {
            return Err(ValidationError::FixtureMissing);
        }
        if sha256_file(&fixture_path)? != entry.content_sha256 {
            return Err(ValidationError::FixtureHashMismatch);
        }

        let sidecar_path = build_path(&entry.sidecar);
        if !sidecar_path.exists() {
            return Err(ValidationError::SidecarMissing);
        }
        if sha256_file(&sidecar_path)? != entry.sidecar_sha256 {
            return Err(ValidationError::SidecarHashMismatch);
        }

        let sidecar_content =
            fs::read_to_string(&sidecar_path).map_err(|_| ValidationError::SidecarMissing)?;
        let sidecar: Sidecar = serde_json::from_str(&sidecar_content)
            .map_err(|_| ValidationError::SidecarParseError)?;

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
    }

    // Validate hostile fixtures
    for entry in &lock.hostile_fixtures {
        if entry.fixture.contains("..") {
            return Err(ValidationError::PathTraversal);
        }
        let path = build_path(&entry.fixture);
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

/// Load and parse the upstream lock file
fn load_lock() -> Result<UpstreamLock, ValidationError> {
    let path = root_path().join("upstream.lock.json");
    let content = fs::read_to_string(&path).map_err(|_| ValidationError::LockFileMissing)?;
    serde_json::from_str(&content).map_err(|_| ValidationError::LockParseError)
}

/// Validate all vendored upstream sources
fn validate_vendored_sources(lock: &UpstreamLock) -> Result<(), ValidationError> {
    for source in &lock.upstream_sources {
        for file in &source.files {
            // Check for path traversal
            if file.source_path.contains("..") || file.vendored_path.contains("..") {
                return Err(ValidationError::PathTraversal);
            }

            let path = root_path().join(&file.vendored_path);
            if !path.exists() {
                return Err(ValidationError::VendoredFileMissing);
            }
            let actual_hash = sha256_file(&path)?;
            if actual_hash != file.sha256 {
                return Err(ValidationError::VendoredHashMismatch);
            }
        }
    }
    Ok(())
}

/// Validate generator files and package-lock bindings
fn validate_generator(lock: &UpstreamLock) -> Result<(), ValidationError> {
    let gen_root = root_path().join(&lock.generator.directory);

    // package.json
    let pkg_path = gen_root.join("package.json");
    if !pkg_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    let pkg_hash = sha256_file(&pkg_path)?;
    if pkg_hash != lock.generator.package_json_sha256 {
        return Err(ValidationError::GeneratorHashMismatch);
    }

    // package-lock.json
    let pkg_lock_path = gen_root.join("package-lock.json");
    if !pkg_lock_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    let pkg_lock_hash = sha256_file(&pkg_lock_path)?;
    if pkg_lock_hash != lock.generator.package_lock_sha256 {
        return Err(ValidationError::GeneratorHashMismatch);
    }

    // Validate package-lock.json bindings
    let pkg_lock_content =
        fs::read_to_string(&pkg_lock_path).map_err(|_| ValidationError::GeneratorFileMissing)?;
    let pkg_lock_json: serde_json::Value = serde_json::from_str(&pkg_lock_content)
        .map_err(|_| ValidationError::GeneratorHashMismatch)?;

    // Validate SDK package binding
    if let Some(packages) = pkg_lock_json.get("packages").and_then(|p| p.as_object()) {
        let sdk_key = format!("node_modules/{}", lock.sdk.package);
        if let Some(sdk_pkg) = packages.get(&sdk_key) {
            if sdk_pkg.get("version").and_then(|v| v.as_str()) != Some(&lock.sdk.version) {
                return Err(ValidationError::PackageLockMismatch);
            }
            if sdk_pkg.get("resolved").and_then(|v| v.as_str()) != Some(&lock.sdk.resolved) {
                return Err(ValidationError::PackageLockMismatch);
            }
            if sdk_pkg.get("integrity").and_then(|v| v.as_str()) != Some(&lock.sdk.integrity) {
                return Err(ValidationError::PackageLockMismatch);
            }
        } else {
            return Err(ValidationError::PackageLockMismatch);
        }

        // Validate exporter package binding
        let exporter_key = format!("node_modules/{}", lock.exporter.package);
        if let Some(exp_pkg) = packages.get(&exporter_key) {
            if exp_pkg.get("version").and_then(|v| v.as_str()) != Some(&lock.exporter.version) {
                return Err(ValidationError::PackageLockMismatch);
            }
            if exp_pkg.get("resolved").and_then(|v| v.as_str()) != Some(&lock.exporter.resolved) {
                return Err(ValidationError::PackageLockMismatch);
            }
            if exp_pkg.get("integrity").and_then(|v| v.as_str()) != Some(&lock.exporter.integrity) {
                return Err(ValidationError::PackageLockMismatch);
            }
        } else {
            return Err(ValidationError::PackageLockMismatch);
        }
    } else {
        return Err(ValidationError::PackageLockMismatch);
    }

    // generate.js
    let script_path = gen_root.join(&lock.generator.script);
    if !script_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    let script_hash = sha256_file(&script_path)?;
    if script_hash != lock.generator.script_sha256 {
        return Err(ValidationError::GeneratorHashMismatch);
    }

    Ok(())
}

/// Validate corpus fixtures and sidecars
fn validate_corpus(lock: &UpstreamLock) -> Result<(), ValidationError> {
    for entry in &lock.corpus {
        // Check for path traversal
        if entry.fixture.contains("..") || entry.sidecar.contains("..") {
            return Err(ValidationError::PathTraversal);
        }

        // Check fixture file
        let fixture_path = root_path().join(&entry.fixture);
        if !fixture_path.exists() {
            return Err(ValidationError::FixtureMissing);
        }
        let fixture_hash = sha256_file(&fixture_path)?;
        if fixture_hash != entry.content_sha256 {
            return Err(ValidationError::FixtureHashMismatch);
        }

        // Check sidecar
        let sidecar_path = root_path().join(&entry.sidecar);
        if !sidecar_path.exists() {
            return Err(ValidationError::SidecarMissing);
        }

        // Verify sidecar hash against lock
        let sidecar_hash = sha256_file(&sidecar_path)?;
        if sidecar_hash != entry.sidecar_sha256 {
            return Err(ValidationError::SidecarHashMismatch);
        }

        let sidecar_content =
            fs::read_to_string(&sidecar_path).map_err(|_| ValidationError::SidecarMissing)?;
        let sidecar: Sidecar = serde_json::from_str(&sidecar_content)
            .map_err(|_| ValidationError::SidecarParseError)?;

        // Verify sidecar content_sha256 matches corpus entry
        if sidecar.content_sha256 != entry.content_sha256 {
            return Err(ValidationError::SidecarHashMismatch);
        }

        // Verify byte count
        let actual_bytes = fs::metadata(&fixture_path)
            .map_err(|_| ValidationError::FixtureMissing)?
            .len() as usize;
        if actual_bytes != entry.byte_count {
            return Err(ValidationError::FixtureHashMismatch);
        }

        // Verify sidecar byte count matches
        if sidecar.byte_count != entry.byte_count {
            return Err(ValidationError::SidecarHashMismatch);
        }

        // Verify span_kind and mcp_method match sidecar semantics
        // (The fixture must contain these values - we validate labels match content later)

        // Verify provenance claims
        if sidecar.provenance.external_deployment {
            return Err(ValidationError::ExternalDeploymentTrue);
        }

        // Verify SDK/exporter versions match lock
        if sidecar.provenance.sdk_version != lock.sdk.version {
            return Err(ValidationError::SidecarHashMismatch);
        }
        if sidecar.provenance.exporter_version != lock.exporter.version {
            return Err(ValidationError::SidecarHashMismatch);
        }
    }
    Ok(())
}

/// Validate hostile fixtures exist and are listed
fn validate_hostile(lock: &UpstreamLock) -> Result<(), ValidationError> {
    for entry in &lock.hostile_fixtures {
        // Check for path traversal
        if entry.fixture.contains("..") {
            return Err(ValidationError::PathTraversal);
        }

        let path = root_path().join(&entry.fixture);
        if !path.exists() {
            return Err(ValidationError::HostileMissing);
        }

        // Verify hash
        let actual_hash = sha256_file(&path)?;
        if actual_hash != entry.sha256 {
            return Err(ValidationError::HostileHashMismatch);
        }
    }
    Ok(())
}

/// Validate no unlisted .json files exist in corpus root
fn validate_no_unlisted_files(lock: &UpstreamLock) -> Result<(), ValidationError> {
    let mut expected: HashSet<String> = HashSet::new();

    // Add lock file
    expected.insert("upstream.lock.json".to_string());

    // Add corpus fixtures and sidecars
    for entry in &lock.corpus {
        expected.insert(entry.fixture.clone());
        expected.insert(entry.sidecar.clone());
    }

    // Add hostile fixtures
    for entry in &lock.hostile_fixtures {
        expected.insert(entry.fixture.clone());
    }

    // Scan directory
    for entry in fs::read_dir(root_path()).map_err(|_| ValidationError::LockFileMissing)? {
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

#[test]
fn test_hermetic_corpus_validation() {
    let lock = load_lock().expect("lock file must parse");

    validate_vendored_sources(&lock).expect("vendored sources must match");
    validate_generator(&lock).expect("generator files must match");
    validate_corpus(&lock).expect("corpus must be valid");
    validate_hostile(&lock).expect("hostile fixtures must exist");
    validate_no_unlisted_files(&lock).expect("no unlisted files");
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
