use super::*;

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
