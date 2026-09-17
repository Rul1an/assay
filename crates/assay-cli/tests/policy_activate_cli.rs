//! Integration tests for #2491: `assay policy activate`, `rollback`, and `status`
//! over a `--policy-root`.

use assay_core::mcp::policy::McpPolicy;
use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};

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
    let suffix = format!("-{name}.json");
    let mut records = Vec::new();
    for entry in std::fs::read_dir(&activations_dir).expect("read activations dir") {
        let entry = entry.expect("entry");
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.ends_with(&suffix) {
            let content = std::fs::read(entry.path()).expect("read activation record");
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
