//! Integration tests for #2491: `assay policy activate`, `rollback`, and `status`
//! over a `--policy-root`.

use assay_core::mcp::policy::McpPolicy;
use assert_cmd::Command;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
#[cfg(any(unix, windows))]
use std::process::{Child, Output, Stdio};
#[cfg(any(unix, windows))]
use std::time::{Duration, Instant};

const SCHEMA_ACTIVATION_V0: &str = "assay.policy.activation.v0";

const VALID_A: &str = r#"version: "2.0"
name: policy-a
tools:
  allow:
    - echo
"#;

const VALID_B: &str = r#"version: "2.0"
name: policy-b
tools:
  allow:
    - echo
    - read_file
"#;

const MALFORMED_YAML: &str = ":\n  -";

const BAD_SCHEMA: &str = r#"version: "2.0"
tools:
  allow: [demo]
schemas:
  demo:
    type: object
    properties:
      value:
        type: string
        pattern: "["
"#;

const VALID_C: &str = r#"version: "2.0"
name: policy-c
tools:
  allow:
    - echo
    - read_file
    - write_file
"#;

const VALID_A_REFORMATTED: &str = r#"tools:
  allow: [echo]
name: policy-a
version: "2.0"
"#;

fn assay() -> Command {
    Command::cargo_bin("assay").expect("binary")
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn write_file(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dir");
    }
    std::fs::write(&path, body).expect("write fixture");
    path
}

fn input_sha256_for(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn policy_digest_for(bytes: &[u8]) -> String {
    let policy = McpPolicy::from_slice(bytes).expect("parse policy");
    policy.policy_digest().expect("policy digest")
}

fn cmd_activate(src: &Path, root: &Path, as_name: &str) -> assert_cmd::assert::Assert {
    assay()
        .args([
            "policy",
            "activate",
            src.to_str().expect("utf8"),
            "--root",
            root.to_str().expect("utf8"),
            "--as",
            as_name,
        ])
        .assert()
}

fn cmd_activate_dry_run(src: &Path, root: &Path, as_name: &str) -> assert_cmd::assert::Assert {
    assay()
        .args([
            "policy",
            "activate",
            src.to_str().expect("utf8"),
            "--root",
            root.to_str().expect("utf8"),
            "--as",
            as_name,
            "--dry-run",
        ])
        .assert()
}

#[cfg(any(unix, windows))]
fn spawn_activate_dry_run(src: &Path, root: &Path, as_name: &str, envs: &[(&str, &Path)]) -> Child {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_assay"));
    command
        .args([
            "policy",
            "activate",
            src.to_str().expect("utf8"),
            "--root",
            root.to_str().expect("utf8"),
            "--as",
            as_name,
            "--dry-run",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in envs {
        command.env(name, value);
    }
    command.spawn().expect("spawn bounded dry-run")
}

#[cfg(any(unix, windows))]
fn wait_bounded(mut child: Child, timeout: Duration) -> Output {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().expect("poll dry-run").is_some() {
            return child.wait_with_output().expect("collect dry-run output");
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill timed-out dry-run");
            let output = child.wait_with_output().expect("collect timed-out dry-run");
            panic!(
                "dry-run exceeded {timeout:?}; stdout={:?} stderr={:?}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn tree_snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(base: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .expect("read snapshot directory")
            .map(|entry| entry.expect("snapshot entry"))
            .collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let child = entry.path();
            let rel = child
                .strip_prefix(base)
                .expect("snapshot path under base")
                .to_string_lossy()
                .to_string();
            let meta = std::fs::symlink_metadata(&child).expect("snapshot metadata");
            if meta.is_dir() {
                out.insert(format!("dir:{rel}"), Vec::new());
                visit(base, &child, out);
            } else if meta.file_type().is_symlink() {
                out.insert(
                    format!("symlink:{rel}"),
                    std::fs::read_link(&child)
                        .expect("read snapshot symlink")
                        .to_string_lossy()
                        .as_bytes()
                        .to_vec(),
                );
            } else {
                out.insert(
                    format!("file:{rel}"),
                    std::fs::read(&child).expect("read snapshot file"),
                );
            }
        }
    }

    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

fn cmd_rollback(root: &Path, name: &str) -> assert_cmd::assert::Assert {
    assay()
        .args([
            "policy",
            "rollback",
            name,
            "--root",
            root.to_str().expect("utf8"),
        ])
        .assert()
}

fn cmd_status(root: &Path, name: &str) -> assert_cmd::assert::Assert {
    assay()
        .args([
            "policy",
            "status",
            name,
            "--root",
            root.to_str().expect("utf8"),
        ])
        .assert()
}

fn list_activation_records(root: &Path, name: &str) -> Vec<(String, Value)> {
    let activations_dir = root.join(".assay").join("activations");
    if !activations_dir.exists() {
        return Vec::new();
    }
    let activations_dir_handle = std::fs::File::open(&activations_dir).expect("open activations");
    let suffix = format!("-{name}.json");
    let mut records = Vec::new();
    for entry in std::fs::read_dir(&activations_dir).expect("read activations dir") {
        let entry = entry.expect("entry");
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.ends_with(&suffix) {
            #[cfg(unix)]
            let content = assay_common::atomic_write::read_at(&activations_dir_handle, &file_name)
                .expect("read activation record");
            #[cfg(not(unix))]
            let content =
                std::fs::read(activations_dir.join(&file_name)).expect("read activation record");
            let v: Value = serde_json::from_slice(&content).expect("parse activation record json");
            records.push((file_name, v));
        }
    }
    records.sort_by(|a, b| a.0.cmp(&b.0));
    records
}

// ── Test 1: activate refuses invalid policy and leaves active unchanged ─────────

#[test]
fn activate_refuses_invalid_policy_and_leaves_active_unchanged() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");

    let src_a = write_file(dir.path(), "valid_a.yaml", VALID_A);
    let src_bad = write_file(dir.path(), "bad.yaml", MALFORMED_YAML);

    // Initial valid activation
    cmd_activate(&src_a, &root, "policy.yaml").success();
    let active_path = root.join("policy.yaml");
    assert_eq!(
        std::fs::read_to_string(&active_path).expect("read active"),
        VALID_A
    );
    let records_before = list_activation_records(&root, "policy.yaml");
    assert_eq!(records_before.len(), 1);

    // Attempt to activate invalid policy
    let out = cmd_activate(&src_bad, &root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("E_POLICY_PARSE"),
        "must classify malformed YAML as E_POLICY_PARSE: {stderr}"
    );

    // Active policy must be byte-identical before and after
    assert_eq!(
        std::fs::read_to_string(&active_path).expect("read active"),
        VALID_A,
        "active policy must remain unchanged on activation failure"
    );

    // No new record written
    let records_after = list_activation_records(&root, "policy.yaml");
    assert_eq!(records_after.len(), 1);
}

// ── Test 2: activate replaces pointer and records previous ─────────────────────

#[test]
fn activate_replaces_pointer_and_records_previous() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");

    let src_a = write_file(dir.path(), "src_a.yaml", VALID_A);
    let src_b = write_file(dir.path(), "src_b.yaml", VALID_B);

    let sha_a = input_sha256_for(VALID_A.as_bytes());
    let digest_a = policy_digest_for(VALID_A.as_bytes());
    let sha_b = input_sha256_for(VALID_B.as_bytes());
    let digest_b = policy_digest_for(VALID_B.as_bytes());

    // 1. First activation
    cmd_activate(&src_a, &root, "policy.yaml").success();

    let active_path = root.join("policy.yaml");
    assert_eq!(
        std::fs::read_to_string(&active_path).expect("read active"),
        VALID_A
    );

    let store_file_a = root.join(".assay").join("policy-store").join(&sha_a);
    assert_eq!(
        std::fs::read_to_string(&store_file_a).expect("read store a"),
        VALID_A
    );

    let records_1 = list_activation_records(&root, "policy.yaml");
    assert_eq!(records_1.len(), 1);
    assert_eq!(records_1[0].0, "000001-policy.yaml.json");
    let r1 = &records_1[0].1;
    assert_eq!(r1["schema"], SCHEMA_ACTIVATION_V0);
    assert_eq!(r1["name"], "policy.yaml");
    assert_eq!(r1["input_sha256"], sha_a);
    assert_eq!(r1["policy_digest"], digest_a);
    assert!(r1.get("previous_input_sha256").is_none() || r1["previous_input_sha256"].is_null());
    assert!(r1.get("previous_policy_digest").is_none() || r1["previous_policy_digest"].is_null());
    assert_eq!(r1["assay_version"], env!("CARGO_PKG_VERSION"));
    assert!(r1["activated_at"].is_string());
    assert!(r1.get("rollback_of").is_none());

    // 2. Second activation
    cmd_activate(&src_b, &root, "policy.yaml").success();

    assert_eq!(
        std::fs::read_to_string(&active_path).expect("read active"),
        VALID_B
    );

    let store_file_b = root.join(".assay").join("policy-store").join(&sha_b);
    assert_eq!(
        std::fs::read_to_string(&store_file_b).expect("read store b"),
        VALID_B
    );

    let records_2 = list_activation_records(&root, "policy.yaml");
    assert_eq!(records_2.len(), 2);
    assert_eq!(records_2[1].0, "000002-policy.yaml.json");
    let r2 = &records_2[1].1;
    assert_eq!(r2["schema"], SCHEMA_ACTIVATION_V0);
    assert_eq!(r2["name"], "policy.yaml");
    assert_eq!(r2["input_sha256"], sha_b);
    assert_eq!(r2["policy_digest"], digest_b);
    assert_eq!(r2["previous_input_sha256"], sha_a);
    assert_eq!(r2["previous_policy_digest"], digest_a);
    assert_eq!(r2["assay_version"], env!("CARGO_PKG_VERSION"));
    assert!(r2["activated_at"].is_string());
    assert!(r2.get("rollback_of").is_none());
}

// ── Test 3: rollback restores previous bytes and records it ────────────────────

#[test]
fn rollback_restores_previous_bytes_and_records_it() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");

    let src_a = write_file(dir.path(), "src_a.yaml", VALID_A);
    let src_b = write_file(dir.path(), "src_b.yaml", VALID_B);

    let sha_a = input_sha256_for(VALID_A.as_bytes());
    let digest_a = policy_digest_for(VALID_A.as_bytes());
    let sha_b = input_sha256_for(VALID_B.as_bytes());
    let digest_b = policy_digest_for(VALID_B.as_bytes());

    // Two activations
    cmd_activate(&src_a, &root, "policy.yaml").success();
    cmd_activate(&src_b, &root, "policy.yaml").success();

    let active_path = root.join("policy.yaml");
    assert_eq!(
        std::fs::read_to_string(&active_path).expect("read active"),
        VALID_B
    );

    // Rollback
    cmd_rollback(&root, "policy.yaml").success();

    // Active path now restored to first source
    assert_eq!(
        std::fs::read_to_string(&active_path).expect("read active"),
        VALID_A
    );

    // Third record written
    let records = list_activation_records(&root, "policy.yaml");
    assert_eq!(records.len(), 3);
    assert_eq!(records[2].0, "000003-policy.yaml.json");
    let r3 = &records[2].1;
    assert_eq!(r3["schema"], SCHEMA_ACTIVATION_V0);
    assert_eq!(r3["name"], "policy.yaml");
    assert_eq!(r3["input_sha256"], sha_a);
    assert_eq!(r3["policy_digest"], digest_a);
    assert_eq!(r3["previous_input_sha256"], sha_b);
    assert_eq!(r3["previous_policy_digest"], digest_b);
    assert!(
        r3["rollback_of"].is_string(),
        "latest record must have rollback_of field: {r3:?}"
    );
}

#[test]
fn rollback_rejects_previous_sha_outside_store_namespace() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");

    let src_a = write_file(dir.path(), "src_a.yaml", VALID_A);
    let src_b = write_file(dir.path(), "src_b.yaml", VALID_B);

    cmd_activate(&src_a, &root, "policy.yaml").success();
    cmd_activate(&src_b, &root, "policy.yaml").success();

    let activations_dir = root.join(".assay").join("activations");
    let latest_path = activations_dir.join("000002-policy.yaml.json");
    let latest_bytes = std::fs::read(&latest_path).expect("read latest activation record");
    let mut latest_json: Value =
        serde_json::from_slice(&latest_bytes).expect("parse latest activation record");
    latest_json["previous_input_sha256"] = Value::String("../outside.yaml".to_string());
    std::fs::write(
        &latest_path,
        serde_json::to_vec_pretty(&latest_json).expect("serialize latest activation record"),
    )
    .expect("rewrite latest activation record");

    // If rollback naively joins store_dir + previous_input_sha256, this becomes readable.
    write_file(&root.join(".assay"), "outside.yaml", VALID_A);

    let out = cmd_rollback(&root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid") || stderr.contains("store") || stderr.contains("failed to read"),
        "rollback must reject traversal-like previous_input_sha256: {stderr}"
    );
}

// ── Test 4: stopping before rename leaves old policy readable ──────────────────

#[test]
fn stopping_before_rename_leaves_old_policy_readable() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");

    let _src_a = write_file(dir.path(), "src_a.yaml", VALID_A);
    let active_path = write_file(&root, "policy.yaml", VALID_A);

    // Store new content in policy-store and stage temp file, but do NOT rename over active file
    let store_dir = root.join(".assay").join("policy-store");
    std::fs::create_dir_all(&store_dir).expect("create store dir");
    let sha_b = input_sha256_for(VALID_B.as_bytes());
    assay_common::atomic_write::write_new(&store_dir, &sha_b, VALID_B.as_bytes())
        .expect("write to store");

    let temp_staged = root.join(".tmp-staged-policy.yaml");
    std::fs::write(&temp_staged, VALID_B).expect("write staged temp");

    // R/policy.yaml is unchanged
    assert_eq!(
        std::fs::read_to_string(&active_path).expect("read active"),
        VALID_A
    );

    // Read and parse active policy through McpPolicy reader path
    let active_bytes = std::fs::read(&active_path).expect("read active bytes");
    let loaded = McpPolicy::from_slice(&active_bytes).expect("parse active policy");
    assert_eq!(loaded.name, "policy-a");
}

// ── Test 5: status refuses unrecorded active bytes ─────────────────────────────

#[test]
fn status_refuses_unrecorded_active_bytes() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");

    let src_a = write_file(dir.path(), "src_a.yaml", VALID_A);
    let active_path = root.join("policy.yaml");

    // Activate policy A
    cmd_activate(&src_a, &root, "policy.yaml").success();

    // Status is clean
    cmd_status(&root, "policy.yaml").success();

    // Hand-edit R/policy.yaml with unrecorded bytes
    std::fs::write(&active_path, VALID_B).expect("hand-edit active policy");

    // Status reports disagreement and exits non-zero
    let out = cmd_status(&root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_ne!(out.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let combined = format!("{stderr}\n{stdout}");
    assert!(
        combined.contains("differ")
            || combined.contains("mismatch")
            || combined.contains("unrecorded")
            || combined.contains("not in sync"),
        "status must report active bytes mismatch: {combined}"
    );
}

#[test]
#[cfg(unix)]
fn status_ignores_symlinked_higher_sequence_activation_record() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");
    let src_a = write_file(dir.path(), "src_a.yaml", VALID_A);

    cmd_activate(&src_a, &root, "policy.yaml").success();

    let activations_dir = root.join(".assay").join("activations");
    let outside_record = dir.path().join("outside-record.json");
    let poisoned = serde_json::json!({
        "schema": SCHEMA_ACTIVATION_V0,
        "name": "policy.yaml",
        "input_sha256": "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "policy_digest": "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "assay_version": env!("CARGO_PKG_VERSION"),
        "activated_at": "2026-09-17T00:00:00Z",
        "source": "poison"
    });
    std::fs::write(
        &outside_record,
        serde_json::to_vec_pretty(&poisoned).expect("serialize poisoned record"),
    )
    .expect("write poisoned record");

    std::os::unix::fs::symlink(
        &outside_record,
        activations_dir.join("999999-policy.yaml.json"),
    )
    .expect("create poisoned symlink");

    cmd_status(&root, "policy.yaml").success();
}

// ── Test 6: parity table across validate, resolve and activate ─────────────────

#[test]
fn parity_table_validate_resolve_activate_error_classification() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");

    let p_malformed = write_file(dir.path(), "malformed.yaml", MALFORMED_YAML);
    let p_bad_schema = write_file(dir.path(), "bad_schema.yaml", BAD_SCHEMA);
    let p_missing = dir.path().join("missing.yaml");

    // 1. Malformed YAML
    let val_malformed = assay()
        .args([
            "policy",
            "validate",
            "--input",
            p_malformed.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    let res_malformed = assay()
        .args([
            "policy",
            "resolve",
            "--input",
            p_malformed.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    let act_malformed = assay()
        .args([
            "policy",
            "activate",
            p_malformed.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "--as",
            "p.yaml",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();

    assert_eq!(val_malformed.status.code(), Some(2));
    assert_eq!(res_malformed.status.code(), Some(2));
    assert_eq!(act_malformed.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&val_malformed.stderr).contains("E_POLICY_PARSE"));
    assert!(String::from_utf8_lossy(&res_malformed.stderr).contains("E_POLICY_PARSE"));
    assert!(String::from_utf8_lossy(&act_malformed.stderr).contains("E_POLICY_PARSE"));

    // 2. Bad Schema Compile
    let val_bad_schema = assay()
        .args([
            "policy",
            "validate",
            "--input",
            p_bad_schema.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    let res_bad_schema = assay()
        .args([
            "policy",
            "resolve",
            "--input",
            p_bad_schema.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    let act_bad_schema = assay()
        .args([
            "policy",
            "activate",
            p_bad_schema.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "--as",
            "p.yaml",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();

    assert_eq!(val_bad_schema.status.code(), Some(2));
    assert_eq!(res_bad_schema.status.code(), Some(2));
    assert_eq!(act_bad_schema.status.code(), Some(2));
    let s_val = String::from_utf8_lossy(&val_bad_schema.stderr);
    let s_res = String::from_utf8_lossy(&res_bad_schema.stderr);
    let s_act = String::from_utf8_lossy(&act_bad_schema.stderr);
    assert!(s_val.contains("policy schemas failed to compile:"));
    assert!(s_res.contains("policy schemas failed to compile:"));
    assert!(s_act.contains("policy schemas failed to compile:"));
    assert!(!s_val.contains("E_POLICY_PARSE"));
    assert!(!s_res.contains("E_POLICY_PARSE"));
    assert!(!s_act.contains("E_POLICY_PARSE"));

    // 3. Missing File
    let val_missing = assay()
        .args(["policy", "validate", "--input", p_missing.to_str().unwrap()])
        .assert()
        .failure()
        .get_output()
        .clone();
    let res_missing = assay()
        .args(["policy", "resolve", "--input", p_missing.to_str().unwrap()])
        .assert()
        .failure()
        .get_output()
        .clone();
    let act_missing = assay()
        .args([
            "policy",
            "activate",
            p_missing.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "--as",
            "p.yaml",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();

    assert_eq!(val_missing.status.code(), Some(2));
    assert_eq!(res_missing.status.code(), Some(2));
    assert_eq!(act_missing.status.code(), Some(2));
    let sm_val = String::from_utf8_lossy(&val_missing.stderr);
    let sm_res = String::from_utf8_lossy(&res_missing.stderr);
    let sm_act = String::from_utf8_lossy(&act_missing.stderr);
    assert!(sm_val.contains("failed to load policy") || sm_val.contains("fatal:"));
    assert!(sm_res.contains("failed to load policy") || sm_res.contains("fatal:"));
    assert!(sm_act.contains("failed to load policy") || sm_act.contains("fatal:"));
    assert!(!sm_val.contains("E_POLICY_PARSE"));
    assert!(!sm_res.contains("E_POLICY_PARSE"));
    assert!(!sm_act.contains("E_POLICY_PARSE"));
}

// ── Test 7: missing-file stderr pins main text for validate and resolve ───────

#[test]
fn missing_file_stderr_pins_main_text_for_validate_and_resolve() {
    let dir = tmp();
    let p_missing = dir.path().join("missing.yaml");
    let path_str = p_missing.to_str().unwrap();

    let val = assay()
        .args(["policy", "validate", "--input", path_str])
        .assert()
        .failure()
        .get_output()
        .clone();
    let val_stderr = String::from_utf8_lossy(&val.stderr);
    let norm_val = val_stderr.replace(path_str, "<PATH>");
    let expected_val = "fatal: failed to load policy <PATH>\n\nCaused by:\n    0: failed to read policy <PATH>\n    1: failed to read policy <PATH>\n    2: No such file or directory (os error 2)\n";
    assert_eq!(
        norm_val, expected_val,
        "validate missing file stderr must match main's exact complete error output"
    );

    let res = assay()
        .args(["policy", "resolve", "--input", path_str, "--format", "json"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let res_stderr = String::from_utf8_lossy(&res.stderr);
    let norm_res = res_stderr.replace(path_str, "<PATH>");
    let expected_res = "fatal: failed to load policy <PATH>\n\nCaused by:\n    No such file or directory (os error 2)\n";
    assert_eq!(
        norm_res, expected_res,
        "resolve missing file stderr must match main's exact complete error output"
    );
}

// ── Test 8: activate refuses symlinks and never escapes root ──────────────────

#[test]
#[cfg(unix)]
fn activate_refuses_symlinked_assay_dir_and_writes_nothing_outside() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");

    let outside = dir.path().join("outside_target");
    std::fs::create_dir_all(&outside).expect("create outside target");

    // Create symlink root/.assay -> outside
    std::os::unix::fs::symlink(&outside, root.join(".assay")).expect("create symlink");

    let src = write_file(dir.path(), "valid.yaml", VALID_A);

    let out = cmd_activate(&src, &root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("symlink") || stderr.contains("refusing to operate on symlink"),
        "must refuse symlinked .assay directory: {stderr}"
    );

    // Check that nothing was written outside
    let outside_entries: Vec<_> = std::fs::read_dir(&outside)
        .expect("read outside")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert!(
        outside_entries.is_empty(),
        "outside directory must remain empty, found: {outside_entries:?}"
    );

    // Check that active policy was not created
    assert!(!root.join("policy.yaml").exists());
}

#[test]
#[cfg(unix)]
fn activate_refuses_symlinked_target_name() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");

    let outside = dir.path().join("outside_file.yaml");
    std::fs::write(&outside, "original content").expect("write outside file");

    // Create symlink root/policy.yaml -> outside
    std::os::unix::fs::symlink(&outside, root.join("policy.yaml")).expect("create symlink");

    let src = write_file(dir.path(), "valid.yaml", VALID_A);

    let out = cmd_activate(&src, &root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("symlink") || stderr.contains("refusing to operate on symlink"),
        "must refuse symlinked target: {stderr}"
    );

    // Outside file must remain untouched
    assert_eq!(
        std::fs::read_to_string(&outside).expect("read outside"),
        "original content"
    );
}

// ── Test 9: activate, rollback and status fail cleanly on missing/non-dir root ──

#[test]
fn activate_rollback_status_fail_on_missing_or_non_dir_root() {
    let dir = tmp();
    let missing_root = dir.path().join("nonexistent_root");
    let src = write_file(dir.path(), "valid.yaml", VALID_A);

    // 1. Missing root on activate
    let out_act = cmd_activate(&src, &missing_root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_eq!(out_act.status.code(), Some(2));
    let s_act = String::from_utf8_lossy(&out_act.stderr);
    assert!(
        s_act.contains("does not exist"),
        "activate on missing root must fail cleanly: {s_act}"
    );
    assert!(
        !missing_root.exists(),
        "activate must never auto-create the missing --root directory"
    );

    // 2. Missing root on rollback
    let out_rb = cmd_rollback(&missing_root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_eq!(out_rb.status.code(), Some(2));
    let s_rb = String::from_utf8_lossy(&out_rb.stderr);
    assert!(
        s_rb.contains("does not exist"),
        "rollback on missing root must fail cleanly: {s_rb}"
    );
    assert!(!missing_root.exists());

    // 3. Missing root on status
    let out_st = cmd_status(&missing_root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_eq!(out_st.status.code(), Some(2));
    let s_st = String::from_utf8_lossy(&out_st.stderr);
    assert!(
        s_st.contains("does not exist"),
        "status on missing root must fail cleanly: {s_st}"
    );
    assert!(!missing_root.exists());

    // 4. Non-directory root (pointing to a regular file)
    let file_root = write_file(dir.path(), "file_root", "not a dir");
    let out_file_act = cmd_activate(&src, &file_root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_eq!(out_file_act.status.code(), Some(2));
    let s_file = String::from_utf8_lossy(&out_file_act.stderr);
    assert!(
        s_file.contains("not a directory"),
        "activate on non-directory root must fail: {s_file}"
    );

    let out_file_rb = cmd_rollback(&file_root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_eq!(out_file_rb.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out_file_rb.stderr).contains("not a directory"));

    let out_file_st = cmd_status(&file_root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert_eq!(out_file_st.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out_file_st.stderr).contains("not a directory"));
}

// ── Test 10 (#2491): concurrent activations race on the record sequence ─────
//
// Retry-on-race code under test:
// `crates/assay-cli/src/cli/commands/policy/activate.rs::write_activation_record`
// re-reads the latest record and retries on `AlreadyExists` (bounded by
// `MAX_RECORD_RETRIES`). Documented outcome (`docs/reference/cli/policy.md`):
// concurrent activations on the same sequence number are resolved by bounded
// retries, i.e. the loser retries cleanly — it does not fail and it is not
// refused. Both processes must therefore exit 0.
//
// The rendezvous that forces the collision is the test-only
// `ASSAY_TEST_ACTIVATE_RACE_BARRIER` seam in `write_activation_record`: both
// racing processes block after reading the latest record and before
// publishing, so both attempt `000002-<name>.json` and exactly one takes the
// `AlreadyExists` retry path to `000003-<name>.json`. No sleeps: overlap is
// guaranteed by the barrier, not by timing.
//
// Asserted (all deterministic regardless of which process wins):
// - both activations succeed (loser took the clean-retry path);
// - exactly one consistent active policy: active bytes equal one full source,
//   never a torn mix, and parse as a valid policy;
// - an activation record whose provenance matches the bytes that became
//   active (input_sha256 + policy_digest recomputed independently here), with
//   the matching bytes present in the content store;
// - both policies recorded exactly once; no gaps, dupes, or stray files.
//
// Deliberately NOT asserted: that the *latest* record matches the active
// pointer, the `previous_*` chain across the racing pair, or `status`
// success. Pointer swaps and record publishes are not mutually ordered across
// processes, so either winner order is possible; the retry code sequences
// record names, not pointer-vs-record order.

#[test]
fn concurrent_activations_retry_on_sequence_race_and_leave_single_consistent_active() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");

    // Baseline activation so the racing pair contends for sequence 000002.
    let src_a = write_file(dir.path(), "src_a.yaml", VALID_A);
    cmd_activate(&src_a, &root, "policy.yaml").success();

    let src_b = write_file(dir.path(), "src_b.yaml", VALID_B);
    let src_c = write_file(dir.path(), "src_c.yaml", VALID_C);
    let sha_b = input_sha256_for(VALID_B.as_bytes());
    let digest_b = policy_digest_for(VALID_B.as_bytes());
    let sha_c = input_sha256_for(VALID_C.as_bytes());
    let digest_c = policy_digest_for(VALID_C.as_bytes());

    let barrier_dir = dir.path().join("race-barrier");
    std::fs::create_dir_all(&barrier_dir).expect("create barrier dir");

    let run_racer = |src: &Path, root: &Path, barrier: &Path| {
        assay()
            .args([
                "policy",
                "activate",
                src.to_str().expect("utf8"),
                "--root",
                root.to_str().expect("utf8"),
                "--as",
                "policy.yaml",
            ])
            .env("ASSAY_TEST_ACTIVATE_RACE_BARRIER", barrier)
            .env("ASSAY_TEST_ACTIVATE_RACE_PARTIES", "2")
            .assert()
            .get_output()
            .clone()
    };

    let (out_b, out_c) = std::thread::scope(|scope| {
        let handle_b = scope.spawn(|| run_racer(&src_b, &root, &barrier_dir));
        let handle_c = scope.spawn(|| run_racer(&src_c, &root, &barrier_dir));
        (
            handle_b.join().expect("racer B finished"),
            handle_c.join().expect("racer C finished"),
        )
    });

    assert!(
        out_b.status.success(),
        "racer B must succeed via clean retry: {}",
        String::from_utf8_lossy(&out_b.stderr)
    );
    assert!(
        out_c.status.success(),
        "racer C must succeed via clean retry: {}",
        String::from_utf8_lossy(&out_c.stderr)
    );

    // The seam engaged on both sides; without it no collision is guaranteed.
    let arrivals = std::fs::read_dir(&barrier_dir)
        .expect("read barrier dir")
        .count();
    assert_eq!(
        arrivals, 2,
        "both racers must have met at the pre-publish barrier"
    );

    // Exactly three sequential records: baseline + one per racer.
    let records = list_activation_records(&root, "policy.yaml");
    let names: Vec<&str> = records.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "000001-policy.yaml.json",
            "000002-policy.yaml.json",
            "000003-policy.yaml.json"
        ],
        "racing activations must occupy 000002 and 000003 with no gaps or dupes"
    );
    let on_disk: Vec<_> = std::fs::read_dir(root.join(".assay").join("activations"))
        .expect("read activations dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(
        on_disk.len(),
        3,
        "no stray or temp files may remain beside the three records: {on_disk:?}"
    );

    // Both racing policies recorded exactly once.
    let mut racer_shas: Vec<&str> = records[1..]
        .iter()
        .map(|(_, record)| record["input_sha256"].as_str().expect("input_sha256"))
        .collect();
    racer_shas.sort_unstable();
    let mut expected_shas = vec![sha_b.as_str(), sha_c.as_str()];
    expected_shas.sort_unstable();
    assert_eq!(racer_shas, expected_shas);
    for (_, record) in &records {
        assert_eq!(record["schema"], SCHEMA_ACTIVATION_V0);
    }
    let digest_for = |sha: &str| {
        records
            .iter()
            .find(|(_, record)| record["input_sha256"] == sha)
            .map(|(_, record)| {
                record["policy_digest"]
                    .as_str()
                    .expect("policy_digest")
                    .to_owned()
            })
            .expect("record for sha")
    };
    assert_eq!(digest_for(&sha_b), digest_b);
    assert_eq!(digest_for(&sha_c), digest_c);

    // Exactly one consistent active policy: one full source, never a mix.
    let active_bytes = std::fs::read(root.join("policy.yaml")).expect("read active policy");
    assert!(
        active_bytes == VALID_B.as_bytes() || active_bytes == VALID_C.as_bytes(),
        "active policy must be exactly one racer's bytes (no torn state)"
    );
    McpPolicy::from_slice(&active_bytes).expect("active bytes parse as a policy");

    // That active policy's record provenance matches its bytes, and the
    // content store holds those exact bytes.
    let active_sha = input_sha256_for(&active_bytes);
    let active_digest = policy_digest_for(&active_bytes);
    assert_eq!(digest_for(&active_sha), active_digest);
    let stored = std::fs::read(root.join(".assay").join("policy-store").join(&active_sha))
        .expect("read active policy from store");
    assert_eq!(stored, active_bytes);
    for (sha, expected) in [
        (sha_b.as_str(), VALID_B.as_bytes()),
        (sha_c.as_str(), VALID_C.as_bytes()),
    ] {
        let stored = std::fs::read(root.join(".assay").join("policy-store").join(sha))
            .expect("read racer policy from store");
        assert_eq!(stored, expected, "store must hold exact bytes for {sha}");
    }
}

#[test]
fn activate_dry_run_reports_semantic_change_and_writes_nothing() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");
    let src_a = write_file(dir.path(), "active.yaml", VALID_A);
    cmd_activate(&src_a, &root, "policy.yaml").success();
    let before = tree_snapshot(&root);

    let src_b = write_file(dir.path(), "proposed.yaml", VALID_B);
    let output = cmd_activate_dry_run(&src_b, &root, "policy.yaml")
        .success()
        .get_output()
        .clone();
    assert!(
        output.stdout.is_empty(),
        "dry-run keeps machine stdout empty"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("first_activation=false"), "{stderr}");
    assert!(stderr.contains("byte_change=true"), "{stderr}");
    assert!(stderr.contains("semantic_change=true"), "{stderr}");
    assert!(stderr.contains("writes=none"), "{stderr}");
    assert_eq!(
        tree_snapshot(&root),
        before,
        "dry-run must not mutate the root"
    );
}

#[test]
fn activate_dry_run_reports_reformatted_equivalent_policy_as_semantically_unchanged() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");
    let active = write_file(dir.path(), "active.yaml", VALID_A);
    cmd_activate(&active, &root, "policy.yaml").success();
    let before = tree_snapshot(&root);

    let proposed = write_file(dir.path(), "reformatted.yaml", VALID_A_REFORMATTED);
    assert_ne!(VALID_A.as_bytes(), VALID_A_REFORMATTED.as_bytes());
    assert_eq!(
        policy_digest_for(VALID_A.as_bytes()),
        policy_digest_for(VALID_A_REFORMATTED.as_bytes())
    );
    let output = cmd_activate_dry_run(&proposed, &root, "policy.yaml")
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("byte_change=true"), "{stderr}");
    assert!(stderr.contains("semantic_change=false"), "{stderr}");
    assert_eq!(
        tree_snapshot(&root),
        before,
        "dry-run must preserve every root entry"
    );
}

#[test]
fn activate_dry_run_first_activation_creates_nothing() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");
    let before = tree_snapshot(&root);
    let proposed = write_file(dir.path(), "proposed.yaml", VALID_A);

    let output = cmd_activate_dry_run(&proposed, &root, "policy.yaml")
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("first_activation=true"), "{stderr}");
    assert!(stderr.contains("byte_change=true"), "{stderr}");
    assert!(stderr.contains("semantic_change=true"), "{stderr}");
    assert!(!root.join(".assay").exists());
    assert_eq!(tree_snapshot(&root), before);
}

#[test]
fn activate_dry_run_refuses_invalid_current_or_proposed_policy_without_writes() {
    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");
    let active = write_file(dir.path(), "active.yaml", VALID_A);
    cmd_activate(&active, &root, "policy.yaml").success();

    let malformed = write_file(dir.path(), "malformed.yaml", MALFORMED_YAML);
    let before_bad_proposal = tree_snapshot(&root);
    cmd_activate_dry_run(&malformed, &root, "policy.yaml").failure();
    assert_eq!(tree_snapshot(&root), before_bad_proposal);

    std::fs::write(root.join("policy.yaml"), MALFORMED_YAML).expect("corrupt active policy");
    let proposed = write_file(dir.path(), "proposed.yaml", VALID_B);
    let before_bad_current = tree_snapshot(&root);
    let output = cmd_activate_dry_run(&proposed, &root, "policy.yaml")
        .failure()
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("active policy"));
    assert_eq!(tree_snapshot(&root), before_bad_current);
}

#[cfg(unix)]
#[test]
fn activate_dry_run_refuses_target_replaced_by_symlink_before_open() {
    use std::os::unix::fs::symlink;

    let dir = tmp();
    let root = dir.path().join("root");
    let barrier = dir.path().join("barrier");
    std::fs::create_dir_all(&root).expect("create root");
    std::fs::create_dir_all(&barrier).expect("create barrier");
    let active = write_file(dir.path(), "active.yaml", VALID_A);
    cmd_activate(&active, &root, "policy.yaml").success();
    let outside = write_file(dir.path(), "outside.yaml", VALID_B);
    let proposed = write_file(dir.path(), "proposed.yaml", VALID_A);

    let child = spawn_activate_dry_run(
        &proposed,
        &root,
        "policy.yaml",
        &[("ASSAY_TEST_DRY_RUN_OPEN_BARRIER", barrier.as_path())],
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while std::fs::read_dir(&barrier)
        .expect("read barrier")
        .next()
        .is_none()
    {
        assert!(
            Instant::now() < deadline,
            "dry-run never reached open barrier"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::remove_file(root.join("policy.yaml")).expect("remove checked target");
    symlink(&outside, root.join("policy.yaml")).expect("replace target with symlink");
    std::fs::write(barrier.join("release"), b"").expect("release dry-run");

    let output = wait_bounded(child, Duration::from_secs(5));
    assert!(
        !output.status.success(),
        "dry-run followed replacement symlink"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("symlink") || stderr.contains("regular file"),
        "{stderr}"
    );
    assert!(!stderr.contains("Policy activation preview"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn activate_dry_run_refuses_fifo_target_without_blocking() {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;

    let dir = tmp();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");
    mkfifo(
        root.join("policy.yaml").as_path(),
        Mode::S_IRUSR | Mode::S_IWUSR,
    )
    .expect("create fifo target");
    let proposed = write_file(dir.path(), "proposed.yaml", VALID_A);

    let output = wait_bounded(
        spawn_activate_dry_run(&proposed, &root, "policy.yaml", &[]),
        Duration::from_secs(2),
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("regular file"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn activate_and_dry_run_share_target_type_classification() {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;
    use std::os::unix::fs::symlink;

    for (case, expected_success) in [
        ("regular", true),
        ("symlink", false),
        ("directory", false),
        ("fifo", false),
    ] {
        let dir = tmp();
        let proposed = write_file(dir.path(), "proposed.yaml", VALID_B);
        let outside = write_file(dir.path(), "outside.yaml", VALID_A);
        let dry_root = dir.path().join("dry-root");
        let activate_root = dir.path().join("activate-root");
        std::fs::create_dir_all(&dry_root).expect("create dry root");
        std::fs::create_dir_all(&activate_root).expect("create activation root");

        for root in [&dry_root, &activate_root] {
            let target = root.join("policy.yaml");
            match case {
                "regular" => std::fs::write(target, VALID_A).expect("write regular target"),
                "symlink" => symlink(&outside, target).expect("create symlink target"),
                "directory" => std::fs::create_dir(target).expect("create directory target"),
                "fifo" => {
                    mkfifo(&target, Mode::S_IRUSR | Mode::S_IWUSR).expect("create fifo target")
                }
                _ => unreachable!(),
            }
        }

        let dry_output = wait_bounded(
            spawn_activate_dry_run(&proposed, &dry_root, "policy.yaml", &[]),
            Duration::from_secs(2),
        );
        let activate_output = assay()
            .args([
                "policy",
                "activate",
                proposed.to_str().expect("utf8"),
                "--root",
                activate_root.to_str().expect("utf8"),
                "--as",
                "policy.yaml",
            ])
            .output()
            .expect("run activation classification case");
        assert_eq!(
            dry_output.status.success(),
            activate_output.status.success(),
            "classification drift for {case}: dry stderr={:?}, activation stderr={:?}",
            String::from_utf8_lossy(&dry_output.stderr),
            String::from_utf8_lossy(&activate_output.stderr)
        );
        assert_eq!(
            dry_output.status.success(),
            expected_success,
            "unexpected classification for {case}"
        );
    }
}

#[cfg(windows)]
#[test]
fn activate_dry_run_refuses_root_replaced_by_junction_before_open() {
    let dir = tmp();
    let root = dir.path().join("root");
    let moved_root = dir.path().join("root-before-swap");
    let outside_root = dir.path().join("outside-root");
    let barrier = dir.path().join("barrier");
    std::fs::create_dir_all(&root).expect("create root");
    std::fs::create_dir_all(&outside_root).expect("create outside root");
    std::fs::create_dir_all(&barrier).expect("create barrier");
    let active = write_file(dir.path(), "active.yaml", VALID_A);
    cmd_activate(&active, &root, "policy.yaml").success();
    std::fs::write(outside_root.join("policy.yaml"), VALID_B).expect("write outside target");
    let proposed = write_file(dir.path(), "proposed.yaml", VALID_A);

    let child = spawn_activate_dry_run(
        &proposed,
        &root,
        "policy.yaml",
        &[("ASSAY_TEST_DRY_RUN_OPEN_BARRIER", barrier.as_path())],
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while std::fs::read_dir(&barrier)
        .expect("read barrier")
        .next()
        .is_none()
    {
        assert!(
            Instant::now() < deadline,
            "dry-run never reached open barrier"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::rename(&root, &moved_root).expect("move validated root");
    let junction = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&root)
        .arg(&outside_root)
        .output()
        .expect("create junction");
    assert!(
        junction.status.success(),
        "mklink /J failed: {}",
        String::from_utf8_lossy(&junction.stderr)
    );
    std::fs::write(barrier.join("release"), b"").expect("release dry-run");

    let output = wait_bounded(child, Duration::from_secs(5));
    assert!(
        !output.status.success(),
        "dry-run followed replacement root"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("reparse-point policy root"), "{stderr}");
    assert!(!stderr.contains("Policy activation preview"), "{stderr}");

    std::fs::remove_dir(&root).expect("remove junction");
    std::fs::rename(&moved_root, &root).expect("restore root for cleanup");
}
