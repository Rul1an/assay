//! Hermetic OTLP MCP Corpus Validator
//!
//! Validates the locked fixture corpus in `tests/fixtures/otel-mcp-ingest-v0/` without
//! using any unbounded production parsers. Every vendored file, generator file, and corpus
//! fixture must match its locked hash. Typed errors never include user values.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures/otel-mcp-ingest-v0";

#[derive(Debug, PartialEq, Eq)]
enum ValidationError {
    LockFileMissing,
    LockParseError,
    VendoredFileMissing,
    VendoredHashMismatch,
    GeneratorFileMissing,
    GeneratorHashMismatch,
    FixtureMissing,
    SidecarMissing,
    FixtureHashMismatch,
    SidecarParseError,
    SidecarHashMismatch,
    UnlistedFileInCorpus,
    DuplicateLockField,
    MissingRequiredField,
}

#[derive(Debug, Deserialize)]
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
struct SdkInfo {
    package: String,
    version: String,
    integrity: String,
    resolved: String,
}

#[derive(Debug, Deserialize)]
struct ExporterInfo {
    package: String,
    version: String,
    integrity: String,
    resolved: String,
}

#[derive(Debug, Deserialize)]
struct UpstreamSource {
    #[serde(rename = "type")]
    source_type: String,
    repository: String,
    tag: Option<String>,
    commit: Option<String>,
    files: Vec<VendoredFile>,
}

#[derive(Debug, Deserialize)]
struct VendoredFile {
    source_path: String,
    vendored_path: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct GeneratorInfo {
    directory: String,
    script: String,
    package_json_sha256: String,
    package_lock_sha256: String,
    script_sha256: String,
}

#[derive(Debug, Deserialize)]
struct CorpusEntry {
    fixture: String,
    sidecar: String,
    content_sha256: String,
    byte_count: usize,
    span_kind: String,
    mcp_method: String,
}

#[derive(Debug, Deserialize)]
struct HostileEntry {
    fixture: String,
    purpose: String,
}

#[derive(Debug, Deserialize)]
struct Provenance {
    note: String,
}

#[derive(Debug, Deserialize)]
struct Sidecar {
    schema_version: String,
    fixture_name: String,
    provenance: SidecarProvenance,
    content_sha256: String,
    byte_count: usize,
    generated_at: String,
}

#[derive(Debug, Deserialize)]
struct SidecarProvenance {
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

/// Validate generator files
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
    let lock_path = gen_root.join("package-lock.json");
    if !lock_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    let lock_hash = sha256_file(&lock_path)?;
    if lock_hash != lock.generator.package_lock_sha256 {
        return Err(ValidationError::GeneratorHashMismatch);
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

        let sidecar_content =
            fs::read_to_string(&sidecar_path).map_err(|_| ValidationError::SidecarMissing)?;
        let sidecar: Sidecar = serde_json::from_str(&sidecar_content)
            .map_err(|_| ValidationError::SidecarParseError)?;

        // Verify sidecar hash matches corpus entry
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

        // Verify provenance claims
        if sidecar.provenance.external_deployment {
            return Err(ValidationError::SidecarParseError); // Honest boolean violated
        }
    }
    Ok(())
}

/// Validate hostile fixtures exist and are listed
fn validate_hostile(lock: &UpstreamLock) -> Result<(), ValidationError> {
    for entry in &lock.hostile_fixtures {
        let path = root_path().join(&entry.fixture);
        if !path.exists() {
            return Err(ValidationError::FixtureMissing);
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
    assert_eq!(lock.sdk.version, "1.28.0");
}

#[test]
fn test_exporter_pinned() {
    let lock = load_lock().unwrap();
    assert_eq!(
        lock.exporter.package,
        "@opentelemetry/exporter-trace-otlp-http"
    );
    assert_eq!(lock.exporter.version, "0.56.0");
}

#[test]
fn test_provenance_claims() {
    let lock = load_lock().unwrap();
    assert!(lock.provenance.note.contains("Locally generated"));
    assert!(lock.provenance.note.contains("Not external deployment"));
}
