//! OTLP MCP Corpus Mutation Tests
//!
//! End-to-end tamper testing: every test mutates a temp copy and proves the validator
//! detects the specific error. All tests call validate_corpus_at_path() and assert
//! precise typed errors (no value leakage).

mod support;
use support::otel_validator::{validate_corpus_at_path, ValidationError};

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

const FIXTURE_ROOT: &str = "tests/fixtures/otel-mcp-ingest-v0";

fn copy_corpus_to_temp() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let src = PathBuf::from(FIXTURE_ROOT);
    let dst = tmp.path().join("corpus");

    fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let ty = entry.file_type().unwrap();
            if ty.is_dir() {
                copy_dir_all(&entry.path(), &dst.join(entry.file_name()));
            } else {
                fs::copy(entry.path(), dst.join(entry.file_name())).unwrap();
            }
        }
    }

    copy_dir_all(&src, &dst);
    let corpus_path = dst.clone();
    (tmp, corpus_path)
}

/// Recompute SHA-256 of a file and return the hex-encoded hash.
fn sha256_of(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(path).unwrap();
    hex::encode(Sha256::digest(&bytes))
}

/// Helper: update the lock file's sidecar hash for a given corpus index after
/// modifying the sidecar file. This allows tests to bypass the sidecar hash check
/// and reach deeper semantic validation.
fn update_sidecar_hash_in_lock(corpus: &std::path::Path, corpus_index: usize) {
    let lock_path = corpus.join("upstream.lock.json");
    let sidecar_name = {
        let lock: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
        lock["corpus"][corpus_index]["sidecar"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let sidecar_path = corpus.join(&sidecar_name);
    let new_hash = sha256_of(&sidecar_path);

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["corpus"][corpus_index]["sidecar_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();
}

/// Helper: rewrite a fixture file with new JSON content, then update both the
/// lock file (content_sha256 + byte_count) and the sidecar (content_sha256 +
/// byte_count) to keep hashes internally consistent, so the test reaches
/// semantic validation rather than stopping at hash checks.
fn rewrite_fixture_consistent(
    corpus: &std::path::Path,
    corpus_index: usize,
    new_fixture_content: &[u8],
) {
    let lock_path = corpus.join("upstream.lock.json");
    let lock_val: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    let fixture_name = lock_val["corpus"][corpus_index]["fixture"]
        .as_str()
        .unwrap()
        .to_string();
    let sidecar_name = lock_val["corpus"][corpus_index]["sidecar"]
        .as_str()
        .unwrap()
        .to_string();

    // Write new fixture content
    let fixture_path = corpus.join(&fixture_name);
    fs::write(&fixture_path, new_fixture_content).unwrap();
    let new_fixture_hash = sha256_of(&fixture_path);
    let new_byte_count = new_fixture_content.len();

    // Update sidecar with new hash and byte count
    let sidecar_path = corpus.join(&sidecar_name);
    let mut sidecar: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&sidecar_path).unwrap()).unwrap();
    sidecar["content_sha256"] = serde_json::json!(new_fixture_hash);
    sidecar["byte_count"] = serde_json::json!(new_byte_count);
    fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar).unwrap()).unwrap();
    let new_sidecar_hash = sha256_of(&sidecar_path);

    // Update lock with new hashes and byte count
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["corpus"][corpus_index]["content_sha256"] = serde_json::json!(new_fixture_hash);
    lock["corpus"][corpus_index]["byte_count"] = serde_json::json!(new_byte_count);
    lock["corpus"][corpus_index]["sidecar_sha256"] = serde_json::json!(new_sidecar_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();
}

// -- Baseline ---------------------------------------------------------------------------------

#[test]
fn test_clean_corpus_acceptance() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let result = validate_corpus_at_path(&corpus);
    assert!(result.is_ok(), "Clean corpus must validate");
}

// -- Byte-level tamper tests ------------------------------------------------------------------

#[test]
fn test_fixture_byte_tamper() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let fixture_path = corpus.join("mcp_client_tools_call.json");

    let mut content = fs::read(&fixture_path).unwrap();
    content[10] ^= 0xFF;
    fs::write(&fixture_path, &content).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::FixtureHashMismatch));
}

#[test]
fn test_sidecar_tamper() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let sidecar_path = corpus.join("mcp_client_tools_call.meta.json");

    let mut content = fs::read(&sidecar_path).unwrap();
    content.push(b' ');
    fs::write(&sidecar_path, &content).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SidecarHashMismatch));
}

#[test]
fn test_sidecar_content_hash_mismatch() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let sidecar_path = corpus.join("mcp_client_tools_call.meta.json");

    let sidecar: serde_json::Value =
        serde_json::from_reader(fs::File::open(&sidecar_path).unwrap()).unwrap();
    let mut modified = sidecar.clone();
    modified["content_sha256"] =
        serde_json::json!("0000000000000000000000000000000000000000000000000000000000000000");

    fs::write(&sidecar_path, serde_json::to_vec_pretty(&modified).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Sidecar file hash changed, so SidecarHashMismatch
    assert_eq!(result, Err(ValidationError::SidecarHashMismatch));
}

#[test]
fn test_proto_tamper() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let proto_path =
        corpus.join("vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/trace/v1/trace.proto");

    let mut content = fs::read(&proto_path).unwrap();
    content[0] ^= 0x01;
    fs::write(&proto_path, &content).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::VendoredHashMismatch));
}

#[test]
fn test_semconv_tamper() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let semconv_path = corpus.join("vendor/semantic-conventions-genai-434c91dc/mcp.md");

    let mut content = fs::read(&semconv_path).unwrap();
    content.extend_from_slice(b"\n<!-- tampered -->\n");
    fs::write(&semconv_path, &content).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::VendoredHashMismatch));
}

// -- Generator tamper tests -------------------------------------------------------------------

#[test]
fn test_generator_source_tamper() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let script_path = corpus.join("generator/generate.js");

    let mut content = fs::read_to_string(&script_path).unwrap();
    content.push_str("// tampered\n");
    fs::write(&script_path, content.as_bytes()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorHashMismatch));
}

#[test]
fn test_package_lock_tamper() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("generator/package-lock.json");

    let mut content = fs::read(&lock_path).unwrap();
    content.push(b' ');
    fs::write(&lock_path, &content).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorHashMismatch));
}

// -- Missing file tests -----------------------------------------------------------------------

#[test]
fn test_missing_fixture() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    fs::remove_file(corpus.join("mcp_client_tools_call.json")).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Governed file set catches this as unlisted (file missing from actual set)
    // but actual validator sees FixtureMissing in the hash phase
    assert_eq!(result, Err(ValidationError::FixtureMissing));
}

#[test]
fn test_missing_sidecar() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    fs::remove_file(corpus.join("mcp_server_tools_call.meta.json")).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SidecarMissing));
}

#[test]
fn test_missing_proto_file() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let proto_path = corpus
        .join("vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/common/v1/common.proto");
    fs::remove_file(&proto_path).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::VendoredFileMissing));
}

#[test]
fn test_missing_generator_script() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    fs::remove_file(corpus.join("generator/generate.js")).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Governed file set catches missing governed files as UnlistedFileInCorpus
    // since the actual set no longer matches the governed set (missing file
    // means actual < governed, but the governed check only fails for actual > governed).
    // The specific GeneratorFileMissing is caught in hash phase.
    assert_eq!(result, Err(ValidationError::GeneratorFileMissing));
}

#[test]
fn test_hostile_fixture_missing() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    fs::remove_file(corpus.join("hostile_deep_nesting.json")).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::HostileMissing));
}

// -- Unlisted / duplicate file tests ----------------------------------------------------------

#[test]
fn test_unlisted_file() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    fs::write(corpus.join("rogue_fixture.json"), b"{}").unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::UnlistedFileInCorpus));
}

#[test]
fn test_unlisted_nested_proto() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    // Rogue proto nested deep in vendor directory
    let rogue_dir =
        corpus.join("vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/metrics/v1");
    fs::create_dir_all(&rogue_dir).unwrap();
    fs::write(rogue_dir.join("rogue.proto"), b"syntax = \"proto3\";").unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::UnlistedFileInCorpus));
}

#[test]
fn test_unlisted_generator_source() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    // Rogue JS file in the generator directory
    fs::write(corpus.join("generator/rogue.js"), b"// rogue").unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::UnlistedFileInCorpus));
}

#[test]
fn test_symlink_rejected() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let link_path = corpus.join("symlink.json");
    // Create a symlink to an existing file
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(corpus.join("upstream.lock.json"), &link_path).unwrap();
        let result = validate_corpus_at_path(&corpus);
        assert_eq!(result, Err(ValidationError::SymlinkInCorpus));
    }
}

#[test]
fn test_duplicate_json_field_in_lock() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    // Read raw JSON and manually insert duplicate key
    let content = fs::read_to_string(&lock_path).unwrap();
    // Insert a duplicate "schema_version" field right after the first one
    let modified = content.replace(
        r#""schema_version": "1","#,
        r#""schema_version": "1","schema_version": "duplicate","#,
    );
    fs::write(&lock_path, modified).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // serde_json should reject duplicate keys with an error
    assert_eq!(result, Err(ValidationError::LockParseError));
}

#[test]
fn test_unknown_lock_field() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_reader(fs::File::open(&lock_path).unwrap()).unwrap();
    lock["unknown_field"] = serde_json::json!("rogue");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::LockParseError));
}

#[test]
fn test_duplicate_fixture_path() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_reader(fs::File::open(&lock_path).unwrap()).unwrap();

    // Duplicate the first corpus entry
    if let Some(corpus_array) = lock["corpus"].as_array_mut() {
        let first = corpus_array[0].clone();
        corpus_array.push(first);
    }

    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Structural duplicate check runs before cardinality
    assert_eq!(result, Err(ValidationError::FixtureDuplicatePath));
}

#[test]
fn test_duplicate_hostile_path() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_reader(fs::File::open(&lock_path).unwrap()).unwrap();

    // Duplicate the first hostile entry
    if let Some(hostile_array) = lock["hostile_fixtures"].as_array_mut() {
        let first = hostile_array[0].clone();
        hostile_array.push(first);
    }

    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Structural duplicate check runs before cardinality
    assert_eq!(result, Err(ValidationError::HostileDuplicatePath));
}

#[test]
fn test_duplicate_vendored_file() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_reader(fs::File::open(&lock_path).unwrap()).unwrap();

    // Duplicate a file entry within the first upstream source
    if let Some(sources) = lock["upstream_sources"].as_array_mut() {
        if let Some(files) = sources[0]["files"].as_array_mut() {
            let first = files[0].clone();
            files.push(first);
        }
    }

    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Structural duplicate check runs before identity
    assert_eq!(result, Err(ValidationError::VendoredDuplicateFile));
}

// -- Path safety tests ------------------------------------------------------------------------

#[test]
fn test_absolute_path_in_fixture() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_reader(fs::File::open(&lock_path).unwrap()).unwrap();

    lock["corpus"][0]["fixture"] = serde_json::json!("/etc/passwd");

    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Structural duplicate check sees the changed name breaks the fixture set,
    // and cardinality fires because the expected tuple is missing
    assert_eq!(result, Err(ValidationError::CorpusCardinalityMismatch));
}

#[test]
fn test_path_traversal_in_vendored() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_reader(fs::File::open(&lock_path).unwrap()).unwrap();

    lock["upstream_sources"][0]["files"][0]["vendored_path"] = serde_json::json!("../escape.proto");

    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::PathTraversal));
}

// -- Semantic sidecar tests (with hash update) ------------------------------------------------

#[test]
fn test_external_deployment_true_with_updated_hash() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let sidecar_path = corpus.join("mcp_client_tools_call.meta.json");

    // Modify sidecar
    let mut sidecar: serde_json::Value =
        serde_json::from_reader(fs::File::open(&sidecar_path).unwrap()).unwrap();
    sidecar["provenance"]["external_deployment"] = serde_json::json!(true);
    fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar).unwrap()).unwrap();

    // Recompute sidecar hash in lock
    update_sidecar_hash_in_lock(&corpus, 0);

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::ExternalDeploymentTrue));
}

#[test]
fn test_fixture_semantic_label_mismatch_with_updated_hash() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let sidecar_path = corpus.join("mcp_client_tools_call.meta.json");

    // Modify sidecar to have wrong fixture_name
    let mut sidecar: serde_json::Value =
        serde_json::from_reader(fs::File::open(&sidecar_path).unwrap()).unwrap();
    sidecar["fixture_name"] = serde_json::json!("wrong_name");
    fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar).unwrap()).unwrap();

    // Recompute sidecar hash in lock
    update_sidecar_hash_in_lock(&corpus, 0);

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SidecarSemanticMismatch));
}

#[test]
fn test_sidecar_generated_at_mismatch_with_lock() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let sidecar_path = corpus.join("mcp_client_tools_call.meta.json");

    // Modify sidecar to have different generated_at (still valid RFC3339)
    let mut sidecar: serde_json::Value =
        serde_json::from_reader(fs::File::open(&sidecar_path).unwrap()).unwrap();
    sidecar["generated_at"] = serde_json::json!("2025-01-01T00:00:00Z");
    fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar).unwrap()).unwrap();

    // Recompute sidecar hash in lock
    update_sidecar_hash_in_lock(&corpus, 0);

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SidecarTimestampMismatch));
}

#[test]
fn test_lock_locked_at_changed_from_sidecar() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    // Change lock's locked_at to a different valid RFC3339 value
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["locked_at"] = serde_json::json!("2025-06-15T12:00:00Z");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SidecarTimestampMismatch));
}

// -- Frozen contract identity mutation tests --------------------------------------------------

#[test]
fn test_proto_tag_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["upstream_sources"][0]["tag"] = serde_json::json!("v1.12.0");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_semconv_commit_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["upstream_sources"][1]["commit"] =
        serde_json::json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_third_upstream_source_added() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();

    // Add a valid-looking third upstream source
    let new_source = serde_json::json!({
        "type": "proto",
        "repository": "https://github.com/open-telemetry/opentelemetry-proto-go",
        "tag": "v1.0.0",
        "commit": null,
        "files": []
    });
    lock["upstream_sources"]
        .as_array_mut()
        .unwrap()
        .push(new_source);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_sdk_package_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["sdk"]["package"] = serde_json::json!("@opentelemetry/sdk-trace-web");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_sdk_version_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["sdk"]["version"] = serde_json::json!("3.0.0");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_exporter_version_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["exporter"]["version"] = serde_json::json!("1.0.0");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_sdk_resolved_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["sdk"]["resolved"] = serde_json::json!("https://evil.example.com/sdk.tgz");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_sdk_integrity_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["sdk"]["integrity"] = serde_json::json!("sha512-TAMPERED==");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_exporter_resolved_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["exporter"]["resolved"] = serde_json::json!("https://evil.example.com/exporter.tgz");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_exporter_integrity_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["exporter"]["integrity"] = serde_json::json!("sha512-TAMPERED==");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

/// Deep consistent mutation: changes resolved+integrity in BOTH upstream.lock.json
/// AND package-lock.json (and updates the lock's package-lock hash). Must still fail
/// SourceIdentityMismatch because the frozen constants bind the exact values.
#[test]
fn test_deep_consistent_sdk_redirect() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_lock_path = corpus.join("generator/package-lock.json");

    let fake_resolved = "https://evil.example.com/sdk-trace-node-2.10.0.tgz";
    let fake_integrity =
        "sha512-EVIL/FakeIntegrityHashThatLooksReal0000000000000000000000000000000==";

    // Mutate package-lock.json to point to the evil URL
    let mut pkg_lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pkg_lock_path).unwrap()).unwrap();
    pkg_lock["packages"]["node_modules/@opentelemetry/sdk-trace-node"]["resolved"] =
        serde_json::json!(fake_resolved);
    pkg_lock["packages"]["node_modules/@opentelemetry/sdk-trace-node"]["integrity"] =
        serde_json::json!(fake_integrity);
    fs::write(
        &pkg_lock_path,
        serde_json::to_vec_pretty(&pkg_lock).unwrap(),
    )
    .unwrap();

    // Recompute package-lock hash
    let new_pkg_lock_hash = sha256_of(&pkg_lock_path);

    // Mutate upstream.lock.json consistently
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["sdk"]["resolved"] = serde_json::json!(fake_resolved);
    lock["sdk"]["integrity"] = serde_json::json!(fake_integrity);
    lock["generator"]["package_lock_sha256"] = serde_json::json!(new_pkg_lock_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Frozen constant binds the exact resolved URL and integrity
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

/// Deep consistent mutation for exporter: same as above but for exporter package.
#[test]
fn test_deep_consistent_exporter_redirect() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_lock_path = corpus.join("generator/package-lock.json");

    let fake_resolved = "https://evil.example.com/exporter-trace-otlp-http-0.221.0.tgz";
    let fake_integrity =
        "sha512-EVIL/FakeExporterIntegrity000000000000000000000000000000000000000==";

    // Mutate package-lock.json
    let mut pkg_lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pkg_lock_path).unwrap()).unwrap();
    pkg_lock["packages"]["node_modules/@opentelemetry/exporter-trace-otlp-http"]["resolved"] =
        serde_json::json!(fake_resolved);
    pkg_lock["packages"]["node_modules/@opentelemetry/exporter-trace-otlp-http"]["integrity"] =
        serde_json::json!(fake_integrity);
    fs::write(
        &pkg_lock_path,
        serde_json::to_vec_pretty(&pkg_lock).unwrap(),
    )
    .unwrap();

    // Recompute package-lock hash
    let new_pkg_lock_hash = sha256_of(&pkg_lock_path);

    // Mutate upstream.lock.json consistently
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["exporter"]["resolved"] = serde_json::json!(fake_resolved);
    lock["exporter"]["integrity"] = serde_json::json!(fake_integrity);
    lock["generator"]["package_lock_sha256"] = serde_json::json!(new_pkg_lock_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_third_corpus_entry_added() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();

    // Add a valid-looking third corpus entry
    let new_entry = serde_json::json!({
        "fixture": "mcp_extra_tools_call.json",
        "sidecar": "mcp_extra_tools_call.meta.json",
        "sidecar_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "content_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "byte_count": 100,
        "span_kind": "CLIENT",
        "mcp_method": "tools/call"
    });
    lock["corpus"].as_array_mut().unwrap().push(new_entry);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::CorpusCardinalityMismatch));
}

#[test]
fn test_hostile_purpose_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Change purpose while keeping the fixture name and hash correct
    lock["hostile_fixtures"][0]["purpose"] = serde_json::json!("test_something_else");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Name/cardinality passes (name still matches), but purpose check fails
    assert_eq!(result, Err(ValidationError::HostilePurposeMismatch));
}

#[test]
fn test_fourth_hostile_added() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();

    let new_hostile = serde_json::json!({
        "fixture": "hostile_unicode_bomb.json",
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "purpose": "test_unicode_normalization"
    });
    lock["hostile_fixtures"]
        .as_array_mut()
        .unwrap()
        .push(new_hostile);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Structural duplicate check passes (unique name), then cardinality fails
    assert_eq!(result, Err(ValidationError::HostileCardinalityMismatch));
}

#[test]
fn test_proto_source_path_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Change one of the proto source paths
    lock["upstream_sources"][0]["files"][0]["source_path"] =
        serde_json::json!("opentelemetry/proto/metrics/v1/metrics.proto");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_proto_vendored_path_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Change the vendored_path of a proto file (source_path stays correct)
    lock["upstream_sources"][0]["files"][0]["vendored_path"] =
        serde_json::json!("vendor/opentelemetry-proto-v1.11.0/different/path/trace_service.proto");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

#[test]
fn test_semconv_vendored_path_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Change semconv vendored_path
    lock["upstream_sources"][1]["files"][0]["vendored_path"] =
        serde_json::json!("vendor/wrong-dir/mcp.md");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SourceIdentityMismatch));
}

// -- Exact provenance note tests --------------------------------------------------------------

#[test]
fn test_provenance_note_with_contradictory_suffix() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Append a contradictory suffix -- old substring check would pass this
    lock["provenance"]["note"] = serde_json::json!(
        "Locally generated test fixtures using official OpenTelemetry SDK and OTLP HTTP exporter. Not external deployment evidence. No production decoder in assay-core. HOWEVER this is actually production data."
    );
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::ProvenanceMarkerInvalid));
}

#[test]
fn test_provenance_note_completely_wrong() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["provenance"]["note"] =
        serde_json::json!("External production evidence from customer deployment.");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::ProvenanceMarkerInvalid));
}

// -- Deep semantic tamper tests ---------------------------------------------------------------
// These tests mutate fixture content semantics, recompute all hashes to keep
// them internally consistent, and verify the validator still catches the
// semantic violation with a typed error.

#[test]
fn test_span_name_tampered_consistent_hashes() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let fixture_path = corpus.join("mcp_client_tools_call.json");

    // Change span name but keep it superficially similar, via JSON parse/modify/serialize
    let content = fs::read_to_string(&fixture_path).unwrap();
    let mut fixture: serde_json::Value = serde_json::from_str(&content).unwrap();
    fixture["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["name"] =
        serde_json::json!("tools/call write_file");
    let modified = serde_json::to_string_pretty(&fixture).unwrap() + "\n";

    rewrite_fixture_consistent(&corpus, 0, modified.as_bytes());

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::FixtureSemanticMismatch));
}

#[test]
fn test_mcp_method_value_tampered_consistent_hashes() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let fixture_path = corpus.join("mcp_client_tools_call.json");

    // Change mcp.method.name value via JSON parse/modify/serialize
    let content = fs::read_to_string(&fixture_path).unwrap();
    let mut fixture: serde_json::Value = serde_json::from_str(&content).unwrap();
    let attrs = fixture["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
        .as_array_mut()
        .unwrap();
    for attr in attrs.iter_mut() {
        if attr["key"].as_str() == Some("mcp.method.name") {
            attr["value"]["stringValue"] = serde_json::json!("tools/list");
        }
    }
    let modified = serde_json::to_string_pretty(&fixture).unwrap() + "\n";

    rewrite_fixture_consistent(&corpus, 0, modified.as_bytes());

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::FixtureAttributeValueMismatch));
}

#[test]
fn test_genai_operation_value_tampered_consistent_hashes() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let fixture_path = corpus.join("mcp_client_tools_call.json");

    // Change gen_ai.operation.name value via JSON parse/modify/serialize
    let content = fs::read_to_string(&fixture_path).unwrap();
    let mut fixture: serde_json::Value = serde_json::from_str(&content).unwrap();
    let attrs = fixture["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
        .as_array_mut()
        .unwrap();
    for attr in attrs.iter_mut() {
        if attr["key"].as_str() == Some("gen_ai.operation.name") {
            attr["value"]["stringValue"] = serde_json::json!("list_tools");
        }
    }
    let modified = serde_json::to_string_pretty(&fixture).unwrap() + "\n";

    rewrite_fixture_consistent(&corpus, 0, modified.as_bytes());

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::FixtureAttributeValueMismatch));
}

#[test]
fn test_genai_tool_name_tampered_consistent_hashes() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let fixture_path = corpus.join("mcp_client_tools_call.json");

    // Change gen_ai.tool.name value via JSON parse/modify/serialize
    let content = fs::read_to_string(&fixture_path).unwrap();
    let mut fixture: serde_json::Value = serde_json::from_str(&content).unwrap();
    let attrs = fixture["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
        .as_array_mut()
        .unwrap();
    for attr in attrs.iter_mut() {
        if attr["key"].as_str() == Some("gen_ai.tool.name") {
            attr["value"]["stringValue"] = serde_json::json!("write_file");
        }
    }
    let modified = serde_json::to_string_pretty(&fixture).unwrap() + "\n";

    rewrite_fixture_consistent(&corpus, 0, modified.as_bytes());

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::FixtureAttributeValueMismatch));
}

#[test]
fn test_protocol_version_non_date_consistent_hashes() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let fixture_path = corpus.join("mcp_client_tools_call.json");

    // Change mcp.protocol.version to a non-date value via JSON parse/modify/serialize
    let content = fs::read_to_string(&fixture_path).unwrap();
    let mut fixture: serde_json::Value = serde_json::from_str(&content).unwrap();
    let attrs = fixture["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
        .as_array_mut()
        .unwrap();
    for attr in attrs.iter_mut() {
        if attr["key"].as_str() == Some("mcp.protocol.version") {
            attr["value"]["stringValue"] = serde_json::json!("v1.0.0");
        }
    }
    let modified = serde_json::to_string_pretty(&fixture).unwrap() + "\n";

    rewrite_fixture_consistent(&corpus, 0, modified.as_bytes());

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::FixtureAttributeValueMismatch));
}

#[test]
fn test_protocol_version_wrong_date_consistent_hashes() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let fixture_path = corpus.join("mcp_client_tools_call.json");

    // Change mcp.protocol.version to a different valid date via JSON parse/modify/serialize
    let content = fs::read_to_string(&fixture_path).unwrap();
    let mut fixture: serde_json::Value = serde_json::from_str(&content).unwrap();
    let attrs = fixture["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
        .as_array_mut()
        .unwrap();
    for attr in attrs.iter_mut() {
        if attr["key"].as_str() == Some("mcp.protocol.version") {
            attr["value"]["stringValue"] = serde_json::json!("2025-01-01");
        }
    }
    let modified = serde_json::to_string_pretty(&fixture).unwrap() + "\n";

    rewrite_fixture_consistent(&corpus, 0, modified.as_bytes());

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::FixtureAttributeValueMismatch));
}

#[test]
fn test_jsonrpc_id_removed_consistent_hashes() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let fixture_path = corpus.join("mcp_client_tools_call.json");

    // Remove the jsonrpc.request.id attribute entirely
    let content = fs::read_to_string(&fixture_path).unwrap();
    let mut fixture: serde_json::Value = serde_json::from_str(&content).unwrap();

    let attrs = fixture["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
        .as_array_mut()
        .unwrap();
    attrs.retain(|a| a["key"].as_str() != Some("jsonrpc.request.id"));

    let modified = serde_json::to_string_pretty(&fixture).unwrap() + "\n";
    rewrite_fixture_consistent(&corpus, 0, modified.as_bytes());

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(
        result,
        Err(ValidationError::FixtureMissingRequiredAttribute)
    );
}

#[test]
fn test_corpus_role_swapped() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Swap CLIENT to SERVER for the first entry
    lock["corpus"][0]["span_kind"] = serde_json::json!("SERVER");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Cardinality check: no CLIENT entry found in expected tuples
    assert_eq!(result, Err(ValidationError::CorpusCardinalityMismatch));
}

#[test]
fn test_corpus_sidecar_name_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Change sidecar name - breaks the expected tuple
    lock["corpus"][0]["sidecar"] = serde_json::json!("wrong_sidecar.meta.json");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::CorpusCardinalityMismatch));
}

#[test]
fn test_corpus_mcp_method_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Change mcp_method - breaks the expected tuple
    lock["corpus"][0]["mcp_method"] = serde_json::json!("tools/list");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::CorpusCardinalityMismatch));
}

#[test]
fn test_hostile_purpose_suffix_contradiction() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Purpose starts with expected value but has contradiction suffix
    lock["hostile_fixtures"][0]["purpose"] =
        serde_json::json!("test_parser_depth_limits but actually benign");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::HostilePurposeMismatch));
}
