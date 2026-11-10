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

// Verified members, end to end. Each cell builds a bundle in code and replays it with the real CLI. A
// canary trace outside the bundle (or an earlier-sorting member) would flip the verdict to a failure if
// replay read it, and its marker would reach the output.

const VERIFIED_MEMBERS_CONFIG: &str = r#"version: 1
suite: replay-verified-members
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected: { type: must_contain, must_contain: ["passed"] }
"#;
const CANARY_MARKER: &str = "CANARY_REPLAY_7e1f";

fn trace_with_output(output: &str) -> Vec<u8> {
    format!(
        "{{\"type\":\"episode_start\",\"episode_id\":\"t1\",\"timestamp\":1000,\"input\":{{\"prompt\":\"hi\"}}}}\n\
         {{\"type\":\"episode_end\",\"episode_id\":\"t1\",\"timestamp\":2000,\"final_output\":\"{output}\"}}\n"
    )
    .into_bytes()
}

fn genuine_entries() -> Vec<BundleEntry> {
    vec![
        BundleEntry {
            path: "files/eval.yaml".to_string(),
            data: VERIFIED_MEMBERS_CONFIG.as_bytes().to_vec(),
        },
        BundleEntry {
            path: "files/trace.jsonl".to_string(),
            data: trace_with_output("passed"),
        },
    ]
}

struct ReplayOutcome {
    code: Option<i32>,
    reason: Value,
    output: String,
}

fn replay_outcome(
    run_dir: &std::path::Path,
    tmpdir: &std::path::Path,
    manifest: &ReplayManifest,
    entries: &[BundleEntry],
) -> ReplayOutcome {
    fs::create_dir_all(run_dir).unwrap();
    fs::create_dir_all(tmpdir).unwrap();
    write_bundle_tar_gz(
        fs::File::create(run_dir.join("bundle.tar.gz")).unwrap(),
        manifest,
        entries,
    )
    .unwrap();
    let out = Command::cargo_bin("assay")
        .unwrap()
        .current_dir(run_dir)
        .env("ASSAY_EXIT_CODES", "v2")
        .env("TMPDIR", tmpdir)
        .arg("replay")
        .arg("--bundle")
        .arg("bundle.tar.gz")
        .output()
        .unwrap();
    let summary = fs::read_to_string(run_dir.join("summary.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or(Value::Null);
    ReplayOutcome {
        code: out.status.code(),
        reason: summary["reason_code"].clone(),
        output: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

fn assert_refused(label: &str, outcome: &ReplayOutcome) {
    assert_eq!(outcome.code, Some(2), "{label}: {}", outcome.output);
    assert_eq!(outcome.reason, "E_CFG_PARSE", "{label}: {}", outcome.output);
    assert!(
        !outcome.output.contains(CANARY_MARKER),
        "{label}: canary content reached the output: {}",
        outcome.output
    );
}

/// A declared trace outside the bundle is refused, and the answer does not depend on whether the
/// host file exists: the same bundle bytes give the same exit and reason with the canary present
/// and absent.
#[test]
fn contract_replay_refuses_a_declared_trace_outside_the_bundle() {
    let dir = tempdir().unwrap();
    let canary = dir.path().join("host").join("canary.jsonl");
    fs::create_dir_all(canary.parent().unwrap()).unwrap();
    let entries = genuine_entries();
    let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
    manifest.files = Some(build_file_manifest(&entries).unwrap());
    manifest.trace_path = Some(canary.to_string_lossy().into_owned());

    fs::write(&canary, trace_with_output(CANARY_MARKER)).unwrap();
    let present = replay_outcome(
        &dir.path().join("present"),
        &dir.path().join("tmp"),
        &manifest,
        &entries,
    );
    fs::remove_file(&canary).unwrap();
    let absent = replay_outcome(
        &dir.path().join("absent"),
        &dir.path().join("tmp"),
        &manifest,
        &entries,
    );
    assert_refused("canary present", &present);
    assert_refused("canary absent", &absent);
}

/// A relative declared trace that climbs out of the replay workspace is refused. TMPDIR places the
/// canary exactly where `../canary.jsonl` resolves from the workspace.
#[test]
fn contract_replay_refuses_a_declared_trace_that_escapes_the_workspace() {
    let dir = tempdir().unwrap();
    let tmp = dir.path().join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    fs::write(tmp.join("canary.jsonl"), trace_with_output(CANARY_MARKER)).unwrap();
    let entries = genuine_entries();
    let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
    manifest.files = Some(build_file_manifest(&entries).unwrap());
    manifest.trace_path = Some("../canary.jsonl".to_string());

    let outcome = replay_outcome(&dir.path().join("run"), &tmp, &manifest, &entries);
    assert_refused("../ trace", &outcome);
}

/// The declared trace is the one replay uses, even when another listed `.jsonl` member sorts first.
#[test]
fn contract_replay_uses_the_declared_trace_over_an_earlier_sorting_member() {
    let dir = tempdir().unwrap();
    let mut entries = genuine_entries();
    entries.push(BundleEntry {
        path: "files/aaa.jsonl".to_string(),
        data: trace_with_output(CANARY_MARKER),
    });
    let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
    manifest.files = Some(build_file_manifest(&entries).unwrap());
    manifest.trace_path = Some("files/trace.jsonl".to_string());

    let outcome = replay_outcome(
        &dir.path().join("run"),
        &dir.path().join("tmp"),
        &manifest,
        &entries,
    );
    assert_eq!(outcome.code, Some(0), "{}", outcome.output);
    assert!(
        !outcome.output.contains(CANARY_MARKER),
        "the earlier-sorting member was used: {}",
        outcome.output
    );
}

/// A member the inventory does not list is refused, even when it would otherwise win selection.
#[test]
fn contract_replay_refuses_an_unlisted_member() {
    let dir = tempdir().unwrap();
    let listed = genuine_entries();
    let mut entries = listed.clone();
    entries.push(BundleEntry {
        path: "files/aaa.jsonl".to_string(),
        data: trace_with_output(CANARY_MARKER),
    });
    let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
    manifest.files = Some(build_file_manifest(&listed).unwrap());
    manifest.trace_path = Some("files/trace.jsonl".to_string());

    let outcome = replay_outcome(
        &dir.path().join("run"),
        &dir.path().join("tmp"),
        &manifest,
        &entries,
    );
    assert_refused("unlisted member", &outcome);
}

/// A payload with no inventory is refused: nothing in it was checked.
#[test]
fn contract_replay_refuses_a_payload_without_an_inventory() {
    let dir = tempdir().unwrap();
    let entries = genuine_entries();
    let manifest = ReplayManifest::minimal("2.15.0".to_string());
    assert!(manifest.files.is_none());

    let outcome = replay_outcome(
        &dir.path().join("run"),
        &dir.path().join("tmp"),
        &manifest,
        &entries,
    );
    assert_refused("no inventory", &outcome);
}

/// The edge cell: a manifest with no payload has nothing to check and still cannot replay, because
/// there is no config. Unchanged by the inventory rule.
#[test]
fn contract_replay_manifest_only_bundle_fails_on_the_missing_config() {
    let dir = tempdir().unwrap();
    let manifest = ReplayManifest::minimal("2.15.0".to_string());
    let outcome = replay_outcome(
        &dir.path().join("run"),
        &dir.path().join("tmp"),
        &manifest,
        &[],
    );
    assert_eq!(outcome.code, Some(2), "{}", outcome.output);
    assert_eq!(
        outcome.reason, "E_REPLAY_MISSING_DEPENDENCY",
        "{}",
        outcome.output
    );
}

/// Replay takes the canonical default config, `files/eval.yaml` and then `files/assay.yaml`, before its name
/// heuristics, so a listed YAML member that sorts earlier is not taken as the config: an args policy next to the
/// config (which this config does not use), and `files/assay.yaml` holding a config that would fail.
#[test]
fn contract_replay_takes_the_default_config_over_an_earlier_sorting_member() {
    let dir = tempdir().unwrap();
    let failing_config =
        VERIFIED_MEMBERS_CONFIG.replace("[\"passed\"]", "[\"absent-from-output\"]");
    let policy = b"version: \"2.0\"\ndeny: [rm]\n".to_vec();
    let cases = [
        (
            "eval.yaml next to args.yaml",
            vec![
                (
                    "files/eval.yaml",
                    VERIFIED_MEMBERS_CONFIG.as_bytes().to_vec(),
                ),
                ("files/args.yaml", policy.clone()),
            ],
        ),
        (
            "assay.yaml next to args.yaml",
            vec![
                (
                    "files/assay.yaml",
                    VERIFIED_MEMBERS_CONFIG.as_bytes().to_vec(),
                ),
                ("files/args.yaml", policy.clone()),
            ],
        ),
        (
            "eval.yaml next to a failing assay.yaml",
            vec![
                (
                    "files/eval.yaml",
                    VERIFIED_MEMBERS_CONFIG.as_bytes().to_vec(),
                ),
                ("files/assay.yaml", failing_config.into_bytes()),
            ],
        ),
    ];
    let mut wrong = Vec::new();
    for (label, members) in cases {
        let mut entries = vec![BundleEntry {
            path: "files/trace.jsonl".to_string(),
            data: trace_with_output("passed"),
        }];
        for (path, data) in members {
            entries.push(BundleEntry {
                path: path.to_string(),
                data,
            });
        }
        let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
        manifest.files = Some(build_file_manifest(&entries).unwrap());
        manifest.trace_path = Some("files/trace.jsonl".to_string());
        let outcome = replay_outcome(
            &dir.path().join(label.replace(' ', "-")),
            &dir.path().join("tmp"),
            &manifest,
            &entries,
        );
        if outcome.code != Some(0) {
            wrong.push(format!(
                "{label}: exit {:?}: {}",
                outcome.code, outcome.output
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the default config was not chosen:\n{}",
        wrong.join("\n")
    );
}

/// Without a default config the existing name heuristics still choose: the first YAML or JSON member under `files/`
/// in name order (`custom.yaml` before a `zeta.json` that is not a config).
#[test]
fn contract_replay_falls_back_to_the_name_heuristics_without_a_default_config() {
    let dir = tempdir().unwrap();
    let entries = vec![
        BundleEntry {
            path: "files/custom.yaml".to_string(),
            data: VERIFIED_MEMBERS_CONFIG.as_bytes().to_vec(),
        },
        BundleEntry {
            path: "files/trace.jsonl".to_string(),
            data: trace_with_output("passed"),
        },
        BundleEntry {
            path: "files/zeta.json".to_string(),
            data: b"{\"not\":\"a config\"}".to_vec(),
        },
    ];
    let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
    manifest.files = Some(build_file_manifest(&entries).unwrap());
    manifest.trace_path = Some("files/trace.jsonl".to_string());
    let outcome = replay_outcome(
        &dir.path().join("run"),
        &dir.path().join("tmp"),
        &manifest,
        &entries,
    );
    assert_eq!(outcome.code, Some(0), "{}", outcome.output);
}

// Bundled config dependencies. The config's `sequence_valid.policy`, `args_valid.policy` (also beside an inline
// schema, where it is read for the fingerprint) and `json_schema.schema_file` must name a listed bundle member next
// to the config; anything else is refused. The canaries for the first, second and fourth field would make the replay
// pass if they were read. Beside an inline schema the verdict does not depend on the policy file at all, so those
// cells only assert the refusal; whether a file is opened is shown separately by the syscall trace.

const CONFIG_INPUTS_MARKER: &str = "CANARY_CONFIG_INPUT_4b2d";

#[derive(Clone, Copy)]
enum DependencyField {
    Sequence,
    Args,
    ArgsBesideInlineSchema,
    SchemaFile,
}

impl DependencyField {
    fn label(self) -> &'static str {
        match self {
            DependencyField::Sequence => "sequence_valid.policy",
            DependencyField::Args => "args_valid.policy",
            DependencyField::ArgsBesideInlineSchema => "args_valid.policy beside an inline schema",
            DependencyField::SchemaFile => "json_schema.schema_file",
        }
    }

    fn expected_yaml(self, path: &str) -> String {
        match self {
            DependencyField::Sequence => format!("{{ type: sequence_valid, policy: {path:?} }}"),
            DependencyField::Args => format!("{{ type: args_valid, policy: {path:?} }}"),
            DependencyField::ArgsBesideInlineSchema => format!(
                "{{ type: args_valid, policy: {path:?}, schema: {{ version: \"2.0\", deny: [rm] }} }}"
            ),
            DependencyField::SchemaFile => {
                format!("{{ type: json_schema, json_schema: \"\", schema_file: {path:?} }}")
            }
        }
    }

    /// Dependency content that lets the test pass, with the canary marker where the format allows it.
    fn passing_content(self, marker: &str) -> String {
        match self {
            DependencyField::Sequence => format!("[] # {marker}\n"),
            DependencyField::Args | DependencyField::ArgsBesideInlineSchema => {
                format!("# {marker}\nversion: \"2.0\"\ndeny: [rm]\n")
            }
            DependencyField::SchemaFile => {
                format!("{{\"$comment\":\"{marker}\",\"type\":\"object\"}}")
            }
        }
    }

    /// Dependency content that makes the test fail (or the policy unreadable), for namesakes outside the bundle.
    fn failing_content(self) -> &'static str {
        match self {
            DependencyField::Sequence => "[\"tool_that_never_ran\"]\n",
            DependencyField::Args | DependencyField::ArgsBesideInlineSchema => "not: [valid\n",
            DependencyField::SchemaFile => "{\"type\":\"string\"}",
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            DependencyField::Sequence => "seq.yaml",
            DependencyField::Args | DependencyField::ArgsBesideInlineSchema => "args.yaml",
            DependencyField::SchemaFile => "schema.json",
        }
    }

    const ALL: [DependencyField; 4] = [
        DependencyField::Sequence,
        DependencyField::Args,
        DependencyField::ArgsBesideInlineSchema,
        DependencyField::SchemaFile,
    ];
}

fn json_output_trace(output: &str) -> Vec<u8> {
    let start = serde_json::json!({"type":"episode_start","episode_id":"t1","timestamp":1000,"input":{"prompt":"hi"}});
    let end = serde_json::json!({"type":"episode_end","episode_id":"t1","timestamp":2000,"final_output":output});
    format!("{start}\n{end}\n").into_bytes()
}

/// A bundle whose config uses `field` with `path`, plus `extra` members listed in the inventory.
fn config_inputs_bundle(
    field: DependencyField,
    path: &str,
    extra: &[(String, Vec<u8>)],
) -> (ReplayManifest, Vec<BundleEntry>) {
    let config = format!(
        "version: 1\nsuite: replay-config-inputs\nmodel: dummy\ntests:\n  - id: t1\n    input: {{ prompt: \"hi\" }}\n    expected: {}\n",
        field.expected_yaml(path)
    );
    let mut entries = vec![
        BundleEntry {
            path: "files/eval.yaml".to_string(),
            data: config.into_bytes(),
        },
        BundleEntry {
            path: "files/trace.jsonl".to_string(),
            data: json_output_trace("{\"ok\":true}"),
        },
    ];
    for (member, data) in extra {
        entries.push(BundleEntry {
            path: member.clone(),
            data: data.clone(),
        });
    }
    let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
    manifest.files = Some(build_file_manifest(&entries).unwrap());
    manifest.trace_path = Some("files/trace.jsonl".to_string());
    (manifest, entries)
}

/// A dependency listed next to the config is the one replay reads. Sequence and args policies are resolved against
/// the config's bundle directory under replay (new behaviour); a listed `schema_file` already works today and is a
/// control, as is a listed args policy beside an inline schema.
#[test]
fn contract_replay_uses_a_listed_dependency_next_to_the_config() {
    let dir = tempdir().unwrap();
    let mut wrong = Vec::new();
    for field in DependencyField::ALL {
        let name = field.file_name();
        let extra = vec![(
            format!("files/{name}"),
            field.passing_content("listed-member").into_bytes(),
        )];
        let (manifest, entries) = config_inputs_bundle(field, name, &extra);
        // A failing namesake in the caller's directory: reading it instead of the member would fail.
        let run_dir = dir
            .path()
            .join(format!("listed-{}", field.label().replace(' ', "-")));
        fs::create_dir_all(&run_dir).unwrap();
        fs::write(run_dir.join(name), field.failing_content()).unwrap();
        let outcome = replay_outcome(&run_dir, &dir.path().join("tmp"), &manifest, &entries);
        if outcome.code != Some(0) {
            wrong.push(format!(
                "{}: exit {:?}: {}",
                field.label(),
                outcome.code,
                outcome.output
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "listed dependencies did not replay:\n{}",
        wrong.join("\n")
    );
}

/// Every dependency path that is not a listed member next to the config is refused before it is read: absolute
/// (canary present and absent give the same answer), `../`, an unlisted name that exists in the caller's
/// directory, and a non-canonical spelling.
#[test]
fn contract_replay_refuses_a_dependency_outside_the_bundle() {
    let dir = tempdir().unwrap();
    let mut wrong = Vec::new();
    for field in DependencyField::ALL {
        let name = field.file_name();
        let canary = field.passing_content(CONFIG_INPUTS_MARKER);
        let case_root = dir.path().join(field.label().replace(' ', "-"));
        let host = case_root.join("host");
        fs::create_dir_all(&host).unwrap();
        let absolute = host.join(name);

        let mut cases: Vec<(String, String, Vec<std::path::PathBuf>)> = Vec::new();
        cases.push((
            "absolute, canary present".into(),
            absolute.to_string_lossy().into_owned(),
            vec![absolute.clone()],
        ));
        cases.push((
            "absolute, canary absent".into(),
            absolute.to_string_lossy().into_owned(),
            vec![],
        ));
        cases.push((
            format!("../{name}"),
            format!("../{name}"),
            vec![case_root.join(name)],
        ));
        cases.push((
            format!("unlisted {name} in the caller's directory"),
            name.to_string(),
            vec![case_root.join("unlisted").join(name)],
        ));
        cases.push((
            format!("./{name}"),
            format!("./{name}"),
            vec![case_root.join("dotslash").join(name)],
        ));

        for (label, path, canaries) in cases {
            for canary_path in &canaries {
                fs::create_dir_all(canary_path.parent().unwrap()).unwrap();
                fs::write(canary_path, &canary).unwrap();
            }
            let run_dir = match label.as_str() {
                l if l.starts_with("unlisted") => case_root.join("unlisted"),
                l if l.starts_with("./") => case_root.join("dotslash"),
                l if l.starts_with("../") => case_root.join("run-parent"),
                _ => case_root.join(format!("run-{}", label.replace([' ', ','], "-"))),
            };
            let (manifest, entries) = config_inputs_bundle(field, &path, &[]);
            let outcome = replay_outcome(&run_dir, &case_root.join("tmp"), &manifest, &entries);
            let refused = outcome.code == Some(2)
                && outcome.reason == "E_CFG_PARSE"
                && !outcome.output.contains(CONFIG_INPUTS_MARKER);
            if !refused {
                wrong.push(format!(
                    "{} / {label}: exit {:?}, reason {}, marker {}",
                    field.label(),
                    outcome.code,
                    outcome.reason,
                    outcome.output.contains(CONFIG_INPUTS_MARKER)
                ));
            }
            for canary_path in &canaries {
                let _ = fs::remove_file(canary_path);
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "dependencies outside the bundle were not refused:\n{}",
        wrong.join("\n")
    );
}

/// An Expected `$ref` stays refused under replay, as it is in an ordinary run.
#[test]
fn contract_replay_still_refuses_an_expected_ref() {
    let dir = tempdir().unwrap();
    let config = "version: 1\nsuite: replay-config-inputs\nmodel: dummy\ntests:\n  - id: t1\n    input: { prompt: \"hi\" }\n    expected: { $ref: \"seq.yaml\" }\n";
    let entries = vec![
        BundleEntry {
            path: "files/eval.yaml".to_string(),
            data: config.as_bytes().to_vec(),
        },
        BundleEntry {
            path: "files/trace.jsonl".to_string(),
            data: json_output_trace("{\"ok\":true}"),
        },
        BundleEntry {
            path: "files/seq.yaml".to_string(),
            data: b"[]\n".to_vec(),
        },
    ];
    let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
    manifest.files = Some(build_file_manifest(&entries).unwrap());
    manifest.trace_path = Some("files/trace.jsonl".to_string());
    let outcome = replay_outcome(
        &dir.path().join("run"),
        &dir.path().join("tmp"),
        &manifest,
        &entries,
    );
    assert_eq!(outcome.code, Some(2), "{}", outcome.output);
}

/// The boundary is replay's: an ordinary `assay run` keeps its own path meanings. The config sits in a directory
/// other than the caller's and the two hold opposite files under the same name, so the verdict shows which one was
/// read: an absolute policy where it points, a relative policy relative to the caller's directory, and a relative
/// `schema_file` relative to the config's directory.
#[test]
fn contract_ordinary_run_keeps_its_dependency_path_semantics() {
    let dir = tempdir().unwrap();
    let caller = dir.path().join("caller");
    let config_dir = dir.path().join("config");
    let elsewhere = dir.path().join("elsewhere");
    for d in [&caller, &config_dir, &elsewhere] {
        fs::create_dir_all(d).unwrap();
    }
    // The file each meaning should pick passes; its namesake in the other directory fails.
    fs::write(elsewhere.join("seq.yaml"), "[]\n").unwrap();
    fs::write(caller.join("seq.yaml"), "[]\n").unwrap();
    fs::write(config_dir.join("seq.yaml"), "[\"tool_that_never_ran\"]\n").unwrap();
    fs::write(config_dir.join("schema.json"), "{\"type\":\"object\"}").unwrap();
    fs::write(caller.join("schema.json"), "{\"type\":\"string\"}").unwrap();
    fs::write(
        caller.join("trace.jsonl"),
        json_output_trace("{\"ok\":true}"),
    )
    .unwrap();
    let absolute = format!(
        "{{ type: sequence_valid, policy: {:?} }}",
        elsewhere.join("seq.yaml").to_string_lossy()
    );
    for (label, expected) in [
        ("absolute policy", absolute),
        (
            "caller-relative policy",
            "{ type: sequence_valid, policy: \"seq.yaml\" }".to_string(),
        ),
        (
            "config-relative schema_file",
            "{ type: json_schema, json_schema: \"\", schema_file: \"schema.json\" }".to_string(),
        ),
    ] {
        fs::write(
            config_dir.join("eval.yaml"),
            format!(
                "version: 1\nsuite: run-paths\nmodel: dummy\ntests:\n  - id: t1\n    input: {{ prompt: \"hi\" }}\n    expected: {expected}\n"
            ),
        )
        .unwrap();
        let out = Command::cargo_bin("assay")
            .unwrap()
            .current_dir(&caller)
            .env("ASSAY_EXIT_CODES", "v2")
            .arg("run")
            .arg("--config")
            .arg(config_dir.join("eval.yaml"))
            .arg("--trace-file")
            .arg("trace.jsonl")
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{label}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
