//! OTLP MCP Corpus Mutation Tests
//!
//! End-to-end tamper testing: every test mutates a temp copy and proves the validator
//! detects the specific error. All tests call validate_corpus_at_path() and assert
//! precise typed errors (no value leakage).

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Import the validator from hermetic test module
#[path = "otel_corpus_hermetic.rs"]
mod hermetic;
use hermetic::{validate_corpus_at_path, ValidationError};

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

#[test]
fn test_clean_corpus_acceptance() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let result = validate_corpus_at_path(&corpus);
    assert!(result.is_ok(), "Clean corpus must validate");
}

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

#[test]
fn test_missing_fixture() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    fs::remove_file(corpus.join("mcp_client_tools_call.json")).unwrap();

    let result = validate_corpus_at_path(&corpus);
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
fn test_unlisted_file() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    fs::write(corpus.join("rogue_fixture.json"), b"{}").unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::UnlistedFileInCorpus));
}

#[test]
fn test_external_deployment_true_rejected() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    let sidecar_path = corpus.join("mcp_client_tools_call.meta.json");

    let mut sidecar: serde_json::Value =
        serde_json::from_reader(fs::File::open(&sidecar_path).unwrap()).unwrap();
    sidecar["provenance"]["external_deployment"] = serde_json::json!(true);
    fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar).unwrap()).unwrap();

    let result = validate_corpus_at_path(&corpus);
    // Sidecar file changed so hash mismatch first
    assert_eq!(result, Err(ValidationError::SidecarHashMismatch));
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
    assert_eq!(result, Err(ValidationError::GeneratorFileMissing));
}

#[test]
fn test_hostile_fixture_missing() {
    let (_tmp, corpus) = copy_corpus_to_temp();
    fs::remove_file(corpus.join("hostile_deep_nesting.json")).unwrap();

    let result = validate_corpus_at_path(&corpus);
    assert_eq!(result, Err(ValidationError::HostileMissing));
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
