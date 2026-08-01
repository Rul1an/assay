//! Hermetic OTLP MCP Corpus Validator
//!
//! Validates the locked fixture corpus in `tests/fixtures/otel-mcp-ingest-v0/` without
//! using any unbounded production parsers. Every vendored file, generator file, and corpus
//! fixture must match its locked hash. Typed errors never include user values.

use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

// -- Frozen Slice A contract constants --------------------------------------------------------

/// Exact upstream source set (repo URL, type, tag-or-commit).
const EXPECTED_SOURCES: &[(&str, &str, Option<&str>, Option<&str>)] = &[
    (
        "https://github.com/open-telemetry/opentelemetry-proto",
        "proto",
        Some("v1.11.0"),
        None,
    ),
    (
        "https://github.com/open-telemetry/semantic-conventions-genai",
        "semconv",
        None,
        Some("434c91dcc34ed038e3048c07720ddfed2c6bddfc"),
    ),
];

/// Exact proto source_path -> vendored_path bindings.
const EXPECTED_PROTO_PAIRS: &[(&str, &str)] = &[
    (
        "opentelemetry/proto/collector/trace/v1/trace_service.proto",
        "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/collector/trace/v1/trace_service.proto",
    ),
    (
        "opentelemetry/proto/trace/v1/trace.proto",
        "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/trace/v1/trace.proto",
    ),
    (
        "opentelemetry/proto/resource/v1/resource.proto",
        "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/resource/v1/resource.proto",
    ),
    (
        "opentelemetry/proto/common/v1/common.proto",
        "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/common/v1/common.proto",
    ),
];

/// Exact semconv source_path -> vendored_path binding.
const EXPECTED_SEMCONV_PAIR: (&str, &str) = (
    "docs/gen-ai/mcp.md",
    "vendor/semantic-conventions-genai-434c91dc/mcp.md",
);

/// Exact SDK identity (package, version, resolved URL, integrity).
const EXPECTED_SDK_PACKAGE: &str = "@opentelemetry/sdk-trace-node";
const EXPECTED_SDK_VERSION: &str = "2.10.0";
const EXPECTED_SDK_RESOLVED: &str =
    "https://registry.npmjs.org/@opentelemetry/sdk-trace-node/-/sdk-trace-node-2.10.0.tgz";
const EXPECTED_SDK_INTEGRITY: &str =
    "sha512-GZK/G6oZyBLGlH1pUgeDch7D91KoHd2uotUGIkWCPi9GI5T9X0p4L7nNAMDR1BQjkRYoDqo+ddfVx9t5Uhys+Q==";

/// Exact exporter identity (package, version, resolved URL, integrity).
const EXPECTED_EXPORTER_PACKAGE: &str = "@opentelemetry/exporter-trace-otlp-http";
const EXPECTED_EXPORTER_VERSION: &str = "0.221.0";
const EXPECTED_EXPORTER_RESOLVED: &str = "https://registry.npmjs.org/@opentelemetry/exporter-trace-otlp-http/-/exporter-trace-otlp-http-0.221.0.tgz";
const EXPECTED_EXPORTER_INTEGRITY: &str =
    "sha512-AySXiKoC+meiWm6zdVj5T2LnPDZuatveBby1cMOeQteIWsYXAUxs8Sru13G2pVSPrUXz6vF+og7QVBX6GdC/oQ==";

/// Exact corpus fixture tuples: (fixture, sidecar, span_kind, mcp_method).
const EXPECTED_CORPUS: &[(&str, &str, &str, &str)] = &[
    (
        "mcp_client_tools_call.json",
        "mcp_client_tools_call.meta.json",
        "CLIENT",
        "tools/call",
    ),
    (
        "mcp_server_tools_call.json",
        "mcp_server_tools_call.meta.json",
        "SERVER",
        "tools/call",
    ),
];

/// Exact hostile fixture names and their purposes.
const EXPECTED_HOSTILE: &[(&str, &str)] = &[
    ("hostile_deep_nesting.json", "test_parser_depth_limits"),
    ("hostile_oversized_attribute.json", "test_size_limits"),
    (
        "hostile_missing_required_fields.json",
        "test_schema_validation",
    ),
];

/// Exact provenance note (no substring acceptance).
const EXPECTED_PROVENANCE_NOTE: &str = "Locally generated test fixtures using official OpenTelemetry SDK and OTLP HTTP exporter. Not external deployment evidence. No production decoder in assay-core.";

/// Exact span name for all benign fixtures.
const EXPECTED_SPAN_NAME: &str = "tools/call read_file";

/// Exact required attribute values for benign fixtures.
const EXPECTED_MCP_METHOD_NAME: &str = "tools/call";
const EXPECTED_GENAI_OPERATION_NAME: &str = "execute_tool";
const EXPECTED_GENAI_TOOL_NAME: &str = "read_file";
const EXPECTED_MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Exact generator identity: directory must be 'generator', script must be 'generate.js',
/// and .node-version must be the governed version file with an exact Node version.
/// npm_version governs the exact npm in the pair; package.json packageManager must match.
const EXPECTED_GENERATOR_DIRECTORY: &str = "generator";
const EXPECTED_GENERATOR_SCRIPT: &str = "generate.js";
const EXPECTED_NODE_VERSION_FILE: &str = ".node-version";
const EXPECTED_NODE_VERSION: &str = "22.16.0";
const EXPECTED_NPM_VERSION: &str = "10.9.2";

/// Independently frozen expected digests for vendored content files.
/// These are NOT read from upstream.lock.json; they are compiled into the
/// validator binary. A coordinated tamper of both vendored files and the
/// lock file's sha256 fields is caught because these constants disagree.
const EXPECTED_VENDORED_DIGESTS: &[(&str, &str)] = &[
    (
        "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/collector/trace/v1/trace_service.proto",
        "03c8cc4e3e101087d884392d6eda32152ad5cd696e6344f50deaa59804a75c7a",
    ),
    (
        "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/trace/v1/trace.proto",
        "677a3db890a63b97bd70ff2be2143a135207e618a4e0be74402e335a5c099c92",
    ),
    (
        "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/resource/v1/resource.proto",
        "e0a7cdc0ffcfeffaa2606e8611839735ebffaa2d6acdf33e9356f2c48ae692d3",
    ),
    (
        "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/common/v1/common.proto",
        "b4430178a6693cd4b5c6d4d3674b060cd92f834f64fc2fc474dc9e2a59d89ff7",
    ),
    (
        "vendor/semantic-conventions-genai-434c91dc/mcp.md",
        "741d3d58000f1c2d678235a27b1c39dc79b01ef3bcc8c4f43218814adc7795de",
    ),
];

/// Exact governed file set (recursive). Every file in the corpus root must be in this set.
/// generator/node_modules is explicitly ignored (created locally by npm ci).
const GOVERNED_FILES: &[&str] = &[
    "upstream.lock.json",
    "README.md",
    "mcp_client_tools_call.json",
    "mcp_client_tools_call.meta.json",
    "mcp_server_tools_call.json",
    "mcp_server_tools_call.meta.json",
    "hostile_deep_nesting.json",
    "hostile_oversized_attribute.json",
    "hostile_missing_required_fields.json",
    "generator/.node-version",
    "generator/check-runtime.cjs",
    "generator/package.json",
    "generator/package-lock.json",
    "generator/generate.js",
    "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/collector/trace/v1/trace_service.proto",
    "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/trace/v1/trace.proto",
    "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/resource/v1/resource.proto",
    "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/common/v1/common.proto",
    "vendor/semantic-conventions-genai-434c91dc/mcp.md",
];

// -- Error types ------------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    LockFileMissing,
    LockParseError,
    LockedAtInvalid,
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
    FixtureAttributeValueMismatch,
    FixtureDuplicateAttribute,
    FixtureInvalidSpanKind,
    FixtureSpanCountMismatch,
    SidecarMissing,
    SidecarHashMismatch,
    SidecarParseError,
    SidecarSemanticMismatch,
    SidecarTimestampMismatch,
    SidecarByteCountMismatch,
    SidecarProvenanceMismatch,
    HostileMissing,
    HostileHashMismatch,
    HostileDuplicatePath,
    HostilePurposeMismatch,
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
    SourceIdentityMismatch,
    CorpusCardinalityMismatch,
    HostileCardinalityMismatch,
    SymlinkInCorpus,
    GovernedFileMissing,
    GeneratorIdentityMismatch,
    DirectoryReadError,
    PackageLockNodeVersionMismatch,
    PackageJsonPackageManagerMismatch,
}

// -- Serde models (test-only, not production) -------------------------------------------------

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
    node_version_file: String,
    node_version: String,
    npm_version: String,
    package_json_sha256: String,
    package_lock_sha256: String,
    script_sha256: String,
    node_version_sha256: String,
    check_runtime_sha256: String,
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

fn sha256_file(path: &Path, missing_err: ValidationError) -> Result<String, ValidationError> {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(path).map_err(|_| missing_err)?;
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

/// Convert a relative `Path` to a canonical POSIX slash-separated string using
/// `Path::components()`. This avoids OS-dependent separator behaviour (Windows
/// backslashes vs POSIX forward slashes) so the result can be compared directly
/// against the slash-separated `GOVERNED_FILES` constants.
///
/// Non-UTF-8 components fail-closed with `PathTraversal`.
///
/// Visible to sibling test modules (e.g. the mutation tests' copy helper) so
/// every slash-governed comparison uses this single component-based mechanism.
pub fn path_to_posix(rel: &Path) -> Result<String, ValidationError> {
    let mut parts: Vec<&str> = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(os) => {
                let s = os.to_str().ok_or(ValidationError::PathTraversal)?;
                parts.push(s);
            }
            // ParentDir / RootDir / Prefix / CurDir are rejected: the caller
            // must have already run containment checks, but fail-closed here too.
            _ => return Err(ValidationError::PathTraversal),
        }
    }
    if parts.is_empty() {
        return Err(ValidationError::PathTraversal);
    }
    Ok(parts.join("/"))
}

/// Look up the compiled-in frozen digest for a vendored path. Absence is
/// fail-closed: every governed vendored file must carry an independent frozen
/// digest, so a missing mapping is a `VendoredHashMismatch`, never a skip.
/// Visible to sibling test modules for direct negative coverage.
pub fn frozen_vendored_digest(vendored_path: &str) -> Result<&'static str, ValidationError> {
    EXPECTED_VENDORED_DIGESTS
        .iter()
        .find(|(p, _)| *p == vendored_path)
        .map(|(_, digest)| *digest)
        .ok_or(ValidationError::VendoredHashMismatch)
}

/// Collect all files recursively under a directory, returning paths relative to root.
/// Rejects symlinks. Ignores generator/node_modules.
fn collect_governed_files(root: &Path) -> Result<HashSet<String>, ValidationError> {
    let mut files = HashSet::new();
    collect_recursive(root, root, &mut files)?;
    Ok(files)
}

fn collect_recursive(
    base: &Path,
    current: &Path,
    files: &mut HashSet<String>,
) -> Result<(), ValidationError> {
    let entries = fs::read_dir(current).map_err(|_| ValidationError::DirectoryReadError)?;
    for entry in entries {
        let entry = entry.map_err(|_| ValidationError::DirectoryReadError)?;
        let path = entry.path();

        // Containment check: verify path stays within the corpus root before
        // any I/O on the entry. strip_prefix succeeds only when `path` is a
        // child of `base`, so no entry outside the governed tree can reach
        // symlink_metadata or subsequent reads.
        let rel = path
            .strip_prefix(base)
            .map_err(|_| ValidationError::PathTraversal)?;

        // Component-correct traversal check: reject ParentDir (..) components.
        // strip_prefix already establishes containment, but this provides
        // defense-in-depth using path components rather than string matching.
        for component in rel.components() {
            if matches!(component, Component::ParentDir) {
                return Err(ValidationError::PathTraversal);
            }
        }

        // Platform-neutral POSIX representation built from path components.
        // On Windows, rel.to_str() would produce backslashes which would not
        // match the slash-separated GOVERNED_FILES constants.
        let rel_posix = path_to_posix(rel)?;

        // Reject symlinks anywhere in the governed tree (after containment is proved)
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| ValidationError::DirectoryReadError)?;
        if metadata.file_type().is_symlink() {
            return Err(ValidationError::SymlinkInCorpus);
        }

        // Ignore generator/node_modules (created locally by npm ci).
        // Compared against the canonical POSIX representation so the
        // exclusion works identically on Windows and POSIX hosts.
        if rel_posix == "generator/node_modules" || rel_posix.starts_with("generator/node_modules/")
        {
            continue;
        }

        if metadata.is_dir() {
            collect_recursive(base, &path, files)?;
        } else {
            files.insert(rel_posix);
        }
    }
    Ok(())
}

/// Public entry point for validating the corpus at a given root path.
/// Used by mutation tests to verify that tampering is detected.
///
/// Validation order:
///   1. Parse lock file (structural)
///   2. Schema version + locked_at (structural)
///   3. Governed file set check (structural - rejects unlisted/symlinked files)
///   4. Duplicate and path safety checks (structural)
///   5. Exact source identity (provenance, SDK, exporter, upstream sources)
///   6. Exact corpus/hostile cardinality and name bindings
///   7. Hash and content validation
///   8. Semantic purpose/role checks (hostile purpose, fixture semantics)
pub fn validate_corpus_at_path(root: &Path) -> Result<(), ValidationError> {
    let lock_path = root.join("upstream.lock.json");
    let content = fs::read_to_string(&lock_path).map_err(|_| ValidationError::LockFileMissing)?;

    let lock: UpstreamLock =
        serde_json::from_str(&content).map_err(|_| ValidationError::LockParseError)?;

    // -- Phase 1: Structural schema checks ----------------------------------------------------

    // Validate schema version
    if lock.schema_version != "1" {
        return Err(ValidationError::SchemaVersionInvalid);
    }

    // Validate locked_at is RFC3339
    if chrono::DateTime::parse_from_rfc3339(&lock.locked_at).is_err() {
        return Err(ValidationError::LockedAtInvalid);
    }

    // -- Phase 2: Governed file set (structural) ----------------------------------------------
    // Every file in the corpus root must be in the governed set. No rogue files anywhere.

    let actual_files = collect_governed_files(root)?;
    let governed_set: HashSet<String> = GOVERNED_FILES.iter().map(|s| s.to_string()).collect();

    for actual in &actual_files {
        if !governed_set.contains(actual.as_str()) {
            return Err(ValidationError::UnlistedFileInCorpus);
        }
    }
    // Check all governed files exist, iterating GOVERNED_FILES in declaration order
    // (deterministic) rather than a HashSet (non-deterministic iteration order).
    for governed in GOVERNED_FILES {
        if !actual_files.contains(*governed) {
            // Classify by file domain for precise typed errors
            if *governed == "README.md" {
                return Err(ValidationError::GovernedFileMissing);
            } else if governed.starts_with("vendor/") {
                return Err(ValidationError::VendoredFileMissing);
            } else if governed.starts_with("generator/") {
                return Err(ValidationError::GeneratorFileMissing);
            } else if *governed == "upstream.lock.json" {
                // Lock file absence is already caught before we reach here
                return Err(ValidationError::LockFileMissing);
            } else if EXPECTED_HOSTILE.iter().any(|(name, _)| *name == *governed) {
                return Err(ValidationError::HostileMissing);
            } else if EXPECTED_CORPUS
                .iter()
                .any(|(fix, _, _, _)| *fix == *governed)
            {
                return Err(ValidationError::FixtureMissing);
            } else if EXPECTED_CORPUS.iter().any(|(_, sc, _, _)| *sc == *governed) {
                return Err(ValidationError::SidecarMissing);
            } else {
                return Err(ValidationError::GovernedFileMissing);
            }
        }
    }

    // -- Phase 3: Structural duplicate and path safety checks ---------------------------------

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
        }
    }

    // Validate generator identity before anything else uses those paths
    if lock.generator.directory != EXPECTED_GENERATOR_DIRECTORY
        || lock.generator.script != EXPECTED_GENERATOR_SCRIPT
        || lock.generator.node_version_file != EXPECTED_NODE_VERSION_FILE
        || lock.generator.node_version != EXPECTED_NODE_VERSION
        || lock.generator.npm_version != EXPECTED_NPM_VERSION
    {
        return Err(ValidationError::GeneratorIdentityMismatch);
    }

    // Path safety on generator paths
    validate_safe_relative_path(&lock.generator.directory)?;
    validate_safe_relative_path(&lock.generator.script)?;

    // Path safety on corpus fixture and sidecar paths
    for entry in &lock.corpus {
        validate_safe_relative_path(&entry.fixture)?;
        validate_safe_relative_path(&entry.sidecar)?;
    }

    // Path safety on hostile fixture paths
    for entry in &lock.hostile_fixtures {
        validate_safe_relative_path(&entry.fixture)?;
    }

    // Validate corpus for duplicate paths
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

    // Validate hostile for duplicate paths
    let mut seen_hostile = HashSet::new();
    for entry in &lock.hostile_fixtures {
        if !seen_hostile.insert(&entry.fixture) {
            return Err(ValidationError::HostileDuplicatePath);
        }
    }

    // -- Phase 4: Exact identity checks -------------------------------------------------------

    // Validate exact provenance note (no substring acceptance)
    if lock.provenance.note != EXPECTED_PROVENANCE_NOTE {
        return Err(ValidationError::ProvenanceMarkerInvalid);
    }

    // Validate exact SDK identity (package, version, resolved, integrity)
    if lock.sdk.package != EXPECTED_SDK_PACKAGE
        || lock.sdk.version != EXPECTED_SDK_VERSION
        || lock.sdk.resolved != EXPECTED_SDK_RESOLVED
        || lock.sdk.integrity != EXPECTED_SDK_INTEGRITY
    {
        return Err(ValidationError::SourceIdentityMismatch);
    }

    // Validate exact exporter identity (package, version, resolved, integrity)
    if lock.exporter.package != EXPECTED_EXPORTER_PACKAGE
        || lock.exporter.version != EXPECTED_EXPORTER_VERSION
        || lock.exporter.resolved != EXPECTED_EXPORTER_RESOLVED
        || lock.exporter.integrity != EXPECTED_EXPORTER_INTEGRITY
    {
        return Err(ValidationError::SourceIdentityMismatch);
    }

    // Validate exact upstream source set cardinality and identity
    if lock.upstream_sources.len() != EXPECTED_SOURCES.len() {
        return Err(ValidationError::SourceIdentityMismatch);
    }

    for (expected_repo, expected_type, expected_tag, expected_commit) in EXPECTED_SOURCES {
        let found = lock.upstream_sources.iter().any(|s| {
            s.repository == *expected_repo
                && s.source_type == *expected_type
                && s.tag.as_deref() == *expected_tag
                && s.commit.as_deref() == *expected_commit
        });
        if !found {
            return Err(ValidationError::SourceIdentityMismatch);
        }
    }

    // Validate exact proto source_path -> vendored_path pairs
    let proto_source = lock
        .upstream_sources
        .iter()
        .find(|s| s.source_type == "proto")
        .ok_or(ValidationError::SourceIdentityMismatch)?;
    if proto_source.files.len() != EXPECTED_PROTO_PAIRS.len() {
        return Err(ValidationError::SourceIdentityMismatch);
    }
    for (expected_src, expected_vendored) in EXPECTED_PROTO_PAIRS {
        if !proto_source
            .files
            .iter()
            .any(|f| f.source_path == *expected_src && f.vendored_path == *expected_vendored)
        {
            return Err(ValidationError::SourceIdentityMismatch);
        }
    }

    // Validate exact semconv source_path -> vendored_path pair
    let semconv_source = lock
        .upstream_sources
        .iter()
        .find(|s| s.source_type == "semconv")
        .ok_or(ValidationError::SourceIdentityMismatch)?;
    if semconv_source.files.len() != 1 {
        return Err(ValidationError::SourceIdentityMismatch);
    }
    if semconv_source.files[0].source_path != EXPECTED_SEMCONV_PAIR.0
        || semconv_source.files[0].vendored_path != EXPECTED_SEMCONV_PAIR.1
    {
        return Err(ValidationError::SourceIdentityMismatch);
    }

    // -- Phase 5: Exact cardinality and name bindings -----------------------------------------

    // Validate exact corpus cardinality and fixture->sidecar->role->method tuples
    if lock.corpus.len() != EXPECTED_CORPUS.len() {
        return Err(ValidationError::CorpusCardinalityMismatch);
    }
    for (expected_fixture, expected_sidecar, expected_kind, expected_method) in EXPECTED_CORPUS {
        let found = lock.corpus.iter().any(|c| {
            c.fixture == *expected_fixture
                && c.sidecar == *expected_sidecar
                && c.span_kind == *expected_kind
                && c.mcp_method == *expected_method
        });
        if !found {
            return Err(ValidationError::CorpusCardinalityMismatch);
        }
    }

    // Validate exact hostile fixture cardinality and names first
    if lock.hostile_fixtures.len() != EXPECTED_HOSTILE.len() {
        return Err(ValidationError::HostileCardinalityMismatch);
    }
    for (expected_fixture, _) in EXPECTED_HOSTILE {
        if !lock
            .hostile_fixtures
            .iter()
            .any(|h| h.fixture == *expected_fixture)
        {
            return Err(ValidationError::HostileCardinalityMismatch);
        }
    }

    // -- Phase 6: Hash and file validation ----------------------------------------------------

    // Helper to build path from temp root
    let build_path = |rel: &str| -> Result<PathBuf, ValidationError> {
        validate_safe_relative_path(rel)?;
        Ok(root.join(rel))
    };

    // Validate vendored file hashes against BOTH lock file and independent frozen digests.
    // The independent check catches coordinated tamper of vendored files + lock hashes.
    for source in &lock.upstream_sources {
        for file in &source.files {
            let path = build_path(&file.vendored_path)?;
            if !path.exists() {
                return Err(ValidationError::VendoredFileMissing);
            }
            let actual_hash = sha256_file(&path, ValidationError::VendoredFileMissing)?;
            if actual_hash != file.sha256 {
                return Err(ValidationError::VendoredHashMismatch);
            }
            // Independent frozen digest check: vendored_path must exist in
            // EXPECTED_VENDORED_DIGESTS and the actual hash must match the
            // compiled-in constant (not derived from the lock file). Absence
            // of a mapping is fail-closed, never a skip.
            let expected_digest = frozen_vendored_digest(&file.vendored_path)?;
            if actual_hash != expected_digest {
                return Err(ValidationError::VendoredHashMismatch);
            }
        }
    }

    // Validate generator (path safety already checked in Phase 3)
    let gen_root = build_path(&lock.generator.directory)?;
    let pkg_path = gen_root.join("package.json");
    if !pkg_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    if sha256_file(&pkg_path, ValidationError::GeneratorFileMissing)?
        != lock.generator.package_json_sha256
    {
        return Err(ValidationError::GeneratorHashMismatch);
    }

    let pkg_lock_path = gen_root.join("package-lock.json");
    if !pkg_lock_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    if sha256_file(&pkg_lock_path, ValidationError::GeneratorFileMissing)?
        != lock.generator.package_lock_sha256
    {
        return Err(ValidationError::GeneratorHashMismatch);
    }

    let script_path = gen_root.join(&lock.generator.script);
    if !script_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    if sha256_file(&script_path, ValidationError::GeneratorFileMissing)?
        != lock.generator.script_sha256
    {
        return Err(ValidationError::GeneratorHashMismatch);
    }

    let node_version_path = gen_root.join(&lock.generator.node_version_file);
    if !node_version_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    if sha256_file(&node_version_path, ValidationError::GeneratorFileMissing)?
        != lock.generator.node_version_sha256
    {
        return Err(ValidationError::GeneratorHashMismatch);
    }
    // Validate .node-version content matches the lock's node_version field
    let nv_content = fs::read_to_string(&node_version_path)
        .map_err(|_| ValidationError::GeneratorFileMissing)?;
    if nv_content.trim() != lock.generator.node_version {
        return Err(ValidationError::GeneratorIdentityMismatch);
    }

    // Validate check-runtime.cjs hash
    let check_runtime_path = gen_root.join("check-runtime.cjs");
    if !check_runtime_path.exists() {
        return Err(ValidationError::GeneratorFileMissing);
    }
    if sha256_file(&check_runtime_path, ValidationError::GeneratorFileMissing)?
        != lock.generator.check_runtime_sha256
    {
        return Err(ValidationError::GeneratorHashMismatch);
    }

    // Validate package.json packageManager field matches governed npm version exactly.
    // npm itself does not reject a mismatched packageManager field, so this is the
    // only enforcement point. Missing or wrong values are rejected (fail-closed).
    let pkg_json_content =
        fs::read_to_string(&pkg_path).map_err(|_| ValidationError::GeneratorFileMissing)?;
    let pkg_json: serde_json::Value = serde_json::from_str(&pkg_json_content)
        .map_err(|_| ValidationError::GeneratorHashMismatch)?;
    {
        let expected_pm = format!("npm@{}", lock.generator.npm_version);
        let actual_pm = pkg_json.get("packageManager").and_then(|v| v.as_str());
        match actual_pm {
            Some(v) if v == expected_pm => {} // exact match
            _ => return Err(ValidationError::PackageJsonPackageManagerMismatch),
        }
    }

    // Validate package-lock bindings (version, resolved, integrity)
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

    // Validate package-lock root packages[""].engines.node matches governed node_version.
    // Total extraction: packages[""] must exist, engines.node must be a string, and it
    // must exactly equal the governed version. Missing or non-string values are rejected
    // (fail-closed) rather than silently accepted.
    {
        let engines_node = pkg_lock_json
            .get("packages")
            .and_then(|p| p.get(""))
            .and_then(|root_pkg| root_pkg.get("engines"))
            .and_then(|e| e.get("node"))
            .and_then(|n| n.as_str());
        match engines_node {
            Some(v) if v == lock.generator.node_version => {} // exact match
            _ => return Err(ValidationError::PackageLockNodeVersionMismatch),
        }
    }

    // Validate corpus fixture hashes and sidecar content (path safety already checked in Phase 3)
    for entry in &lock.corpus {
        let fixture_path = build_path(&entry.fixture)?;
        if !fixture_path.exists() {
            return Err(ValidationError::FixtureMissing);
        }
        if sha256_file(&fixture_path, ValidationError::FixtureMissing)? != entry.content_sha256 {
            return Err(ValidationError::FixtureHashMismatch);
        }

        let sidecar_path = build_path(&entry.sidecar)?;
        if !sidecar_path.exists() {
            return Err(ValidationError::SidecarMissing);
        }

        // Validate sidecar hash FIRST before parsing content
        if sha256_file(&sidecar_path, ValidationError::SidecarMissing)? != entry.sidecar_sha256 {
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

        // Validate sidecar generated_at is RFC3339 and equals lock.locked_at
        if chrono::DateTime::parse_from_rfc3339(&sidecar.generated_at).is_err() {
            return Err(ValidationError::SidecarSemanticMismatch);
        }
        if sidecar.generated_at != lock.locked_at {
            return Err(ValidationError::SidecarTimestampMismatch);
        }

        if sidecar.content_sha256 != entry.content_sha256 {
            return Err(ValidationError::SidecarHashMismatch);
        }

        let actual_bytes = fs::metadata(&fixture_path)
            .map_err(|_| ValidationError::FixtureMissing)?
            .len() as usize;
        if actual_bytes != entry.byte_count || sidecar.byte_count != entry.byte_count {
            return Err(ValidationError::SidecarByteCountMismatch);
        }

        if sidecar.provenance.external_deployment {
            return Err(ValidationError::ExternalDeploymentTrue);
        }
        if sidecar.provenance.sdk_version != lock.sdk.version
            || sidecar.provenance.exporter_version != lock.exporter.version
        {
            return Err(ValidationError::SidecarProvenanceMismatch);
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

        // Validate exact span name (no substring acceptance)
        if span.name != EXPECTED_SPAN_NAME {
            return Err(ValidationError::FixtureSemanticMismatch);
        }

        // Validate required attributes with exact values
        let mut seen_attr_keys = HashSet::new();
        let mut found_mcp_method = None;
        let mut found_genai_operation = None;
        let mut found_genai_tool = None;
        let mut found_jsonrpc_id = None;
        let mut found_protocol_version = None;

        for attr in &span.attributes {
            // Check for duplicates
            if !seen_attr_keys.insert(attr.key.clone()) {
                return Err(ValidationError::FixtureDuplicateAttribute);
            }

            // Reject legacy mcp.tool.* keys
            if attr.key.starts_with("mcp.tool.") {
                return Err(ValidationError::FixtureSemanticMismatch);
            }

            // Reject sensitive attributes: exact key denial for the two Opt-In
            // attributes defined in the pinned semconv (434c91dc). Not broadened
            // to gen_ai.tool.call.* because the semconv only defines these two.
            if attr.key == "gen_ai.tool.call.arguments" || attr.key == "gen_ai.tool.call.result" {
                return Err(ValidationError::FixtureSemanticMismatch);
            }

            match attr.key.as_str() {
                "mcp.method.name" => found_mcp_method = attr.value.string_value.as_deref(),
                "gen_ai.operation.name" => {
                    found_genai_operation = attr.value.string_value.as_deref()
                }
                "gen_ai.tool.name" => found_genai_tool = attr.value.string_value.as_deref(),
                "jsonrpc.request.id" => found_jsonrpc_id = attr.value.string_value.as_deref(),
                "mcp.protocol.version" => {
                    found_protocol_version = attr.value.string_value.as_deref()
                }
                _ => {}
            }
        }

        // Check required attributes are present
        let required_attrs = [
            "mcp.method.name",
            "gen_ai.operation.name",
            "gen_ai.tool.name",
            "jsonrpc.request.id",
            "mcp.protocol.version",
        ];
        for req in &required_attrs {
            if !seen_attr_keys.contains(*req) {
                return Err(ValidationError::FixtureMissingRequiredAttribute);
            }
        }

        // Validate exact attribute values
        if found_mcp_method != Some(EXPECTED_MCP_METHOD_NAME) {
            return Err(ValidationError::FixtureAttributeValueMismatch);
        }
        if found_genai_operation != Some(EXPECTED_GENAI_OPERATION_NAME) {
            return Err(ValidationError::FixtureAttributeValueMismatch);
        }
        if found_genai_tool != Some(EXPECTED_GENAI_TOOL_NAME) {
            return Err(ValidationError::FixtureAttributeValueMismatch);
        }

        // jsonrpc.request.id must be a string (presence of string_value suffices)
        if found_jsonrpc_id.is_none() {
            return Err(ValidationError::FixtureAttributeValueMismatch);
        }

        // mcp.protocol.version must match expected value exactly
        if found_protocol_version != Some(EXPECTED_MCP_PROTOCOL_VERSION) {
            return Err(ValidationError::FixtureAttributeValueMismatch);
        }

        // Validate mcp_method derived from actual attribute
        if found_mcp_method != Some(&entry.mcp_method) {
            return Err(ValidationError::FixtureSemanticMismatch);
        }
    }

    // -- Phase 7: Hostile fixture validation (purpose checked after name/cardinality) ----------

    for entry in &lock.hostile_fixtures {
        // Path safety already checked in Phase 3
        let path = build_path(&entry.fixture)?;
        if !path.exists() {
            return Err(ValidationError::HostileMissing);
        }
        if sha256_file(&path, ValidationError::HostileMissing)? != entry.sha256 {
            return Err(ValidationError::HostileHashMismatch);
        }

        // Purpose check is last - names/cardinality already validated in Phase 5
        let expected_purpose = EXPECTED_HOSTILE
            .iter()
            .find(|(name, _)| *name == entry.fixture)
            .map(|(_, purpose)| *purpose);
        match expected_purpose {
            Some(p) if entry.purpose == p => {}
            _ => return Err(ValidationError::HostilePurposeMismatch),
        }
    }

    Ok(())
}

// -- Unit tests for path_to_posix (platform-neutral, no filesystem access) --------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_path_to_posix_single_component() {
        let p = Path::new("file.txt");
        assert_eq!(path_to_posix(p).unwrap(), "file.txt");
    }

    #[test]
    fn test_path_to_posix_nested_via_join() {
        // PathBuf::join uses the platform separator internally, so on Windows
        // this would produce "generator\.node-version". path_to_posix must
        // normalise to forward slashes via components on every platform.
        let p = PathBuf::from("generator").join(".node-version");
        assert_eq!(path_to_posix(&p).unwrap(), "generator/.node-version");
    }

    #[test]
    fn test_path_to_posix_deep_nesting() {
        let p = PathBuf::from("vendor")
            .join("opentelemetry-proto-v1.11.0")
            .join("opentelemetry")
            .join("proto")
            .join("collector")
            .join("trace")
            .join("v1")
            .join("trace_service.proto");
        assert_eq!(
            path_to_posix(&p).unwrap(),
            "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/collector/trace/v1/trace_service.proto"
        );
    }

    #[test]
    fn test_path_to_posix_node_modules_exclusion_prefix() {
        // Verify that a path built via join produces the exact string that
        // the node_modules exclusion check expects.
        let p = PathBuf::from("generator").join("node_modules");
        let posix = path_to_posix(&p).unwrap();
        assert_eq!(posix, "generator/node_modules");

        let nested = PathBuf::from("generator")
            .join("node_modules")
            .join("some-pkg")
            .join("index.js");
        let nested_posix = path_to_posix(&nested).unwrap();
        assert!(nested_posix.starts_with("generator/node_modules/"));
    }

    #[test]
    fn test_path_to_posix_rejects_parent_dir() {
        let p = Path::new("generator/../etc/passwd");
        assert_eq!(path_to_posix(p), Err(ValidationError::PathTraversal));
    }

    #[test]
    fn test_path_to_posix_rejects_empty() {
        let p = Path::new("");
        assert_eq!(path_to_posix(p), Err(ValidationError::PathTraversal));
    }

    #[test]
    fn test_path_to_posix_rejects_absolute() {
        let p = Path::new("/etc/passwd");
        assert_eq!(path_to_posix(p), Err(ValidationError::PathTraversal));
    }

    #[test]
    fn test_path_to_posix_rejects_curdir() {
        let p = Path::new("./file.txt");
        assert_eq!(path_to_posix(p), Err(ValidationError::PathTraversal));
    }

    #[test]
    fn test_all_governed_files_match_path_to_posix_roundtrip() {
        // Every entry in GOVERNED_FILES, when parsed as a Path and converted
        // back through path_to_posix, must produce the original string.
        // This proves the constant set is consistent with the normalisation.
        for &governed in GOVERNED_FILES {
            let p = Path::new(governed);
            let result = path_to_posix(p).unwrap_or_else(|e| {
                panic!("path_to_posix failed for governed file {governed:?}: {e:?}")
            });
            assert_eq!(
                result, governed,
                "roundtrip mismatch for governed file {governed:?}"
            );
        }
    }

    #[test]
    fn test_frozen_vendored_digest_mapping_is_total() {
        // The independent frozen digest mapping must be exactly the governed
        // vendored path set (proto pairs plus semconv pair), in both
        // directions. A pair added without a frozen digest would silently
        // drop the coordinated-tamper defence; a frozen digest without a
        // governed pair is a stale constant. Both directions must fail here.
        use std::collections::BTreeSet;
        let governed: BTreeSet<&str> = EXPECTED_PROTO_PAIRS
            .iter()
            .map(|(_, vendored)| *vendored)
            .chain(std::iter::once(EXPECTED_SEMCONV_PAIR.1))
            .collect();
        let frozen: BTreeSet<&str> = EXPECTED_VENDORED_DIGESTS.iter().map(|(p, _)| *p).collect();
        assert_eq!(
            governed, frozen,
            "EXPECTED_VENDORED_DIGESTS must map exactly the governed vendored paths"
        );
    }
}
