//! Integration tests for `assay tool` signing commands.

use std::process::Command;
use tempfile::TempDir;

fn assay_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_assay"))
}

#[test]
fn test_keygen_creates_keypair() {
    let tmp = TempDir::new().unwrap();
    let out_dir = tmp.path();

    let output = assay_cmd()
        .args(["tool", "keygen", "--out"])
        .arg(out_dir)
        .output()
        .expect("failed to run assay tool keygen");

    assert!(output.status.success(), "keygen should succeed");

    // Check files exist
    assert!(out_dir.join("private_key.pem").exists());
    assert!(out_dir.join("public_key.pem").exists());

    // Check output contains key_id
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("key_id: sha256:"), "should print key_id");

    // Check private key permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(out_dir.join("private_key.pem")).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "private key should have mode 0600");
    }
}

#[test]
fn test_keygen_refuses_overwrite_without_force() {
    let tmp = TempDir::new().unwrap();
    let out_dir = tmp.path();

    // First keygen
    let output = assay_cmd()
        .args(["tool", "keygen", "--out"])
        .arg(out_dir)
        .output()
        .expect("failed to run first keygen");
    assert!(output.status.success());

    // Second keygen without --force should fail
    let output = assay_cmd()
        .args(["tool", "keygen", "--out"])
        .arg(out_dir)
        .output()
        .expect("failed to run second keygen");
    assert!(!output.status.success(), "should fail without --force");

    // With --force should succeed
    let output = assay_cmd()
        .args(["tool", "keygen", "--out"])
        .arg(out_dir)
        .args(["--force"])
        .output()
        .expect("failed to run keygen with --force");
    assert!(output.status.success(), "should succeed with --force");
}

#[test]
fn test_sign_verify_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let key_dir = tmp.path().join("keys");
    std::fs::create_dir_all(&key_dir).unwrap();

    // Generate keypair
    let output = assay_cmd()
        .args(["tool", "keygen", "--out"])
        .arg(&key_dir)
        .output()
        .expect("keygen failed");
    assert!(output.status.success());

    // Create tool definition
    let tool_path = tmp.path().join("tool.json");
    let tool_json =
        r#"{"name": "test_tool", "description": "A test tool", "inputSchema": {"type": "object"}}"#;
    std::fs::write(&tool_path, tool_json).unwrap();

    // Sign
    let signed_path = tmp.path().join("signed.json");
    let output = assay_cmd()
        .args(["tool", "sign"])
        .arg(&tool_path)
        .args(["--key"])
        .arg(key_dir.join("private_key.pem"))
        .args(["--out"])
        .arg(&signed_path)
        .output()
        .expect("sign failed");
    assert!(output.status.success(), "sign should succeed");

    // Verify signed file contains x-assay-sig
    let signed_content = std::fs::read_to_string(&signed_path).unwrap();
    assert!(signed_content.contains("x-assay-sig"));
    assert!(signed_content.contains("\"version\": 1"));
    assert!(signed_content.contains("ed25519"));

    // Verify
    let output = assay_cmd()
        .args(["tool", "verify"])
        .arg(&signed_path)
        .args(["--pubkey"])
        .arg(key_dir.join("public_key.pem"))
        .output()
        .expect("verify failed");
    assert!(output.status.success(), "verify should succeed");
    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Verification successful!"));
}

#[test]
fn test_verify_unsigned_exits_0() {
    let tmp = TempDir::new().unwrap();
    let key_dir = tmp.path().join("keys");
    std::fs::create_dir_all(&key_dir).unwrap();

    // Generate keypair
    assay_cmd()
        .args(["tool", "keygen", "--out"])
        .arg(&key_dir)
        .output()
        .expect("keygen failed");

    // Create unsigned tool
    let tool_path = tmp.path().join("unsigned.json");
    std::fs::write(&tool_path, r#"{"name": "unsigned"}"#).unwrap();

    // Verify unsigned (no policy requiring signature) -> exit 0
    let output = assay_cmd()
        .args(["tool", "verify"])
        .arg(&tool_path)
        .args(["--pubkey"])
        .arg(key_dir.join("public_key.pem"))
        .output()
        .expect("verify failed");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("not signed"));
}

#[test]
fn test_verify_wrong_key_exits_4() {
    let tmp = TempDir::new().unwrap();

    // Generate two keypairs
    let key1_dir = tmp.path().join("key1");
    let key2_dir = tmp.path().join("key2");
    std::fs::create_dir_all(&key1_dir).unwrap();
    std::fs::create_dir_all(&key2_dir).unwrap();

    assay_cmd()
        .args(["tool", "keygen", "--out"])
        .arg(&key1_dir)
        .output()
        .unwrap();
    assay_cmd()
        .args(["tool", "keygen", "--out"])
        .arg(&key2_dir)
        .output()
        .unwrap();

    // Create and sign with key1
    let tool_path = tmp.path().join("tool.json");
    std::fs::write(&tool_path, r#"{"name": "test"}"#).unwrap();

    let signed_path = tmp.path().join("signed.json");
    assay_cmd()
        .args(["tool", "sign"])
        .arg(&tool_path)
        .args(["--key"])
        .arg(key1_dir.join("private_key.pem"))
        .args(["--out"])
        .arg(&signed_path)
        .output()
        .unwrap();

    // Verify with key2 -> should fail with exit code 4
    let output = assay_cmd()
        .args(["tool", "verify"])
        .arg(&signed_path)
        .args(["--pubkey"])
        .arg(key2_dir.join("public_key.pem"))
        .output()
        .expect("verify failed");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(4), "wrong key should exit 4");
}

#[test]
fn test_verify_tampered_exits_4() {
    let tmp = TempDir::new().unwrap();
    let key_dir = tmp.path().join("keys");
    std::fs::create_dir_all(&key_dir).unwrap();

    // Generate keypair
    assay_cmd()
        .args(["tool", "keygen", "--out"])
        .arg(&key_dir)
        .output()
        .unwrap();

    // Create and sign
    let tool_path = tmp.path().join("tool.json");
    std::fs::write(&tool_path, r#"{"name": "original", "description": "safe"}"#).unwrap();

    let signed_path = tmp.path().join("signed.json");
    assay_cmd()
        .args(["tool", "sign"])
        .arg(&tool_path)
        .args(["--key"])
        .arg(key_dir.join("private_key.pem"))
        .args(["--out"])
        .arg(&signed_path)
        .output()
        .unwrap();

    // Tamper with signed file
    let content = std::fs::read_to_string(&signed_path).unwrap();
    let tampered = content.replace("safe", "MALICIOUS");
    std::fs::write(&signed_path, tampered).unwrap();

    // Verify -> should fail with exit code 4
    let output = assay_cmd()
        .args(["tool", "verify"])
        .arg(&signed_path)
        .args(["--pubkey"])
        .arg(key_dir.join("public_key.pem"))
        .output()
        .expect("verify failed");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(4), "tampered should exit 4");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("digest mismatch"));
}

#[test]
fn test_sign_requires_output() {
    let tmp = TempDir::new().unwrap();
    let key_dir = tmp.path().join("keys");
    std::fs::create_dir_all(&key_dir).unwrap();

    assay_cmd()
        .args(["tool", "keygen", "--out"])
        .arg(&key_dir)
        .output()
        .unwrap();

    let tool_path = tmp.path().join("tool.json");
    std::fs::write(&tool_path, r#"{"name": "test"}"#).unwrap();

    // Sign without --out or --in-place should fail
    let output = assay_cmd()
        .args(["tool", "sign"])
        .arg(&tool_path)
        .args(["--key"])
        .arg(key_dir.join("private_key.pem"))
        .output()
        .expect("sign failed");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--out") || stderr.contains("--in-place"));
}
