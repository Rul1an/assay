//! Integration tests for `assay mcp tool` signing commands.

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
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(out_dir)
        .output()
        .expect("failed to run assay mcp tool keygen");

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
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(out_dir)
        .output()
        .expect("failed to run first keygen");
    assert!(output.status.success());

    // Second keygen without --force should fail
    let output = assay_cmd()
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(out_dir)
        .output()
        .expect("failed to run second keygen");
    assert!(!output.status.success(), "should fail without --force");

    // With --force should succeed
    let output = assay_cmd()
        .args(["mcp", "tool", "keygen", "--out"])
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
        .args(["mcp", "tool", "keygen", "--out"])
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
        .args(["mcp", "tool", "sign"])
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
        .args(["mcp", "tool", "verify"])
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
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(&key_dir)
        .output()
        .expect("keygen failed");

    // Create unsigned tool
    let tool_path = tmp.path().join("unsigned.json");
    std::fs::write(&tool_path, r#"{"name": "unsigned"}"#).unwrap();

    // Verify unsigned (no policy requiring signature) -> exit 0
    let output = assay_cmd()
        .args(["mcp", "tool", "verify"])
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
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(&key1_dir)
        .output()
        .unwrap();
    assay_cmd()
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(&key2_dir)
        .output()
        .unwrap();

    // Create and sign with key1
    let tool_path = tmp.path().join("tool.json");
    std::fs::write(&tool_path, r#"{"name": "test"}"#).unwrap();

    let signed_path = tmp.path().join("signed.json");
    assay_cmd()
        .args(["mcp", "tool", "sign"])
        .arg(&tool_path)
        .args(["--key"])
        .arg(key1_dir.join("private_key.pem"))
        .args(["--out"])
        .arg(&signed_path)
        .output()
        .unwrap();

    // Verify with key2 -> should fail with exit code 4
    let output = assay_cmd()
        .args(["mcp", "tool", "verify"])
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
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(&key_dir)
        .output()
        .unwrap();

    // Create and sign
    let tool_path = tmp.path().join("tool.json");
    std::fs::write(&tool_path, r#"{"name": "original", "description": "safe"}"#).unwrap();

    let signed_path = tmp.path().join("signed.json");
    assay_cmd()
        .args(["mcp", "tool", "sign"])
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
        .args(["mcp", "tool", "verify"])
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
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(&key_dir)
        .output()
        .unwrap();

    let tool_path = tmp.path().join("tool.json");
    std::fs::write(&tool_path, r#"{"name": "test"}"#).unwrap();

    // Sign without --out or --in-place should fail
    let output = assay_cmd()
        .args(["mcp", "tool", "sign"])
        .arg(&tool_path)
        .args(["--key"])
        .arg(key_dir.join("private_key.pem"))
        .output()
        .expect("sign failed");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--out") || stderr.contains("--in-place"));
}

#[cfg(unix)]
mod keygen_rotation {
    use super::*;
    use std::fs;
    use std::io::Read;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
    use std::path::Path;

    fn generate(dir: &Path, force: bool) -> std::process::Output {
        let mut cmd = assay_cmd();
        cmd.args(["mcp", "tool", "keygen", "--out"]).arg(dir);
        if force {
            cmd.arg("--force");
        }
        cmd.output().unwrap()
    }

    #[test]
    fn rotation_replaces_private_inode_preserves_old_handles_and_public_metadata() {
        use ed25519_dalek::pkcs8::{DecodePrivateKey, DecodePublicKey};
        for mode in [0o644, 0o600] {
            let tmp = TempDir::new().unwrap();
            let private = tmp.path().join("private_key.pem");
            let public = tmp.path().join("public_key.pem");
            let alias = tmp.path().join("old-key");
            fs::write(&private, "old private marker").unwrap();
            fs::set_permissions(&private, fs::Permissions::from_mode(mode)).unwrap();
            fs::hard_link(&private, &alias).unwrap();
            let mut old_fd = fs::File::open(&private).unwrap();
            fs::write(&public, "old public marker").unwrap();
            fs::set_permissions(&public, fs::Permissions::from_mode(0o640)).unwrap();
            let old_private = fs::metadata(&private).unwrap();
            let old_public = fs::metadata(&public).unwrap();
            let output = generate(tmp.path(), true);
            assert!(output.status.success(), "{:?}", output);
            let new_private = fs::metadata(&private).unwrap();
            assert_eq!(new_private.mode() & 0o777, 0o600);
            assert_ne!(old_private.ino(), new_private.ino());
            let mut old_bytes = String::new();
            old_fd.read_to_string(&mut old_bytes).unwrap();
            assert_eq!(old_bytes, "old private marker");
            assert_eq!(fs::read_to_string(alias).unwrap(), old_bytes);
            let new_public = fs::metadata(&public).unwrap();
            assert_eq!(
                (
                    new_public.ino(),
                    new_public.mode(),
                    new_public.uid(),
                    new_public.gid()
                ),
                (
                    old_public.ino(),
                    old_public.mode(),
                    old_public.uid(),
                    old_public.gid()
                )
            );
            let key =
                ed25519_dalek::SigningKey::from_pkcs8_pem(&fs::read_to_string(private).unwrap())
                    .unwrap();
            let pubkey = ed25519_dalek::VerifyingKey::from_public_key_pem(
                &fs::read_to_string(public).unwrap(),
            )
            .unwrap();
            assert_eq!(key.verifying_key(), pubkey);
            assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 3);
        }
    }

    #[test]
    fn unsafe_destinations_are_rejected_before_either_key_changes() {
        for name in ["private_key.pem", "public_key.pem"] {
            for dangling in [false, true] {
                for force in [false, true] {
                    let tmp = TempDir::new().unwrap();
                    let victim = tmp.path().join("victim");
                    if !dangling {
                        fs::write(&victim, "victim marker").unwrap();
                    }
                    let destination = tmp.path().join(name);
                    symlink(&victim, &destination).unwrap();
                    let other = tmp.path().join(if name == "private_key.pem" {
                        "public_key.pem"
                    } else {
                        "private_key.pem"
                    });
                    fs::write(&other, "other marker").unwrap();
                    let before = fs::metadata(&other).unwrap();
                    let output = generate(tmp.path(), force);
                    assert!(
                        !output.status.success(),
                        "accepted {name}, dangling={dangling}, force={force}"
                    );
                    assert!(fs::symlink_metadata(destination)
                        .unwrap()
                        .file_type()
                        .is_symlink());
                    assert_eq!(fs::read_to_string(&other).unwrap(), "other marker");
                    assert_eq!(fs::metadata(other).unwrap().ino(), before.ino());
                    if dangling {
                        assert!(!victim.exists());
                    } else {
                        assert_eq!(fs::read_to_string(victim).unwrap(), "victim marker");
                    }
                }
            }
            let tmp = TempDir::new().unwrap();
            fs::create_dir(tmp.path().join(name)).unwrap();
            assert!(!generate(tmp.path(), true).status.success());
            let other = tmp.path().join(if name == "private_key.pem" {
                "public_key.pem"
            } else {
                "private_key.pem"
            });
            assert!(!other.exists());
        }
    }

    #[test]
    fn restrictive_umask_is_reported_and_public_defaults_are_preserved() {
        let tmp = TempDir::new().unwrap();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "umask 0777; exec \"$@\"", "keygen-umask"])
            .arg(env!("CARGO_BIN_EXE_assay"))
            .args(["mcp", "tool", "keygen", "--out"])
            .arg(tmp.path());
        let output = cmd.output().unwrap();
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(
            fs::metadata(tmp.path().join("private_key.pem"))
                .unwrap()
                .mode()
                & 0o777,
            0
        );
        assert_eq!(
            fs::metadata(tmp.path().join("public_key.pem"))
                .unwrap()
                .mode()
                & 0o777,
            0
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("mode 0000"));
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("control"), "public").unwrap();
        assert!(generate(tmp.path(), false).status.success());
        assert_eq!(
            fs::metadata(tmp.path().join("control")).unwrap().mode(),
            fs::metadata(tmp.path().join("public_key.pem"))
                .unwrap()
                .mode()
        );
    }

    #[test]
    fn public_hardlink_to_old_private_does_not_corrupt_new_private() {
        use ed25519_dalek::pkcs8::{DecodePrivateKey, DecodePublicKey};
        let tmp = TempDir::new().unwrap();
        let private = tmp.path().join("private_key.pem");
        let public = tmp.path().join("public_key.pem");
        fs::write(&private, "old").unwrap();
        fs::hard_link(&private, &public).unwrap();
        assert!(generate(tmp.path(), true).status.success());
        assert_ne!(
            fs::metadata(&private).unwrap().ino(),
            fs::metadata(&public).unwrap().ino()
        );
        let key = ed25519_dalek::SigningKey::from_pkcs8_pem(&fs::read_to_string(private).unwrap())
            .unwrap();
        let pubkey =
            ed25519_dalek::VerifyingKey::from_public_key_pem(&fs::read_to_string(public).unwrap())
                .unwrap();
        assert_eq!(key.verifying_key(), pubkey);
    }

    #[test]
    fn fifo_destinations_are_rejected_without_blocking() {
        use std::time::{Duration, Instant};
        for name in ["private_key.pem", "public_key.pem"] {
            let tmp = TempDir::new().unwrap();
            nix::unistd::mkfifo(&tmp.path().join(name), nix::sys::stat::Mode::S_IRUSR).unwrap();
            let mut child = assay_cmd()
                .args(["mcp", "tool", "keygen", "--force", "--out"])
                .arg(tmp.path())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("keygen blocked on {name}");
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            assert!(!status.success());
            assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn no_force_preserves_existing_bytes_and_inode() {
        let tmp = TempDir::new().unwrap();
        let private = tmp.path().join("private_key.pem");
        fs::write(&private, "old marker").unwrap();
        let before = fs::metadata(&private).unwrap();
        assert!(!generate(tmp.path(), false).status.success());
        assert_eq!(fs::read_to_string(&private).unwrap(), "old marker");
        assert_eq!(fs::metadata(private).unwrap().ino(), before.ino());
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
    }
}
