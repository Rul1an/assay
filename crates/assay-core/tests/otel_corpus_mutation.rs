//! OTLP MCP Corpus Mutation Tests
//!
//! End-to-end tamper testing: every test mutates a temp copy and proves the validator
//! detects the specific error. All tests call validate_corpus_at_path() and assert
//! precise typed errors (no value leakage).

mod support;
use support::otel_validator::{
    frozen_vendored_digest, path_to_posix, validate_corpus_at_path, ValidationError,
};

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

const FIXTURE_ROOT: &str = "tests/fixtures/otel-mcp-ingest-v0";

/// Recursively copy a directory tree, rejecting symlinks before dereference.
/// Uses symlink_metadata (lstat) to inspect each entry without following symlinks,
/// closing the TOCTOU gap where fs::copy would silently follow a symlink-to-file.
/// Skips generator/node_modules (created locally by npm ci) but still rejects
/// a node_modules symlink before skipping, so a symlink-to-node_modules attack
/// does not silently pass.
/// Panics with a static message if a symlink is found anywhere in the tree.
fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) {
    copy_dir_all_inner(src, src, dst);
}

fn copy_dir_all_inner(root: &std::path::Path, current: &std::path::Path, dst: &std::path::Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let meta =
            fs::symlink_metadata(&src_path).expect("symlink_metadata failed on source entry");
        // Reject symlinks BEFORE any skip decision or dereference
        if meta.file_type().is_symlink() {
            panic!("symlink found in fixture source tree (refusing to copy)");
        }
        // Skip generator/node_modules after proving it is not a symlink.
        // Component-based normalisation via path_to_posix: on Windows,
        // strip_prefix yields backslash-separated relative paths, so a raw
        // to_str() comparison against the slash literal would miss the skip.
        if let Ok(rel) = src_path.strip_prefix(root) {
            if let Ok(rel_posix) = path_to_posix(rel) {
                if rel_posix == "generator/node_modules"
                    || rel_posix.starts_with("generator/node_modules/")
                {
                    continue;
                }
            }
        }
        if meta.is_dir() {
            copy_dir_all_inner(root, &src_path, &dst.join(entry.file_name()));
        } else {
            fs::copy(&src_path, dst.join(entry.file_name())).unwrap();
        }
    }
}

fn copy_corpus_to_temp() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let src = PathBuf::from(FIXTURE_ROOT);
    let dst = tmp.path().join("corpus");

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

/// Locate an upstream_sources entry by its "type" field, returning its array index.
/// Avoids fragile hardcoded array indices that break if lock field order changes.
fn find_source_index(lock: &serde_json::Value, source_type: &str) -> usize {
    lock["upstream_sources"]
        .as_array()
        .expect("upstream_sources must be an array")
        .iter()
        .position(|s| s["type"].as_str() == Some(source_type))
        .unwrap_or_else(|| panic!("upstream_sources has no entry with type={source_type}"))
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
    // Governed file set (Phase 2) catches missing governed files by domain;
    // FixtureMissing is classified because the file matches a corpus fixture name.
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
    // Governed file set (Phase 2) classifies missing governed files by domain;
    // generator/ prefix maps to GeneratorFileMissing.
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

#[cfg(unix)]
#[test]
fn test_symlink_rejected() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let link_path = corpus.join("symlink.json");
    std::os::unix::fs::symlink(corpus.join("upstream.lock.json"), &link_path).unwrap();
    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SymlinkInCorpus));
}

/// Proves the module-level copy_dir_all rejects symlinks in the source tree
/// rather than silently dereferencing them via fs::copy. This test calls the
/// real helper so that a regression in the production guard is caught.
#[cfg(unix)]
#[test]
fn test_copy_rejects_symlink_in_source() {
    // Create a fake source tree with a symlink
    let src_dir = TempDir::new().unwrap();
    let src = src_dir.path();
    fs::write(src.join("real.json"), b"{}").unwrap();
    std::os::unix::fs::symlink(src.join("real.json"), src.join("link.json")).unwrap();

    let dst_dir = TempDir::new().unwrap();
    let dst = dst_dir.path().join("out");

    // Call the real module-level copy_dir_all; it must panic on the symlink.
    let result = std::panic::catch_unwind(|| {
        copy_dir_all(src, &dst);
    });
    assert!(
        result.is_err(),
        "copy_dir_all must panic on symlink in source tree"
    );
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
    // Assert the textual mutation actually occurred (guard against no-op replace)
    assert_ne!(
        content, modified,
        "textual mutation must produce different content"
    );
    assert!(
        modified.contains(r#""schema_version": "duplicate""#),
        "modified content must contain the duplicate key"
    );
    fs::write(&lock_path, modified).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // serde_json with #[serde(deny_unknown_fields)] on a derived struct rejects
    // duplicate struct fields at parse time. This is the correct strict behavior:
    // duplicate keys in the lock file are a parse error, not silently accepted.
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

    // Duplicate a file entry within the proto upstream source (located by type)
    let proto_idx = find_source_index(&lock, "proto");
    if let Some(sources) = lock["upstream_sources"].as_array_mut() {
        if let Some(files) = sources[proto_idx]["files"].as_array_mut() {
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
    // Path safety check in Phase 3 catches absolute paths before cardinality
    assert_eq!(result, Err(ValidationError::PathTraversal));
}

#[test]
fn test_path_traversal_in_vendored() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_reader(fs::File::open(&lock_path).unwrap()).unwrap();

    let proto_idx = find_source_index(&lock, "proto");
    lock["upstream_sources"][proto_idx]["files"][0]["vendored_path"] =
        serde_json::json!("../escape.proto");

    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::PathTraversal));
}

// -- Missing governed file tests (Fix 1: exact set completeness) ------------------------------

#[test]
fn test_readme_missing_governed_completeness() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    fs::remove_file(corpus.join("README.md")).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GovernedFileMissing));
}

// -- Path safety tests (additional: sidecar/hostile traversal) --------------------------------

#[test]
fn test_path_traversal_in_sidecar() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_reader(fs::File::open(&lock_path).unwrap()).unwrap();

    lock["corpus"][0]["sidecar"] = serde_json::json!("../escape.meta.json");

    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::PathTraversal));
}

#[test]
fn test_path_traversal_in_hostile() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_reader(fs::File::open(&lock_path).unwrap()).unwrap();

    lock["hostile_fixtures"][0]["fixture"] = serde_json::json!("../../../etc/passwd");

    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::PathTraversal));
}

// -- Generator identity tests (Fix 3: freeze directory+script name) ---------------------------

#[test]
fn test_generator_directory_repointed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["directory"] = serde_json::json!("scripts");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorIdentityMismatch));
}

#[test]
fn test_generator_script_repointed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Repoint script to package.json - with matching hash this would still pass
    // hash validation, but generator identity must reject it
    lock["generator"]["script"] = serde_json::json!("package.json");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorIdentityMismatch));
}

#[test]
fn test_generator_consistent_repoint_to_package_json() {
    // Consistent mutation: repoint lock.generator.script to package.json and
    // update script_sha256 to match. The generator identity freeze must still
    // catch this because the script name is not 'generate.js'.
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let pkg_json_hash = sha256_of(&corpus.join("generator/package.json"));

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["script"] = serde_json::json!("package.json");
    lock["generator"]["script_sha256"] = serde_json::json!(pkg_json_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorIdentityMismatch));
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
    let proto_idx = find_source_index(&lock, "proto");
    lock["upstream_sources"][proto_idx]["tag"] = serde_json::json!("v1.12.0");
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
    let semconv_idx = find_source_index(&lock, "semconv");
    lock["upstream_sources"][semconv_idx]["commit"] =
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

/// Coordinated vendored+lock tamper: modifies a vendored proto file AND updates
/// the lock's sha256 to match. The independently frozen digests in the validator
/// must still catch this because they are compiled-in constants.
#[test]
fn test_coordinated_vendored_and_lock_tamper() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let proto_path =
        corpus.join("vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/trace/v1/trace.proto");

    // Tamper the vendored file
    let mut content = fs::read(&proto_path).unwrap();
    content.extend_from_slice(b"\n// coordinated tamper\n");
    fs::write(&proto_path, &content).unwrap();
    let tampered_hash = sha256_of(&proto_path);

    // Update the lock file's hash to match the tampered file
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    let proto_idx = find_source_index(&lock, "proto");
    // Find the trace.proto file entry by source_path
    let files = lock["upstream_sources"][proto_idx]["files"]
        .as_array_mut()
        .unwrap();
    for file in files.iter_mut() {
        if file["source_path"].as_str() == Some("opentelemetry/proto/trace/v1/trace.proto") {
            file["sha256"] = serde_json::json!(tampered_hash);
        }
    }
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Independent frozen digest catches the coordinated tamper
    assert_eq!(result, Err(ValidationError::VendoredHashMismatch));
}

/// Proves the independent frozen digest lookup is fail-closed: a vendored path
/// with no entry in the compiled-in EXPECTED_VENDORED_DIGESTS mapping is a
/// typed VendoredHashMismatch, never a silent skip of the independent check.
/// (Phase 4 pins the exact vendored path set, so this branch is defence in
/// depth against future divergence between the pair constants and the frozen
/// digest mapping; the totality of the current mapping is proven separately in
/// the support module's unit tests.)
#[test]
fn test_frozen_vendored_digest_absent_path_fails_closed() {
    assert_eq!(
        frozen_vendored_digest("vendor/unmapped-source/unmapped.proto"),
        Err(ValidationError::VendoredHashMismatch)
    );
    // A governed path still resolves to its compiled-in digest.
    assert!(frozen_vendored_digest(
        "vendor/opentelemetry-proto-v1.11.0/opentelemetry/proto/trace/v1/trace.proto"
    )
    .is_ok());
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
    // Change one of the proto source paths (located by type)
    let proto_idx = find_source_index(&lock, "proto");
    lock["upstream_sources"][proto_idx]["files"][0]["source_path"] =
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
    // Change the vendored_path of a proto file (located by type, source_path stays correct)
    let proto_idx = find_source_index(&lock, "proto");
    lock["upstream_sources"][proto_idx]["files"][0]["vendored_path"] =
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
    // Change semconv vendored_path (located by type)
    let semconv_idx = find_source_index(&lock, "semconv");
    lock["upstream_sources"][semconv_idx]["files"][0]["vendored_path"] =
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
        "Locally generated test fixtures using official OpenTelemetry SDK and OTLP HTTP exporter. Not external deployment evidence. Slice A adds no production decoder for this MCP-shaped OTLP/JSON corpus. HOWEVER this is actually production data."
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

// -- Load-bearing coverage tests for uncovered ValidationError variants -----------------------
// Each test reaches a reachable branch in the frozen contract and proves the
// typed error fires. Unreachable defensive variants are documented inline.

#[test]
fn test_schema_version_invalid() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["schema_version"] = serde_json::json!("2");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SchemaVersionInvalid));
}

#[test]
fn test_locked_at_invalid_format() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["locked_at"] = serde_json::json!("not-a-date");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::LockedAtInvalid));
}

#[test]
fn test_sidecar_parse_error() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let sidecar_path = corpus.join("mcp_client_tools_call.meta.json");

    // Write invalid JSON that passes hash check only because we update the lock
    let broken_content = b"{ not valid json at all\n";
    fs::write(&sidecar_path, broken_content).unwrap();

    // Update lock hashes to match broken sidecar
    let lock_path = corpus.join("upstream.lock.json");
    let new_sidecar_hash = sha256_of(&sidecar_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["corpus"][0]["sidecar_sha256"] = serde_json::json!(new_sidecar_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SidecarParseError));
}

#[test]
fn test_package_lock_mismatch_sdk_version() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_lock_path = corpus.join("generator/package-lock.json");

    // Change SDK version in package-lock.json
    let mut pkg_lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pkg_lock_path).unwrap()).unwrap();
    pkg_lock["packages"]["node_modules/@opentelemetry/sdk-trace-node"]["version"] =
        serde_json::json!("9.99.99");
    fs::write(
        &pkg_lock_path,
        serde_json::to_vec_pretty(&pkg_lock).unwrap(),
    )
    .unwrap();

    // Update lock's package_lock_sha256 to match modified package-lock
    let new_hash = sha256_of(&pkg_lock_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_lock_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::PackageLockMismatch));
}

/// Proves the validator rejects a package-lock.json whose root packages[""].engines.node
/// disagrees with the governed node_version. This is the exact bug where package-lock.json
/// carried the stale EOL version (20.16.0) after upgrading .node-version to 22.16.0.
#[test]
fn test_package_lock_root_engines_node_mismatch() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_lock_path = corpus.join("generator/package-lock.json");

    // Mutate only packages[""].engines.node to a stale version
    let mut pkg_lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pkg_lock_path).unwrap()).unwrap();
    pkg_lock["packages"][""]["engines"]["node"] = serde_json::json!("20.16.0");
    fs::write(
        &pkg_lock_path,
        serde_json::to_vec_pretty(&pkg_lock).unwrap(),
    )
    .unwrap();

    // Update lock's package_lock_sha256 to match modified package-lock (keep hashes consistent)
    let new_hash = sha256_of(&pkg_lock_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_lock_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::PackageLockNodeVersionMismatch));
}

/// Consistent mutation: remove packages[""].engines.node entirely (keep engines object
/// but drop the "node" key). Update package-lock hash so validation reaches the
/// engines.node guard. Must reject — a missing engines.node is not "matches governed
/// version".
#[test]
fn test_package_lock_engines_node_missing() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_lock_path = corpus.join("generator/package-lock.json");

    // Remove packages[""].engines.node but keep the engines object
    let mut pkg_lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pkg_lock_path).unwrap()).unwrap();
    pkg_lock["packages"][""]["engines"]
        .as_object_mut()
        .unwrap()
        .remove("node");
    fs::write(
        &pkg_lock_path,
        serde_json::to_vec_pretty(&pkg_lock).unwrap(),
    )
    .unwrap();

    // Update lock's package_lock_sha256 to match modified package-lock
    let new_hash = sha256_of(&pkg_lock_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_lock_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::PackageLockNodeVersionMismatch));
}

/// Consistent mutation: set packages[""].engines.node to a non-string (integer).
/// Update package-lock hash so validation reaches the engines.node guard. Must reject —
/// a non-string engines.node is not "matches governed version".
#[test]
fn test_package_lock_engines_node_non_string() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_lock_path = corpus.join("generator/package-lock.json");

    // Set packages[""].engines.node to an integer
    let mut pkg_lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pkg_lock_path).unwrap()).unwrap();
    pkg_lock["packages"][""]["engines"]["node"] = serde_json::json!(22);
    fs::write(
        &pkg_lock_path,
        serde_json::to_vec_pretty(&pkg_lock).unwrap(),
    )
    .unwrap();

    // Update lock's package_lock_sha256 to match modified package-lock
    let new_hash = sha256_of(&pkg_lock_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_lock_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::PackageLockNodeVersionMismatch));
}

#[test]
fn test_fixture_duplicate_attribute() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let fixture_path = corpus.join("mcp_client_tools_call.json");

    // Parse fixture and duplicate an attribute
    let content = fs::read_to_string(&fixture_path).unwrap();
    let mut fixture: serde_json::Value = serde_json::from_str(&content).unwrap();
    let attrs = fixture["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
        .as_array_mut()
        .unwrap();
    // Push a duplicate of the first attribute
    let dup = attrs[0].clone();
    attrs.push(dup);
    let modified = serde_json::to_string_pretty(&fixture).unwrap() + "\n";

    rewrite_fixture_consistent(&corpus, 0, modified.as_bytes());

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::FixtureDuplicateAttribute));
}

#[test]
fn test_fixture_span_count_zero() {
    let (_tmp, corpus) = copy_corpus_to_temp();

    // Replace fixture with zero spans
    let zero_spans = serde_json::json!({
        "resourceSpans": [{
            "scopeSpans": [{
                "spans": []
            }]
        }]
    });
    let modified = serde_json::to_string_pretty(&zero_spans).unwrap() + "\n";
    rewrite_fixture_consistent(&corpus, 0, modified.as_bytes());

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::FixtureSpanCountMismatch));
}

#[test]
fn test_upstream_source_type_invalid() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    let proto_idx = find_source_index(&lock, "proto");
    lock["upstream_sources"][proto_idx]["type"] = serde_json::json!("invalid_type");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::UpstreamSourceTypeInvalid));
}

#[test]
fn test_upstream_source_repository_invalid() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    let proto_idx = find_source_index(&lock, "proto");
    lock["upstream_sources"][proto_idx]["repository"] =
        serde_json::json!("https://github.com/evil-org/evil-proto");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(
        result,
        Err(ValidationError::UpstreamSourceRepositoryInvalid)
    );
}

#[test]
fn test_upstream_source_cardinality_both_tag_and_commit() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    let proto_idx = find_source_index(&lock, "proto");
    // Set both tag AND commit (proto normally has only tag)
    lock["upstream_sources"][proto_idx]["commit"] =
        serde_json::json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(
        result,
        Err(ValidationError::UpstreamSourceCardinalityInvalid)
    );
}

#[test]
fn test_upstream_source_duplicate() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    // Duplicate the proto source entry exactly
    let proto_idx = find_source_index(&lock, "proto");
    let dup = lock["upstream_sources"][proto_idx].clone();
    lock["upstream_sources"].as_array_mut().unwrap().push(dup);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::UpstreamSourceDuplicate));
}

#[test]
fn test_sidecar_byte_count_mismatch() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let sidecar_path = corpus.join("mcp_client_tools_call.meta.json");

    // Modify sidecar byte_count to be wrong
    let mut sidecar: serde_json::Value =
        serde_json::from_reader(fs::File::open(&sidecar_path).unwrap()).unwrap();
    sidecar["byte_count"] = serde_json::json!(99999);
    fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar).unwrap()).unwrap();

    // Update sidecar hash in lock to match modified sidecar
    update_sidecar_hash_in_lock(&corpus, 0);

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SidecarByteCountMismatch));
}

#[test]
fn test_sidecar_provenance_sdk_version_mismatch() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let sidecar_path = corpus.join("mcp_client_tools_call.meta.json");

    // Modify sidecar provenance sdk_version
    let mut sidecar: serde_json::Value =
        serde_json::from_reader(fs::File::open(&sidecar_path).unwrap()).unwrap();
    sidecar["provenance"]["sdk_version"] = serde_json::json!("9.99.0");
    fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar).unwrap()).unwrap();

    // Update sidecar hash in lock
    update_sidecar_hash_in_lock(&corpus, 0);

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::SidecarProvenanceMismatch));
}

// Note: FixtureInvalidSpanKind is a defensive variant that fires only if the
// lock file contains a span_kind value other than "CLIENT" or "SERVER". Since
// the frozen contract binds EXPECTED_CORPUS with exactly those two kinds, this
// branch is unreachable through normal lock mutation (CorpusCardinalityMismatch
// fires first). It remains as defense-in-depth against future contract changes.

// -- Exact npm governance mutation tests (packageManager enforcement) --------------------------

/// Proves the validator rejects a package.json whose packageManager field is missing.
/// This is the gap that was previously undetected: npm itself does not reject a
/// mismatched or missing packageManager field, so the validator must enforce it.
#[test]
fn test_package_json_package_manager_missing() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_path = corpus.join("generator/package.json");

    // Remove the packageManager field from package.json
    let mut pkg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pkg_path).unwrap()).unwrap();
    pkg.as_object_mut().unwrap().remove("packageManager");
    fs::write(&pkg_path, serde_json::to_vec_pretty(&pkg).unwrap()).unwrap();

    // Update package_json_sha256 in upstream.lock.json to keep hashes consistent
    let new_hash = sha256_of(&pkg_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_json_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(
        result,
        Err(ValidationError::PackageJsonPackageManagerMismatch)
    );
}

/// Proves the validator rejects a package.json whose packageManager field names a
/// different npm version than the governed one (e.g., npm@9.0.0 instead of npm@10.9.2).
#[test]
fn test_package_json_package_manager_wrong_version() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_path = corpus.join("generator/package.json");

    // Set packageManager to a wrong npm version
    let mut pkg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pkg_path).unwrap()).unwrap();
    pkg["packageManager"] = serde_json::json!("npm@9.0.0");
    fs::write(&pkg_path, serde_json::to_vec_pretty(&pkg).unwrap()).unwrap();

    // Update package_json_sha256 in upstream.lock.json to keep hashes consistent
    let new_hash = sha256_of(&pkg_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_json_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(
        result,
        Err(ValidationError::PackageJsonPackageManagerMismatch)
    );
}

/// Proves the validator rejects a package.json whose packageManager field names
/// a different tool entirely (e.g., yarn instead of npm).
#[test]
fn test_package_json_package_manager_wrong_tool() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_path = corpus.join("generator/package.json");

    // Set packageManager to yarn
    let mut pkg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pkg_path).unwrap()).unwrap();
    pkg["packageManager"] = serde_json::json!("yarn@4.0.0");
    fs::write(&pkg_path, serde_json::to_vec_pretty(&pkg).unwrap()).unwrap();

    // Update package_json_sha256 in upstream.lock.json to keep hashes consistent
    let new_hash = sha256_of(&pkg_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_json_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(
        result,
        Err(ValidationError::PackageJsonPackageManagerMismatch)
    );
}

/// Proves malformed generator/package.json is a typed parse error, not a hash
/// mismatch: the governed hash in the lock is updated to match the malformed
/// bytes, so validation passes the hash check and must fail at parsing with
/// the value-free GeneratorParseError (never a false GeneratorHashMismatch).
#[test]
fn test_package_json_malformed_is_parse_error() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_path = corpus.join("generator/package.json");

    fs::write(&pkg_path, b"{ \"packageManager\": ").unwrap();

    let new_hash = sha256_of(&pkg_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_json_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorParseError));
}

/// Proves malformed generator/package-lock.json is a typed parse error, not a
/// hash mismatch: same construction as above via package_lock_sha256.
#[test]
fn test_package_lock_malformed_is_parse_error() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_lock_path = corpus.join("generator/package-lock.json");

    fs::write(&pkg_lock_path, b"{ \"packages\": [ oops").unwrap();

    let new_hash = sha256_of(&pkg_lock_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_lock_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorParseError));
}

/// Regression pin: hash-before-parse classification for generator/package.json.
/// Malformed bytes WITHOUT a matching governed digest must be typed as the
/// hash claim (GeneratorHashMismatch), not the parse claim: the byte-integrity
/// decision precedes parsing, so an untrusted edit is reported as tamper first.
#[test]
fn test_package_json_malformed_without_hash_update_is_hash_mismatch() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let pkg_path = corpus.join("generator/package.json");

    fs::write(&pkg_path, b"{ \"packageManager\": ").unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorHashMismatch));
}

/// Regression pin: hash-before-parse classification for
/// generator/package-lock.json, same construction as above.
#[test]
fn test_package_lock_malformed_without_hash_update_is_hash_mismatch() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let pkg_lock_path = corpus.join("generator/package-lock.json");

    fs::write(&pkg_lock_path, b"{ \"packages\": [ oops").unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorHashMismatch));
}

/// Proves duplicate object keys in generator/package.json are structurally
/// rejected as GeneratorParseError. The duplicate packageManager carries the
/// governed npm value LAST, so a last-wins Value parser would accept the file
/// silently and full validation would pass; only depth-aware duplicate-key
/// rejection catches it. The governed hash is updated so validation reaches
/// the parse site.
#[test]
fn test_package_json_duplicate_package_manager_is_parse_error() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_path = corpus.join("generator/package.json");

    fs::write(
        &pkg_path,
        b"{\n  \"packageManager\": \"npm@9.9.9\",\n  \"packageManager\": \"npm@10.9.2\"\n}\n",
    )
    .unwrap();

    let new_hash = sha256_of(&pkg_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_json_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorParseError));
}

/// Proves duplicate object keys nested deep inside generator/package-lock.json
/// are structurally rejected as GeneratorParseError. The duplicated critical
/// key is packages[""].engines.node with the governed version LAST, so a
/// last-wins parser would pass the engines.node exact-runtime check and full
/// validation would succeed. The governed hash is updated so validation
/// reaches the parse site.
#[test]
fn test_package_lock_nested_duplicate_key_is_parse_error() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");
    let pkg_lock_path = corpus.join("generator/package-lock.json");

    let content = fs::read_to_string(&pkg_lock_path).unwrap();
    let duplicated = content.replacen(
        "\"node\": \"22.16.0\"",
        "\"node\": \"1.0.0\", \"node\": \"22.16.0\"",
        1,
    );
    assert_ne!(
        content, duplicated,
        "engines.node injection site must exist in package-lock.json"
    );
    fs::write(&pkg_lock_path, duplicated.as_bytes()).unwrap();

    let new_hash = sha256_of(&pkg_lock_path);
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["package_lock_sha256"] = serde_json::json!(new_hash);
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorParseError));
}

/// Proves the validator rejects when the lock file's npm_version does not match
/// the governed constant (catch an attempt to downgrade/upgrade npm in the lock).
#[test]
fn test_lock_npm_version_changed() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let lock_path = corpus.join("upstream.lock.json");

    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["generator"]["npm_version"] = serde_json::json!("11.0.0");
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorIdentityMismatch));
}

/// Proves the validator rejects when check-runtime.cjs is tampered.
#[test]
fn test_check_runtime_script_tamper() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let script_path = corpus.join("generator/check-runtime.cjs");

    let mut content = fs::read_to_string(&script_path).unwrap();
    content.push_str("// tampered\n");
    fs::write(&script_path, content.as_bytes()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::GeneratorHashMismatch));
}

/// Proves check-runtime.cjs itself fails closed on a malformed package.json:
/// nonzero exit with only the controlled "FAIL: cannot read or parse package.json"
/// diagnostic. Node >= 20 embeds a snippet of the malformed source in the
/// JSON.parse SyntaxError message, so an escaping raw error would echo
/// attacker-controlled file content, violating the value-free diagnostic contract.
/// (Runs the script directly rather than validate_corpus_at_path, since the
/// contract under test is the script's own stderr behavior.)
#[test]
fn test_check_runtime_malformed_package_json_no_content_leak() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let generator = corpus.join("generator");
    let marker = "MALFORMED-CONTENT-MARKER";
    fs::write(
        generator.join("package.json"),
        format!("{{ \"{marker}\": oops"),
    )
    .unwrap();

    // Unavailable infrastructure must never become a clean result: a missing
    // Node runtime fails this test rather than silently skipping the proof.
    let output = std::process::Command::new("node")
        .arg("check-runtime.cjs")
        .current_dir(&generator)
        .output()
        .expect("prerequisite missing: node runtime required to run check-runtime.cjs");

    assert!(
        !output.status.success(),
        "malformed package.json must exit nonzero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.is_empty(), "stdout must be empty, got: {stdout}");
    // Exact equality after removing exactly one platform line ending: any extra
    // stderr byte (including extra blank lines) is a leak candidate and must fail.
    let stderr_line = stderr
        .strip_suffix("\r\n")
        .or_else(|| stderr.strip_suffix('\n'))
        .unwrap_or(&stderr);
    assert_eq!(
        stderr_line, "FAIL: cannot read or parse package.json",
        "stderr must be exactly the controlled diagnostic"
    );
    // Redundant with the exact match above; kept to document the attack surface.
    assert!(
        !stderr.contains(marker),
        "must not echo malformed package.json content"
    );
    assert!(
        !stderr.contains("SyntaxError"),
        "must not expose a raw SyntaxError"
    );
}

/// Portable proof that copy_dir_all excludes a real generator/node_modules
/// directory on every platform. The skip comparison is component-based
/// (path_to_posix), so Windows backslash-relative paths cannot dodge the
/// exclusion; a sentinel file inside node_modules must never reach the copy.
#[test]
fn test_copy_excludes_real_node_modules_sentinel() {
    let src_dir = TempDir::new().unwrap();
    let src = src_dir.path();
    fs::create_dir_all(src.join("generator/node_modules/some-pkg")).unwrap();
    fs::write(src.join("generator/package.json"), b"{}").unwrap();
    fs::write(
        src.join("generator/node_modules/sentinel.txt"),
        b"must-not-copy",
    )
    .unwrap();
    fs::write(
        src.join("generator/node_modules/some-pkg/index.js"),
        b"must-not-copy",
    )
    .unwrap();

    let dst_dir = TempDir::new().unwrap();
    let dst = dst_dir.path().join("out");
    copy_dir_all(src, &dst);

    assert!(
        dst.join("generator/package.json").exists(),
        "governed generator file must be copied"
    );
    assert!(
        !dst.join("generator/node_modules").exists(),
        "node_modules must be excluded from the copy"
    );
    assert!(
        !dst.join("generator/node_modules/sentinel.txt").exists(),
        "sentinel inside node_modules must never reach the copy"
    );
}

/// Proves the copy_dir_all helper skips generator/node_modules while still
/// rejecting a node_modules symlink before the skip decision.
#[cfg(unix)]
#[test]
fn test_copy_skips_node_modules_but_rejects_symlink() {
    // Create a source tree with generator/node_modules as a symlink
    let src_dir = TempDir::new().unwrap();
    let src = src_dir.path();
    fs::create_dir_all(src.join("generator")).unwrap();
    fs::write(src.join("generator/package.json"), b"{}").unwrap();
    // Create node_modules as a symlink (attack vector)
    std::os::unix::fs::symlink("/tmp", src.join("generator/node_modules")).unwrap();

    let dst_dir = TempDir::new().unwrap();
    let dst = dst_dir.path().join("out");

    // copy_dir_all must panic on the symlink even though it's named node_modules
    let result = std::panic::catch_unwind(|| {
        copy_dir_all(src, &dst);
    });
    assert!(
        result.is_err(),
        "copy_dir_all must reject node_modules symlink before skip"
    );
}
