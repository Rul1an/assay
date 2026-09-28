//! End-to-end contract tests for bounded offline policy lookup (#3225).

use assay_core::mcp::policy::McpPolicy;
use assert_cmd::Command;
use serde_json::Value;

const POLICY: &str = r#"version: "2.0"
name: lookup-demo
tools:
  allow:
    - echo
"#;

const POLICY_REFORMATTED: &str = r#"tools:
  allow: [echo]
name: lookup-demo
version: "2.0"
"#;

const INVALID_SCHEMA_POLICY: &str = r#"version: "2.0"
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
    Command::cargo_bin("assay").expect("assay binary")
}

fn semantic_digest(bytes: &[u8]) -> String {
    McpPolicy::from_slice(bytes)
        .expect("valid policy")
        .policy_digest()
        .expect("digest")
}

fn input_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    format!("sha256:{}", hex::encode(digest))
}

fn activated_fixture() -> (tempfile::TempDir, std::path::PathBuf, String) {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir(&root).expect("policy root");
    let src = temp.path().join("source.yaml");
    std::fs::write(&src, POLICY).expect("source policy");
    assay()
        .args([
            "policy",
            "activate",
            src.to_str().expect("utf8"),
            "--root",
            root.to_str().expect("utf8"),
            "--as",
            "demo.yaml",
        ])
        .assert()
        .success();
    (temp, root, semantic_digest(POLICY.as_bytes()))
}

fn lookup_failure(root: &std::path::Path, digest: &str) -> String {
    let output = assay()
        .args([
            "policy",
            "lookup",
            digest,
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(
        output.stdout.is_empty(),
        "failed lookup emitted a complete report"
    );
    String::from_utf8(output.stderr).expect("UTF-8 stderr")
}

#[test]
fn unique_digest_finds_verified_bytes_and_committed_activation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir(&root).expect("policy root");
    let src = temp.path().join("source.yaml");
    std::fs::write(&src, POLICY).expect("source policy");

    assay()
        .args([
            "policy",
            "activate",
            src.to_str().expect("utf8"),
            "--root",
            root.to_str().expect("utf8"),
            "--as",
            "demo.yaml",
        ])
        .assert()
        .success();

    let digest = semantic_digest(POLICY.as_bytes());
    let output = assay()
        .args([
            "policy",
            "lookup",
            &digest,
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let document: Value = serde_json::from_slice(&output).expect("lookup JSON");
    assert_eq!(document["schema"], "assay.policy.lookup.v0");
    assert_eq!(document["policy_digest"], digest);
    let matches = document["matches"].as_array().expect("matches array");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["input_sha256"], input_sha256(POLICY.as_bytes()));
    assert_eq!(matches[0]["byte_length"], POLICY.len());
    assert_eq!(matches[0]["recorded"], true);
    let activations = matches[0]["activations"].as_array().expect("activations");
    assert_eq!(activations.len(), 1);
    assert_eq!(activations[0]["name"], "demo.yaml");
    assert_eq!(activations[0]["sequence"], 1);
    assert!(activations[0]["record_file"]
        .as_str()
        .expect("record filename")
        .ends_with("-demo.yaml.json"));

    let export = temp.path().join("export.yaml");
    assay()
        .args([
            "policy",
            "lookup",
            &digest,
            "--root",
            root.to_str().expect("utf8"),
            "--input-sha256",
            &input_sha256(POLICY.as_bytes()),
            "--output",
            export.to_str().expect("utf8"),
        ])
        .assert()
        .success();
    assert_eq!(
        std::fs::read(export).expect("exported policy"),
        POLICY.as_bytes()
    );
}

#[test]
fn same_semantic_digest_keeps_raw_byte_variants_and_refuses_ambiguous_export() {
    assert_eq!(
        semantic_digest(POLICY.as_bytes()),
        semantic_digest(POLICY_REFORMATTED.as_bytes()),
        "fixture policies must have one semantic identity"
    );
    assert_ne!(
        input_sha256(POLICY.as_bytes()),
        input_sha256(POLICY_REFORMATTED.as_bytes()),
        "fixture policies must differ in raw-byte identity"
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir(&root).expect("policy root");
    for (name, body) in [("first.yaml", POLICY), ("second.yaml", POLICY_REFORMATTED)] {
        let src = temp.path().join(name);
        std::fs::write(&src, body).expect("source policy");
        assay()
            .args([
                "policy",
                "activate",
                src.to_str().expect("utf8"),
                "--root",
                root.to_str().expect("utf8"),
                "--as",
                "demo.yaml",
            ])
            .assert()
            .success();
    }

    let digest = semantic_digest(POLICY.as_bytes());
    let output = assay()
        .args([
            "policy",
            "lookup",
            &digest,
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let document: Value = serde_json::from_slice(&output).expect("lookup JSON");
    let matches = document["matches"].as_array().expect("matches array");
    assert_eq!(matches.len(), 2);
    assert!(matches[0]["input_sha256"].as_str() < matches[1]["input_sha256"].as_str());
    let mut identities: Vec<_> = matches
        .iter()
        .map(|entry| entry["input_sha256"].as_str().expect("input digest"))
        .collect();
    identities.sort_unstable();
    let mut expected = [
        input_sha256(POLICY.as_bytes()),
        input_sha256(POLICY_REFORMATTED.as_bytes()),
    ];
    expected.sort();
    let expected_refs: Vec<_> = expected.iter().map(String::as_str).collect();
    assert_eq!(identities, expected_refs);
    assert_eq!(
        matches
            .iter()
            .map(|entry| entry["activations"].as_array().expect("activations").len())
            .sum::<usize>(),
        2
    );

    let export = temp.path().join("existing.yaml");
    std::fs::write(&export, b"do not change").expect("existing destination");
    assay()
        .args([
            "policy",
            "lookup",
            &digest,
            "--root",
            root.to_str().expect("utf8"),
            "--output",
            export.to_str().expect("utf8"),
        ])
        .assert()
        .failure();
    assert_eq!(
        std::fs::read(export).expect("preserved destination"),
        b"do not change"
    );
}

#[test]
fn malformed_query_digest_refuses_before_accessing_root() {
    let temp = tempfile::tempdir().expect("tempdir");
    let nonexistent = temp.path().join("no-such-root");
    let output = assay()
        .args([
            "policy",
            "lookup",
            "sha256:NOT-A-DIGEST",
            "--root",
            nonexistent.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("digest"), "stderr: {stderr}");
    assert!(
        !stderr.contains("no-such-root"),
        "root accessed before digest validation: {stderr}"
    );

    let second = assay()
        .args([
            "policy",
            "lookup",
            &semantic_digest(POLICY.as_bytes()),
            "--root",
            nonexistent.to_str().expect("utf8"),
            "--input-sha256",
            "sha256:UPPERCASE",
            "--output",
            temp.path().join("out.yaml").to_str().expect("utf8"),
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    let second_stderr = String::from_utf8_lossy(&second.stderr);
    assert!(second_stderr.contains("digest"));
    assert!(!second_stderr.contains("no-such-root"));
}

#[test]
fn retained_bytes_without_a_committed_record_are_labeled_unrecorded() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir(&root).expect("policy root");
    let src = temp.path().join("source.yaml");
    std::fs::write(&src, POLICY).expect("source policy");
    assay()
        .args([
            "policy",
            "activate",
            src.to_str().expect("utf8"),
            "--root",
            root.to_str().expect("utf8"),
            "--as",
            "demo.yaml",
        ])
        .assert()
        .success();
    let identity = input_sha256(POLICY_REFORMATTED.as_bytes());
    let file_name = identity.replacen(':', "-", 1);
    std::fs::write(
        root.join(".assay/policy-store").join(file_name),
        POLICY_REFORMATTED,
    )
    .expect("retained unrecorded object");

    let output = assay()
        .args([
            "policy",
            "lookup",
            &semantic_digest(POLICY.as_bytes()),
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let document: Value = serde_json::from_slice(&output).expect("lookup JSON");
    let entry = document["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .find(|entry| entry["input_sha256"] == identity)
        .expect("retained identity");
    assert_eq!(entry["recorded"], false);
    assert_eq!(
        entry["activations"].as_array().expect("activations").len(),
        0
    );
}

#[test]
fn unknown_activation_field_is_not_silently_accepted_as_complete_provenance() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir(&root).expect("policy root");
    let src = temp.path().join("source.yaml");
    std::fs::write(&src, POLICY).expect("source policy");
    assay()
        .args([
            "policy",
            "activate",
            src.to_str().expect("utf8"),
            "--root",
            root.to_str().expect("utf8"),
            "--as",
            "demo.yaml",
        ])
        .assert()
        .success();
    let record = root.join(".assay/activations/000001-demo.yaml.json");
    let mut document: Value =
        serde_json::from_slice(&std::fs::read(&record).expect("record")).expect("record JSON");
    document["unearned_claim"] = Value::String("certified".to_owned());
    std::fs::write(&record, serde_json::to_vec(&document).expect("record JSON"))
        .expect("mutated record");

    assay()
        .args([
            "policy",
            "lookup",
            &semantic_digest(POLICY.as_bytes()),
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .failure();
}

#[test]
fn broken_committed_predecessor_is_not_reported_as_a_valid_history() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir(&root).expect("policy root");
    for (name, body) in [("first.yaml", POLICY), ("second.yaml", POLICY_REFORMATTED)] {
        let src = temp.path().join(name);
        std::fs::write(&src, body).expect("source policy");
        assay()
            .args([
                "policy",
                "activate",
                src.to_str().expect("utf8"),
                "--root",
                root.to_str().expect("utf8"),
                "--as",
                "demo.yaml",
            ])
            .assert()
            .success();
    }
    let record = root.join(".assay/activations/000002-demo.yaml.json");
    let mut document: Value =
        serde_json::from_slice(&std::fs::read(&record).expect("record")).expect("record JSON");
    document["previous_input_sha256"] = Value::String(format!("sha256:{}", "0".repeat(64)));
    std::fs::write(&record, serde_json::to_vec(&document).expect("record JSON"))
        .expect("mutated record");

    assay()
        .args([
            "policy",
            "lookup",
            &semantic_digest(POLICY.as_bytes()),
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .failure();
}

#[test]
#[cfg(unix)]
fn conflicting_legacy_alias_refuses_lookup_and_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir(&root).expect("policy root");
    let src = temp.path().join("source.yaml");
    std::fs::write(&src, POLICY).expect("source policy");
    assay()
        .args([
            "policy",
            "activate",
            src.to_str().expect("utf8"),
            "--root",
            root.to_str().expect("utf8"),
            "--as",
            "demo.yaml",
        ])
        .assert()
        .success();

    // The legacy name claims the first object's identity but carries another
    // byte representation. The portable object must not hide it.
    let identity = input_sha256(POLICY.as_bytes());
    std::fs::write(
        root.join(".assay/policy-store").join(&identity),
        POLICY_REFORMATTED,
    )
    .expect("conflicting legacy alias");
    assay()
        .args([
            "policy",
            "lookup",
            &semantic_digest(POLICY.as_bytes()),
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .failure();
    assay()
        .args([
            "policy",
            "status",
            "demo.yaml",
            "--root",
            root.to_str().expect("utf8"),
        ])
        .assert()
        .failure();
}

#[test]
fn oversized_store_object_refuses_before_materialization() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir_all(root.join(".assay/policy-store")).expect("policy store");
    std::fs::create_dir_all(root.join(".assay/activations")).expect("activations");
    // A valid YAML policy with a long comment: removing the ceiling would
    // produce a clean semantic match, not merely a different parse error.
    let oversized = format!("{POLICY}# {}\n", "x".repeat(1_000_000));
    assert!(oversized.len() > 1_000_000);
    let identity = input_sha256(oversized.as_bytes());
    std::fs::write(
        root.join(".assay/policy-store")
            .join(identity.replacen(':', "-", 1)),
        oversized,
    )
    .expect("oversized object");
    let output = assay()
        .args([
            "policy",
            "lookup",
            &semantic_digest(POLICY.as_bytes()),
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("byte ceiling"));
    assert!(output.stdout.is_empty());
}

#[test]
fn concurrent_store_append_is_retried_as_one_bounded_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir(&root).expect("policy root");
    let src = temp.path().join("source.yaml");
    std::fs::write(&src, POLICY).expect("source policy");
    assay()
        .args([
            "policy",
            "activate",
            src.to_str().expect("utf8"),
            "--root",
            root.to_str().expect("utf8"),
            "--as",
            "demo.yaml",
        ])
        .assert()
        .success();

    let barrier = temp.path().join("lookup-barrier");
    std::fs::create_dir(&barrier).expect("barrier");
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_assay"))
        .args([
            "policy",
            "lookup",
            &semantic_digest(POLICY.as_bytes()),
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .env("ASSAY_TEST_LOOKUP_AFTER_FIRST_SCAN", &barrier)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("lookup child");
    let marker = barrier.join("first-scan-complete");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !marker.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "lookup did not reach first-scan barrier"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let second_identity = input_sha256(POLICY_REFORMATTED.as_bytes());
    std::fs::write(
        root.join(".assay/policy-store")
            .join(second_identity.replacen(':', "-", 1)),
        POLICY_REFORMATTED,
    )
    .expect("concurrent retained object");
    std::fs::write(barrier.join("continue"), b"").expect("release lookup");
    let output = child.wait_with_output().expect("lookup result");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: Value = serde_json::from_slice(&output.stdout).expect("lookup JSON");
    assert_eq!(document["matches"].as_array().expect("matches").len(), 2);
}

#[test]
fn object_and_committed_record_appended_between_scans_are_retried() {
    let (temp, root, digest) = activated_fixture();
    let barrier = temp.path().join("store-record-barrier");
    std::fs::create_dir(&barrier).expect("barrier");
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_assay"))
        .args([
            "policy",
            "lookup",
            &digest,
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .env("ASSAY_TEST_LOOKUP_AFTER_STORE_SCAN", &barrier)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("lookup child");
    let marker = barrier.join("store-scan-complete");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !marker.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "lookup did not reach store scan barrier"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let identity = input_sha256(POLICY_REFORMATTED.as_bytes());
    std::fs::write(
        root.join(".assay/policy-store")
            .join(identity.replacen(':', "-", 1)),
        POLICY_REFORMATTED,
    )
    .expect("new store object");
    let records = root.join(".assay/activations");
    let mut new_record: Value = serde_json::from_slice(
        &std::fs::read(records.join("000001-demo.yaml.json")).expect("record"),
    )
    .expect("record JSON");
    new_record["name"] = Value::String("new.yaml".to_owned());
    new_record["input_sha256"] = Value::String(identity);
    std::fs::write(
        records.join("000001-new.yaml.json"),
        serde_json::to_vec(&new_record).expect("record JSON"),
    )
    .expect("new committed record");
    std::fs::write(barrier.join("continue"), b"").expect("release lookup");
    let output = child.wait_with_output().expect("lookup result");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("lookup JSON");
    let matches = report["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches
            .iter()
            .map(|entry| entry["activations"].as_array().expect("activations").len())
            .sum::<usize>(),
        2
    );
}

#[test]
fn continuously_changing_store_refuses_with_named_snapshot_error() {
    let (temp, root, digest) = activated_fixture();
    let first_barrier = temp.path().join("first-barrier");
    let second_barrier = temp.path().join("second-barrier");
    std::fs::create_dir(&first_barrier).expect("first barrier");
    std::fs::create_dir(&second_barrier).expect("second barrier");
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_assay"))
        .args([
            "policy",
            "lookup",
            &digest,
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .env("ASSAY_TEST_LOOKUP_AFTER_FIRST_SCAN", &first_barrier)
        .env("ASSAY_TEST_LOOKUP_AFTER_SECOND_SCAN", &second_barrier)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("lookup child");
    let wait_for = |marker: &std::path::Path| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "lookup did not reach {marker:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    wait_for(&first_barrier.join("first-scan-complete"));
    for (body, barrier) in [
        (POLICY_REFORMATTED.to_owned(), &first_barrier),
        (
            format!("{POLICY}\n# third representation\n"),
            &second_barrier,
        ),
    ] {
        let identity = input_sha256(body.as_bytes());
        std::fs::write(
            root.join(".assay/policy-store")
                .join(identity.replacen(':', "-", 1)),
            body,
        )
        .expect("concurrent store append");
        std::fs::write(barrier.join("continue"), b"").expect("release lookup");
        if barrier == &first_barrier {
            wait_for(&second_barrier.join("second-scan-complete"));
        }
    }
    let output = child.wait_with_output().expect("lookup result");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("changed during lookup"));
}

#[test]
fn a_new_writer_lock_during_lookup_refuses_as_changed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir_all(root.join(".assay/policy-store")).expect("store");
    std::fs::create_dir_all(root.join(".assay/activations")).expect("activations");
    let barrier = temp.path().join("barrier");
    std::fs::create_dir(&barrier).expect("barrier");
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_assay"))
        .args([
            "policy",
            "lookup",
            &semantic_digest(POLICY.as_bytes()),
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .env("ASSAY_TEST_LOOKUP_AFTER_FIRST_SCAN", &barrier)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("lookup child");
    let marker = barrier.join("first-scan-complete");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !marker.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "lookup did not reach barrier"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::fs::write(root.join(".assay/lock"), b"").expect("writer lock appeared");
    std::fs::write(barrier.join("continue"), b"").expect("release lookup");
    let output = child.wait_with_output().expect("lookup result");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("changed during lookup"));
}

#[test]
fn aggregate_byte_ceiling_refuses_even_when_each_object_is_individually_bounded() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    let store = root.join(".assay/policy-store");
    std::fs::create_dir_all(&store).expect("store");
    std::fs::create_dir_all(root.join(".assay/activations")).expect("activations");
    let padding = "x".repeat(989_900);
    for i in 0..68 {
        let body = format!("{POLICY}# {i}{padding}\n");
        assert!(body.len() < 1_000_000);
        let identity = input_sha256(body.as_bytes());
        std::fs::write(store.join(identity.replacen(':', "-", 1)), body).expect("object");
    }
    assert!(lookup_failure(&root, &semantic_digest(POLICY.as_bytes())).contains("byte ceiling"));
}

#[test]
fn store_entry_ceiling_refuses_instead_of_truncating_results() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    let store = root.join(".assay/policy-store");
    std::fs::create_dir_all(&store).expect("store");
    std::fs::create_dir_all(root.join(".assay/activations")).expect("activations");
    for i in 0..10_001 {
        let body = format!("{POLICY}# representation {i}\n");
        let identity = input_sha256(body.as_bytes());
        std::fs::write(store.join(identity.replacen(':', "-", 1)), body).expect("object");
    }
    assert!(lookup_failure(&root, &semantic_digest(POLICY.as_bytes())).contains("entry ceiling"));
}

#[test]
fn activation_entry_ceiling_refuses_instead_of_truncating_history() {
    let (_temp, root, digest) = activated_fixture();
    let dir = root.join(".assay/activations");
    let original: Value =
        serde_json::from_slice(&std::fs::read(dir.join("000001-demo.yaml.json")).expect("record"))
            .expect("record JSON");
    for i in 0..10_000 {
        let name = format!("p{i:05}.yaml");
        let mut record = original.clone();
        record["name"] = Value::String(name.clone());
        std::fs::write(
            dir.join(format!("000001-{name}.json")),
            serde_json::to_vec(&record).expect("JSON"),
        )
        .expect("record");
    }
    assert!(lookup_failure(&root, &digest).contains("entry ceiling"));
}

#[test]
fn corrupt_or_missing_stored_bytes_never_become_lookup_matches() {
    let (_temp, root, digest) = activated_fixture();
    let object = root
        .join(".assay/policy-store")
        .join(input_sha256(POLICY.as_bytes()).replacen(':', "-", 1));
    std::fs::write(&object, POLICY_REFORMATTED).expect("replace object bytes");
    assert!(lookup_failure(&root, &digest).contains("digest mismatch"));
    std::fs::remove_file(&object).expect("remove object");
    assert!(lookup_failure(&root, &digest).contains("no retained policy-store object"));
}

#[test]
fn every_stored_candidate_uses_the_shared_schema_compiling_loader() {
    let (_temp, root, digest) = activated_fixture();
    let identity = input_sha256(INVALID_SCHEMA_POLICY.as_bytes());
    std::fs::write(
        root.join(".assay/policy-store")
            .join(identity.replacen(':', "-", 1)),
        INVALID_SCHEMA_POLICY,
    )
    .expect("invalid schema candidate");
    assert!(lookup_failure(&root, &digest).contains("policy schemas failed to compile"));
}

#[test]
fn contradictory_or_oversized_committed_record_refuses_without_result() {
    let (_temp, root, digest) = activated_fixture();
    let record = root.join(".assay/activations/000001-demo.yaml.json");
    let original = std::fs::read(&record).expect("record");
    let mut document: Value = serde_json::from_slice(&original).expect("record JSON");
    document["policy_digest"] = Value::String(format!("sha256:{}", "0".repeat(64)));
    std::fs::write(&record, serde_json::to_vec(&document).expect("JSON")).expect("mutation");
    assert!(lookup_failure(&root, &digest).contains("policy digest mismatch"));

    // Whitespace remains valid JSON, so only the byte ceiling can refuse it.
    let mut oversized = original;
    oversized.resize(65_537, b' ');
    std::fs::write(&record, oversized).expect("oversized record");
    assert!(lookup_failure(&root, &digest).contains("byte ceiling"));
}

#[test]
fn malformed_activation_timestamp_is_not_reported_as_provenance() {
    let (_temp, root, digest) = activated_fixture();
    let record = root.join(".assay/activations/000001-demo.yaml.json");
    let mut document: Value =
        serde_json::from_slice(&std::fs::read(&record).expect("record")).expect("JSON");
    document["activated_at"] = Value::String("yesterday".to_owned());
    std::fs::write(&record, serde_json::to_vec(&document).expect("JSON")).expect("mutation");
    assert!(lookup_failure(&root, &digest).contains("activated_at"));
    assay()
        .args([
            "policy",
            "status",
            "demo.yaml",
            "--root",
            root.to_str().expect("utf8"),
        ])
        .assert()
        .failure();
}

#[test]
#[cfg(unix)]
fn symlinked_record_and_noncanonical_store_name_refuse() {
    use std::os::unix::fs::symlink;
    let (_temp, root, digest) = activated_fixture();
    let record = root.join(".assay/activations/000001-demo.yaml.json");
    let preserved = record.with_extension("saved");
    std::fs::rename(&record, &preserved).expect("preserve record");
    symlink(&preserved, &record).expect("symlink candidate");
    assert!(lookup_failure(&root, &digest).contains("not a regular file"));
    std::fs::remove_file(&record).expect("remove symlink");
    std::fs::rename(&preserved, &record).expect("restore record");
    std::fs::write(root.join(".assay/policy-store/not-a-digest"), b"").expect("invalid locator");
    assert!(lookup_failure(&root, &digest).contains("invalid policy-store object"));
}

#[test]
fn failed_export_keeps_existing_bytes_and_cleans_owned_temp() {
    let (temp, root, digest) = activated_fixture();
    let output = temp.path().join("export.yaml");
    std::fs::write(&output, b"prior bytes").expect("prior output");
    let result = assay()
        .args([
            "policy",
            "lookup",
            &digest,
            "--root",
            root.to_str().expect("utf8"),
            "--output",
            output.to_str().expect("utf8"),
        ])
        .env("ASSAY_TEST_LOOKUP_FAIL_BEFORE_OUTPUT_RENAME", "1")
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(result.stdout.is_empty());
    assert_eq!(
        std::fs::read(&output).expect("prior output"),
        b"prior bytes"
    );
    let leftovers: Vec<_> = std::fs::read_dir(temp.path())
        .expect("output parent")
        .map(|entry| entry.expect("entry").file_name())
        .filter(|name| name.to_string_lossy().starts_with(".assay-lookup-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "lookup temp files remain: {leftovers:?}"
    );
}

#[test]
fn export_cannot_replace_a_verified_store_object() {
    let (temp, root, digest) = activated_fixture();
    let second = temp.path().join("reformatted.yaml");
    std::fs::write(&second, POLICY_REFORMATTED).expect("second policy");
    assay()
        .args([
            "policy",
            "activate",
            second.to_str().expect("utf8"),
            "--root",
            root.to_str().expect("utf8"),
            "--as",
            "demo.yaml",
        ])
        .assert()
        .success();
    let identity = input_sha256(POLICY_REFORMATTED.as_bytes());
    let store_object = root
        .join(".assay/policy-store")
        .join(identity.replacen(':', "-", 1));
    let before = std::fs::read(&store_object).expect("stored bytes");
    assay()
        .args([
            "policy",
            "lookup",
            &digest,
            "--root",
            root.to_str().expect("utf8"),
            "--input-sha256",
            &input_sha256(POLICY.as_bytes()),
            "--output",
            store_object.to_str().expect("utf8"),
        ])
        .assert()
        .failure();
    assert_eq!(
        std::fs::read(store_object).expect("retained object"),
        before
    );
}

#[test]
#[cfg(unix)]
fn legacy_and_portable_aliases_share_one_verified_identity() {
    let (_temp, root, digest) = activated_fixture();
    let identity = input_sha256(POLICY.as_bytes());
    let store = root.join(".assay/policy-store");
    let portable = store.join(identity.replacen(':', "-", 1));
    let legacy = store.join(&identity);
    std::fs::rename(&portable, &legacy).expect("legacy-only object");
    let legacy_report = assay()
        .args([
            "policy",
            "lookup",
            &digest,
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let legacy_doc: Value = serde_json::from_slice(&legacy_report).expect("JSON");
    assert_eq!(legacy_doc["matches"].as_array().expect("matches").len(), 1);
    assert_eq!(
        legacy_doc["matches"][0]["store_filenames"],
        serde_json::json!([identity])
    );

    std::fs::copy(&legacy, &portable).expect("identical portable alias");
    let dual_report = assay()
        .args([
            "policy",
            "lookup",
            &digest,
            "--root",
            root.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dual_doc: Value = serde_json::from_slice(&dual_report).expect("JSON");
    assert_eq!(dual_doc["matches"].as_array().expect("matches").len(), 1);
    assert_eq!(
        dual_doc["matches"][0]["store_filenames"]
            .as_array()
            .expect("aliases")
            .len(),
        2
    );
}
