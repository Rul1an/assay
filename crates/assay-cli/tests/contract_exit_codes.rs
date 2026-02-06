#![allow(deprecated)]
use assay_core::replay::{
    build_file_manifest, read_bundle_tar_gz, write_bundle_tar_gz, BundleEntry, ReplayCoverage,
    ReplayManifest,
};
use assert_cmd::Command;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use tempfile::tempdir;

fn read_run_json(dir: &std::path::Path) -> Value {
    let path = dir.join("run.json");
    if !path.exists() {
        panic!("run.json missing in {}", dir.display());
    }
    let content = fs::read_to_string(&path).unwrap();
    serde_json::from_str(&content).expect("Invalid JSON in run.json")
}

fn read_summary_json(dir: &std::path::Path) -> Value {
    let path = dir.join("summary.json");
    if !path.exists() {
        panic!("summary.json missing in {}", dir.display());
    }
    let content = fs::read_to_string(&path).unwrap();
    serde_json::from_str(&content).expect("Invalid JSON in summary.json")
}

fn assert_schema(v: &Value) {
    assert!(
        v.get("exit_code").expect("missing exit_code").is_i64(),
        "exit_code must be int"
    );
    assert!(
        v.get("reason_code")
            .expect("missing reason_code")
            .is_string(),
        "reason_code must be string"
    );
    if let Some(w) = v.get("warnings") {
        let arr = w.as_array().expect("warnings must be array");
        for item in arr {
            assert!(item.is_string(), "warning items must be strings");
        }
    }
}

/// E7.2: Early-exit run.json must have seed_version present; order_seed/judge_seed keys present and null.
fn assert_run_json_seeds_early_exit(v: &Value) {
    assert_eq!(
        v.get("seed_version").and_then(Value::as_u64),
        Some(1),
        "run.json must have seed_version == 1"
    );
    assert!(v.get("order_seed").is_some(), "order_seed key must exist");
    assert!(v.get("judge_seed").is_some(), "judge_seed key must exist");
    assert!(
        v["order_seed"].is_null(),
        "order_seed must be null on early exit"
    );
    assert!(
        v["judge_seed"].is_null(),
        "judge_seed must be null on early exit"
    );
}

/// E7.2: Successful run run.json: seed_version 1; order_seed string (no number precision loss); judge_seed key present (null until implemented).
fn assert_run_json_seeds_happy(v: &Value) {
    assert_eq!(
        v.get("seed_version").and_then(Value::as_u64),
        Some(1),
        "run.json must have seed_version == 1"
    );
    assert!(
        v["order_seed"].is_string(),
        "order_seed must be string to avoid JSON precision loss"
    );
    assert!(v.get("judge_seed").is_some(), "judge_seed key must exist");
    assert!(
        v["judge_seed"].is_null(),
        "judge_seed reserved, must be null until implemented"
    );
}

/// E7.2: Early-exit summary.json must have seeds with seed_version; order_seed/judge_seed keys present (null or string).
fn assert_summary_seeds_early_exit(v: &Value) {
    let seeds = v
        .get("seeds")
        .expect("summary.json must have seeds on early exit");
    assert_eq!(
        seeds.get("seed_version").and_then(Value::as_u64),
        Some(1),
        "summary seeds must have seed_version == 1"
    );
    assert!(
        seeds.get("order_seed").is_some(),
        "order_seed key must exist"
    );
    assert!(
        seeds.get("judge_seed").is_some(),
        "judge_seed key must exist"
    );
    assert!(
        seeds["order_seed"].is_null() || seeds["order_seed"].is_string(),
        "order_seed must be string or null"
    );
    assert!(
        seeds["judge_seed"].is_null() || seeds["judge_seed"].is_string(),
        "judge_seed must be string or null"
    );
}

/// E7.2: Successful run summary.json: seeds with seed_version; order_seed string, judge_seed null (reserved).
fn assert_summary_seeds_happy(v: &Value) {
    let seeds = v
        .get("seeds")
        .expect("summary.json must have seeds on success");
    assert_eq!(
        seeds.get("seed_version").and_then(Value::as_u64),
        Some(1),
        "summary seeds must have seed_version == 1"
    );
    assert!(
        seeds["order_seed"].is_string(),
        "summary seeds.order_seed must be string (no precision loss)"
    );
    assert!(
        seeds.get("judge_seed").is_some(),
        "judge_seed key must exist"
    );
    assert!(
        seeds["judge_seed"].is_null(),
        "judge_seed reserved, null until implemented"
    );
}

#[test]
fn contract_ci_report_io_failure() {
    let dir = tempdir().unwrap();
    // Valid config, 1 passing test
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: pass\n    input: hello",
    )
    .unwrap();

    // Output is a directory -> IO Error
    let bad_path = dir.path().join("bad_output");
    fs::create_dir(&bad_path).unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("ci")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--junit")
        .arg(&bad_path)
        .arg("--sarif")
        .arg(&bad_path)
        .assert()
        .success(); // Option B: Success

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 0);

    // Verification of Machine-Readable Warnings (Strict)
    let warnings = v
        .get("warnings")
        .expect("warnings field missing in run.json")
        .as_array()
        .expect("warnings must be an array");

    // Expect exactly 2 warnings (JUnit and SARIF)
    assert_eq!(
        warnings.len(),
        2,
        "Expected exactly 2 warnings (JUnit + SARIF)"
    );

    let has_junit = warnings
        .iter()
        .any(|w| w.as_str().unwrap().contains("Failed to write JUnit"));
    let has_sarif = warnings
        .iter()
        .any(|w| w.as_str().unwrap().contains("Failed to write SARIF"));

    assert!(
        has_junit,
        "Missing JUnit warning in run.json. Found: {:?}",
        warnings
    );
    assert!(
        has_sarif,
        "Missing SARIF warning in run.json. Found: {:?}",
        warnings
    );
}

#[test]
fn contract_run_json_always_written_arg_conflict() {
    let dir = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--baseline")
        .arg("dummy")
        .arg("--export-baseline")
        .arg("dummy")
        .assert()
        .code(2);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 2);
    assert_eq!(v["reason_code"], "E_INVALID_ARGS");
}

#[test]
fn contract_reason_code_trace_not_found_v2() {
    let dir = tempdir().unwrap();
    // Valid config schema with ID
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: dummy\n    input: hello",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("missing_trace.jsonl")
        .assert()
        .code(2);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 2);
    assert_eq!(v["reason_code"], "E_TRACE_NOT_FOUND");
    assert_run_json_seeds_early_exit(&v);
    let summary = read_summary_json(dir.path());
    assert_summary_seeds_early_exit(&summary);
}

#[test]
fn contract_legacy_v1_trace_not_found() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: dummy\n    input: hello",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v1")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("missing_trace.jsonl")
        .assert()
        .code(3);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 3);
    assert_eq!(v["reason_code"], "E_TRACE_NOT_FOUND");
}

/// E7.2: Happy path — run completes; run.json and summary.json contain seed_version 1 and integer order_seed/judge_seed.
#[test]
fn contract_e72_seeds_happy_path() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"version: 1
suite: e72-seeds
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected: { type: must_contain, must_contain: ["passed"] }
"#,
    )
    .unwrap();
    // Minimal v2 trace: episode_start + episode_end for t1 with final_output containing "passed"
    fs::write(
        dir.path().join("trace.jsonl"),
        r#"{"type":"episode_start","episode_id":"t1","timestamp":1000,"input":{"prompt":"hi"}}
{"type":"episode_end","episode_id":"t1","timestamp":2000,"final_output":"passed"}
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("trace.jsonl")
        .arg("--strict")
        .assert()
        .success();

    let run = read_run_json(dir.path());
    assert_schema(&run);
    assert_eq!(run["exit_code"], 0);
    assert_run_json_seeds_happy(&run);
    let summary = read_summary_json(dir.path());
    assert_summary_seeds_happy(&summary);
}

#[test]
fn contract_exit_codes_missing_config() {
    let dir = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("non_existent.yaml")
        .assert()
        .code(2);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 2);
}

#[test]
fn contract_replay_missing_dependency_offline() {
    let dir = tempdir().unwrap();
    let bundle_path = dir.path().join("bundle.tar.gz");
    let config = r#"version: 1
suite: replay-missing
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected: { type: must_contain, must_contain: ["ok"] }
"#;

    let entries = vec![BundleEntry {
        path: "files/eval.yaml".to_string(),
        data: config.as_bytes().to_vec(),
    }];
    let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
    manifest.files = Some(build_file_manifest(&entries).unwrap());
    manifest.replay_coverage = Some(ReplayCoverage {
        complete_tests: vec![],
        incomplete_tests: vec!["t1".to_string()],
        reason: Some(BTreeMap::from([(
            "t1".to_string(),
            "trace missing".to_string(),
        )])),
    });

    write_bundle_tar_gz(
        std::fs::File::create(&bundle_path).unwrap(),
        &manifest,
        &entries,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("replay")
        .arg("--bundle")
        .arg("bundle.tar.gz")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_schema(&run);
    assert_eq!(run["exit_code"], 2);
    assert_eq!(run["reason_code"], "E_REPLAY_MISSING_DEPENDENCY");
    assert_run_json_seeds_early_exit(&run);
    assert_eq!(run["provenance"]["replay"], true);
    assert_eq!(run["provenance"]["replay_mode"], "offline");
    assert!(run["provenance"]["bundle_digest"]
        .as_str()
        .unwrap_or_default()
        .starts_with("sha256:"));

    let summary = read_summary_json(dir.path());
    assert_eq!(summary["exit_code"], 2);
    assert_eq!(summary["reason_code"], "E_REPLAY_MISSING_DEPENDENCY");
    assert_summary_seeds_early_exit(&summary);
    assert_eq!(summary["provenance"]["replay"], true);
    assert_eq!(summary["provenance"]["replay_mode"], "offline");
}

#[test]
fn contract_replay_verify_failure_writes_outputs_with_provenance() {
    let dir = tempdir().unwrap();
    let bundle_path = dir.path().join("bad-bundle.tar.gz");
    let unsafe_config = r#"version: 1
suite: replay-verify-fail
model: dummy
settings:
  injected: "OPENAI_API_KEY=sk-abcdefghij1234567890abcdefghij"
"#;

    let entries = vec![BundleEntry {
        path: "files/eval.yaml".to_string(),
        data: unsafe_config.as_bytes().to_vec(),
    }];
    let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
    manifest.files = Some(build_file_manifest(&entries).unwrap());

    write_bundle_tar_gz(
        std::fs::File::create(&bundle_path).unwrap(),
        &manifest,
        &entries,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("replay")
        .arg("--bundle")
        .arg("bad-bundle.tar.gz")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_schema(&run);
    assert_eq!(run["exit_code"], 2);
    assert_eq!(run["reason_code"], "E_CFG_PARSE");
    assert_run_json_seeds_early_exit(&run);
    assert_eq!(run["provenance"]["replay"], true);
    assert_eq!(run["provenance"]["replay_mode"], "offline");
    assert!(run["provenance"]["bundle_digest"]
        .as_str()
        .unwrap_or_default()
        .starts_with("sha256:"));

    let summary = read_summary_json(dir.path());
    assert_eq!(summary["exit_code"], 2);
    assert_eq!(summary["reason_code"], "E_CFG_PARSE");
    assert_summary_seeds_early_exit(&summary);
    assert_eq!(summary["provenance"]["replay"], true);
    assert_eq!(summary["provenance"]["replay_mode"], "offline");
}

#[test]
fn contract_bundle_create_marks_missing_trace_as_incomplete_for_offline_replay() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"version: 1
suite: replay-missing-trace
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected: { type: must_contain, must_contain: ["passed"] }
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("trace.jsonl"),
        r#"{"type":"episode_start","episode_id":"t1","timestamp":1000,"input":{"prompt":"hi"}}
{"type":"episode_end","episode_id":"t1","timestamp":2000,"final_output":"passed"}
"#,
    )
    .unwrap();

    let mut run_cmd = Command::cargo_bin("assay").unwrap();
    run_cmd
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("trace.jsonl")
        .arg("--strict")
        .assert()
        .success();

    // Build a bundle from a run directory where trace input is missing.
    fs::remove_file(dir.path().join("trace.jsonl")).unwrap();

    let mut bundle_cmd = Command::cargo_bin("assay").unwrap();
    bundle_cmd
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("bundle")
        .arg("create")
        .arg("--from")
        .arg(".")
        .arg("--output")
        .arg("replay-missing-trace.tar.gz")
        .assert()
        .success();

    let bundle_file = std::fs::File::open(dir.path().join("replay-missing-trace.tar.gz")).unwrap();
    let bundle = read_bundle_tar_gz(bundle_file).unwrap();
    let coverage = bundle
        .manifest
        .replay_coverage
        .expect("bundle manifest must include replay_coverage");
    assert!(
        coverage.complete_tests.is_empty(),
        "missing trace snapshot must force complete_tests to be empty"
    );
    assert!(
        coverage.incomplete_tests.iter().any(|t| t == "t1"),
        "missing trace snapshot must mark test as incomplete"
    );
    let reason = coverage
        .reason
        .as_ref()
        .and_then(|m| m.get("t1"))
        .cloned()
        .unwrap_or_default();
    assert!(
        reason.contains("trace snapshot missing"),
        "expected trace-missing reason, got: {}",
        reason
    );

    fs::remove_file(dir.path().join("run.json")).unwrap();
    fs::remove_file(dir.path().join("summary.json")).unwrap();

    let mut replay_cmd = Command::cargo_bin("assay").unwrap();
    replay_cmd
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("replay")
        .arg("--bundle")
        .arg("replay-missing-trace.tar.gz")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_schema(&run);
    assert_eq!(run["exit_code"], 2);
    assert_eq!(run["reason_code"], "E_REPLAY_MISSING_DEPENDENCY");
    assert_eq!(run["provenance"]["replay"], true);
    assert_eq!(run["provenance"]["replay_mode"], "offline");

    let summary = read_summary_json(dir.path());
    assert_eq!(summary["exit_code"], 2);
    assert_eq!(summary["reason_code"], "E_REPLAY_MISSING_DEPENDENCY");
    assert_eq!(summary["provenance"]["replay"], true);
    assert_eq!(summary["provenance"]["replay_mode"], "offline");
}

#[test]
fn contract_replay_roundtrip_from_created_bundle() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"version: 1
suite: replay-roundtrip
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected: { type: must_contain, must_contain: ["passed"] }
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("trace.jsonl"),
        r#"{"type":"episode_start","episode_id":"t1","timestamp":1000,"input":{"prompt":"hi"}}
{"type":"episode_end","episode_id":"t1","timestamp":2000,"final_output":"passed"}
"#,
    )
    .unwrap();

    let mut run_cmd = Command::cargo_bin("assay").unwrap();
    run_cmd
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("trace.jsonl")
        .arg("--strict")
        .assert()
        .success();

    let mut bundle_cmd = Command::cargo_bin("assay").unwrap();
    bundle_cmd
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("bundle")
        .arg("create")
        .arg("--from")
        .arg(".")
        .arg("--output")
        .arg("replay.tar.gz")
        .assert()
        .success();

    let original_run = read_run_json(dir.path());
    let bundle_file = std::fs::File::open(dir.path().join("replay.tar.gz")).unwrap();
    let bundle = assay_core::replay::read_bundle_tar_gz(bundle_file).unwrap();
    let coverage = bundle
        .manifest
        .replay_coverage
        .expect("bundle manifest must include replay_coverage");
    assert!(
        coverage.incomplete_tests.is_empty(),
        "roundtrip fixture should be fully replayable"
    );
    assert!(
        !coverage.complete_tests.is_empty(),
        "complete_tests must be present for subset contract"
    );

    fs::remove_file(dir.path().join("run.json")).unwrap();
    fs::remove_file(dir.path().join("summary.json")).unwrap();

    let mut replay_cmd = Command::cargo_bin("assay").unwrap();
    replay_cmd
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("replay")
        .arg("--bundle")
        .arg("replay.tar.gz")
        .assert()
        .success();

    let summary = read_summary_json(dir.path());
    assert_eq!(summary["exit_code"], 0);
    assert_eq!(summary["provenance"]["replay"], true);
    assert_eq!(summary["provenance"]["replay_mode"], "offline");

    let run = read_run_json(dir.path());
    assert_eq!(run["exit_code"], 0);
    assert_eq!(run["provenance"]["replay"], true);
    assert_eq!(run["provenance"]["replay_mode"], "offline");

    let original_status = test_status_map(&original_run);
    let replay_status = test_status_map(&run);
    for test_id in coverage.complete_tests {
        let before = original_status
            .get(&test_id)
            .unwrap_or_else(|| panic!("missing original status for test {}", test_id));
        let after = replay_status
            .get(&test_id)
            .unwrap_or_else(|| panic!("missing replay status for test {}", test_id));
        assert_eq!(
            before, after,
            "subset contract failed for complete test {}",
            test_id
        );
    }
}

#[test]
fn contract_replay_offline_is_hermetic_under_network_deny() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"version: 1
suite: replay-hermetic
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected: { type: must_contain, must_contain: ["passed"] }
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("trace.jsonl"),
        r#"{"type":"episode_start","episode_id":"t1","timestamp":1000,"input":{"prompt":"hi"}}
{"type":"episode_end","episode_id":"t1","timestamp":2000,"final_output":"passed"}
"#,
    )
    .unwrap();

    let mut run_cmd = Command::cargo_bin("assay").unwrap();
    run_cmd
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("trace.jsonl")
        .arg("--strict")
        .assert()
        .success();

    let mut bundle_cmd = Command::cargo_bin("assay").unwrap();
    bundle_cmd
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("bundle")
        .arg("create")
        .arg("--from")
        .arg(".")
        .arg("--output")
        .arg("replay-hermetic.tar.gz")
        .assert()
        .success();

    fs::remove_file(dir.path().join("run.json")).unwrap();
    fs::remove_file(dir.path().join("summary.json")).unwrap();

    let mut replay_cmd = Command::cargo_bin("assay").unwrap();
    replay_cmd
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .env("ASSAY_NETWORK_POLICY", "deny")
        .arg("replay")
        .arg("--bundle")
        .arg("replay-hermetic.tar.gz")
        .assert()
        .success();

    let run = read_run_json(dir.path());
    assert_eq!(run["exit_code"], 0);
    assert_eq!(run["provenance"]["replay"], true);
    assert_eq!(run["provenance"]["replay_mode"], "offline");
}

fn test_status_map(run_json: &Value) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let Some(rows) = run_json.get("results").and_then(Value::as_array) else {
        return out;
    };
    for row in rows {
        let Some(test_id) = row.get("test_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(status) = row.get("status").and_then(Value::as_str) else {
            continue;
        };
        out.insert(test_id.to_string(), status.to_string());
    }
    out
}
