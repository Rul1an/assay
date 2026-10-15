#![cfg(unix)]

use assert_cmd::Command;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const FIXTURE: &[u8] =
    include_bytes!("../../assay-evidence/tests/fixtures/incident_package/valid_present_empty.tar");

struct Inputs {
    dir: tempfile::TempDir,
    digest: String,
}

impl Inputs {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let container = assay_evidence::read_incident_container(FIXTURE).unwrap();
        std::fs::write(dir.path().join("inventory.json"), container.inventory.bytes).unwrap();
        std::fs::write(
            dir.path().join("assessment.json"),
            container.assessment.bytes,
        )
        .unwrap();
        assert_eq!(container.objects.len(), 1);
        let object = &container.objects[0];
        std::fs::write(dir.path().join("source"), &object.bytes).unwrap();
        Self {
            dir,
            digest: object.name.strip_prefix("objects/").unwrap().into(),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn command(&self, out: &Path) -> Command {
        let mut cmd = Command::cargo_bin("assay").unwrap();
        cmd.args(["evidence", "export-incident-package", "--inventory"])
            .arg(self.path("inventory.json"))
            .arg("--assessment")
            .arg(self.path("assessment.json"))
            .arg("--object")
            .arg(format!("{}={}", self.digest, self.path("source").display()))
            .arg("--out")
            .arg(out)
            .timeout(std::time::Duration::from_secs(10));
        cmd
    }
}

fn report(cmd: &mut Command, code: i32) -> Value {
    let output = cmd.assert().code(code).get_output().stdout.clone();
    serde_json::from_slice(&output).expect("structured incident report")
}

#[test]
fn export_matches_original_bytes_and_reverifies() {
    let inputs = Inputs::new();
    let out = inputs.path("export.tar");
    let result = report(&mut inputs.command(&out), 0);
    assert_eq!(result["outcome"], "package_verified");
    assert_eq!(std::fs::read(&out).unwrap(), FIXTURE);
    let verified = report(
        Command::cargo_bin("assay")
            .unwrap()
            .args(["evidence", "verify-incident-package"])
            .arg(&out),
        0,
    );
    assert_eq!(result["artifact_sha256"], verified["artifact_sha256"]);
}

#[test]
fn existing_destination_is_unchanged() {
    let inputs = Inputs::new();
    let out = inputs.path("export.tar");
    std::fs::write(&out, b"existing").unwrap();
    assert_eq!(
        report(&mut inputs.command(&out), 2)["reason"],
        "atomic_output"
    );
    assert_eq!(std::fs::read(out).unwrap(), b"existing");
    assert_eq!(std::fs::read_dir(inputs.dir.path()).unwrap().count(), 4);
}

#[test]
fn changed_source_refuses_without_publication() {
    let inputs = Inputs::new();
    std::fs::write(inputs.path("source"), b"{}\n").unwrap();
    let out = inputs.path("export.tar");
    assert_eq!(report(&mut inputs.command(&out), 2)["reason"], "digest");
    assert!(!out.exists());
    assert_eq!(std::fs::read_dir(inputs.dir.path()).unwrap().count(), 3);
}

#[cfg(unix)]
#[test]
fn source_fifo_refuses_without_blocking() {
    let inputs = Inputs::new();
    std::fs::remove_file(inputs.path("source")).unwrap();
    assert!(std::process::Command::new("mkfifo")
        .arg(inputs.path("source"))
        .status()
        .unwrap()
        .success());
    let out = inputs.path("export.tar");
    assert_eq!(
        report(&mut inputs.command(&out), 2)["reason"],
        "input_shape"
    );
    assert!(!out.exists());
}

#[test]
fn oversized_context_is_bounded_for_verifier() {
    let inputs = Inputs::new();
    let context = inputs.path("context.json");
    // A valid context followed by whitespace remains valid JSON; only the
    // source envelope can reject it before materialization.
    let mut bytes = serde_json::to_vec(&assay_evidence::ContextInput::default()).unwrap();
    bytes.resize(16_777_217, b' ');
    std::fs::write(&context, bytes).unwrap();
    std::fs::write(inputs.path("original.tar"), FIXTURE).unwrap();
    let result = report(
        Command::cargo_bin("assay")
            .unwrap()
            .args(["evidence", "verify-incident-package"])
            .arg(inputs.path("original.tar"))
            .arg("--context")
            .arg(context),
        2,
    );
    assert_eq!(result["reason"], "resource_hard");
}

fn canonical(value: &Value) -> Vec<u8> {
    let mut bytes = assay_canonical::jcs::to_vec(value).unwrap();
    bytes.push(b'\n');
    bytes
}

impl Inputs {
    fn rewrite(
        &self,
        context: &assay_evidence::ContextInput,
        change: impl FnOnce(&mut Value, &mut Value),
    ) {
        let mut inventory: Value =
            serde_json::from_slice(&std::fs::read(self.path("inventory.json")).unwrap()).unwrap();
        let mut assessment: Value =
            serde_json::from_slice(&std::fs::read(self.path("assessment.json")).unwrap()).unwrap();
        assessment["verification_context_sha256"] = hex::encode(Sha256::digest(canonical(
            &serde_json::to_value(context).unwrap(),
        )))
        .into();
        change(&mut inventory, &mut assessment);
        let assessment = canonical(&assessment);
        inventory["assessment_sha256"] = hex::encode(Sha256::digest(&assessment)).into();
        std::fs::write(self.path("assessment.json"), assessment).unwrap();
        std::fs::write(self.path("inventory.json"), canonical(&inventory)).unwrap();
        std::fs::write(
            self.path("context.json"),
            serde_json::to_vec(context).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn report_has_exactly_one_canonical_trailing_lf() {
    let inputs = Inputs::new();
    let output = inputs
        .command(&inputs.path("out.tar"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(output, canonical(&value));
}

#[test]
fn duplicate_occurrences_charge_twice_but_store_one_object() {
    for (limit, expected_code, reason) in [(6, 0, None), (5, 2, Some("resource_selected"))] {
        let inputs = Inputs::new();
        let mut context = assay_evidence::ContextInput::default();
        context.limits.object_bytes_total = limit;
        inputs.rewrite(&context, |inventory, assessment| {
            let mut duplicate = inventory["inputs"][0].clone();
            duplicate["id"] = "second-history".into();
            inventory["inputs"].as_array_mut().unwrap().push(duplicate);
            assessment["surfaces"][4]["source_inputs"] =
                serde_json::json!(["empty-history", "second-history"]);
        });
        let out = inputs.path("out.tar");
        let result = report(
            inputs
                .command(&out)
                .arg("--context")
                .arg(inputs.path("context.json")),
            expected_code,
        );
        assert_eq!(result["reason"].as_str(), reason);
        if expected_code == 0 {
            let archive =
                assay_evidence::read_incident_container(&std::fs::read(out).unwrap()).unwrap();
            assert_eq!(archive.objects.len(), 1);
            assert_eq!(
                serde_json::from_slice::<Value>(&archive.inventory.bytes).unwrap()["inputs"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
        } else {
            assert!(!out.exists());
        }
    }
}

#[test]
fn declaration_budget_refuses_before_opening_object() {
    for field in ["transcript_bytes", "object_bytes_total", "inputs"] {
        let inputs = Inputs::new();
        let mut value = serde_json::to_value(assay_evidence::ContextInput::default()).unwrap();
        value["limits"][field] = if field == "inputs" { 1 } else { 2 }.into();
        let context = serde_json::from_value(value).unwrap();
        inputs.rewrite(&context, |inventory, _| {
            if field == "inputs" {
                let mut duplicate = inventory["inputs"][0].clone();
                duplicate["id"] = "second-history".into();
                inventory["inputs"].as_array_mut().unwrap().push(duplicate);
            }
        });
        std::fs::remove_file(inputs.path("source")).unwrap();
        let out = inputs.path("out.tar");
        assert_eq!(
            report(
                inputs
                    .command(&out)
                    .arg("--context")
                    .arg(inputs.path("context.json")),
                2
            )["reason"],
            "resource_selected",
            "{field}"
        );
        assert!(!out.exists());
    }
}

#[test]
fn same_digest_still_checks_each_format_and_declared_length() {
    for inconsistent_size in [false, true] {
        let inputs = Inputs::new();
        let mut context = assay_evidence::ContextInput::default();
        context.limits.policy_bytes = 2;
        inputs.rewrite(&context, |inventory, _| {
            let mut duplicate = inventory["inputs"][0].clone();
            duplicate["id"] = "second-input".into();
            if inconsistent_size {
                duplicate["bytes"] = 2.into();
            } else {
                duplicate["format"] = "policy-bytes".into();
                duplicate["surfaces"] = serde_json::json!([]);
            }
            inventory["inputs"].as_array_mut().unwrap().push(duplicate);
        });
        std::fs::remove_file(inputs.path("source")).unwrap();
        let out = inputs.path("out.tar");
        assert_eq!(
            report(
                inputs
                    .command(&out)
                    .arg("--context")
                    .arg(inputs.path("context.json")),
                2
            )["reason"],
            if inconsistent_size {
                "digest"
            } else {
                "resource_selected"
            }
        );
        assert!(!out.exists());
    }
}

#[test]
fn stale_assessment_is_not_published() {
    let inputs = Inputs::new();
    inputs.rewrite(&assay_evidence::ContextInput::default(), |_, assessment| {
        assessment["verification_context_sha256"] = "0".repeat(64).into();
    });
    let out = inputs.path("out.tar");
    assert_eq!(
        report(&mut inputs.command(&out), 2)["reason"],
        "stale_assessment"
    );
    assert!(!out.exists());
    assert!(!std::fs::read_dir(inputs.dir.path())
        .unwrap()
        .any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp.")));
}

#[test]
fn surplus_or_duplicate_mapping_does_not_open_missing_paths() {
    for digest in ["0".repeat(64), Inputs::new().digest] {
        let inputs = Inputs::new();
        let out = inputs.path("out.tar");
        let result = report(
            inputs
                .command(&out)
                .arg("--object")
                .arg(format!("{digest}=/nonexistent-assay-export-test")),
            2,
        );
        assert_eq!(
            result["reason"],
            if digest == inputs.digest {
                "input_shape"
            } else {
                "digest"
            }
        );
        assert!(!out.exists());
    }
}

#[cfg(unix)]
#[test]
fn symlinks_and_nonregular_sources_are_refused() {
    for kind in ["link", "directory"] {
        let inputs = Inputs::new();
        std::fs::rename(inputs.path("source"), inputs.path("saved")).unwrap();
        if kind == "link" {
            std::os::unix::fs::symlink(inputs.path("saved"), inputs.path("source")).unwrap();
        } else {
            std::fs::create_dir(inputs.path("source")).unwrap();
        }
        let out = inputs.path("out.tar");
        assert_eq!(
            report(&mut inputs.command(&out), 2)["reason"],
            "input_shape"
        );
        assert!(!out.exists());
        assert_eq!(std::fs::read(inputs.path("saved")).unwrap(), b"[]\n");
    }
}

#[cfg(unix)]
#[test]
fn dangling_destination_symlink_is_preserved() {
    let inputs = Inputs::new();
    let out = inputs.path("out.tar");
    std::os::unix::fs::symlink("missing-target", &out).unwrap();
    assert_eq!(
        report(&mut inputs.command(&out), 2)["reason"],
        "atomic_output"
    );
    assert_eq!(
        std::fs::read_link(out).unwrap(),
        Path::new("missing-target")
    );
    assert!(!inputs.path("missing-target").exists());
}

#[test]
fn concurrent_publication_has_one_winner() {
    let inputs = Inputs::new();
    let out = inputs.path("out.tar");
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let run = || {
            let mut cmd = inputs.command(&out);
            barrier.wait();
            let output = cmd.output().unwrap();
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            (output.status.code().unwrap(), report["reason"].clone())
        };
        let one = scope.spawn(run);
        let two = scope.spawn(run);
        let mut results = [one.join().unwrap(), two.join().unwrap()];
        results.sort_by_key(|result| result.0);
        assert_eq!(results, [(0, Value::Null), (2, "atomic_output".into())]);
    });
    assert_eq!(std::fs::read(out).unwrap(), FIXTURE);
}

#[cfg(unix)]
#[test]
fn stdout_failure_after_publication_is_not_success_or_rollback() {
    use std::process::Stdio;
    let inputs = Inputs::new();
    let out = inputs.path("out.tar");
    let cmd = inputs.command(&out);
    let mut stdcmd = std::process::Command::new(cmd.get_program());
    stdcmd.args(cmd.get_args());
    let mut reader = std::process::Command::new("sh")
        .args(["-c", "exit 0"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let pipe = reader.stdin.take().unwrap();
    reader.wait().unwrap();
    stdcmd.stdout(Stdio::from(pipe));
    assert_eq!(stdcmd.status().unwrap().code(), Some(3));
    assert_eq!(std::fs::read(out).unwrap(), FIXTURE);
}

#[test]
fn duplicate_context_keys_refuse_on_both_commands() {
    let inputs = Inputs::new();
    let mut context = serde_json::to_string(&assay_evidence::ContextInput::default()).unwrap();
    context.insert_str(1, "\"schema_pin\":\"ignored\",");
    std::fs::write(inputs.path("context.json"), context).unwrap();
    std::fs::write(inputs.path("original.tar"), FIXTURE).unwrap();
    let out = inputs.path("out.tar");
    assert_eq!(
        report(
            inputs
                .command(&out)
                .arg("--context")
                .arg(inputs.path("context.json")),
            2
        )["reason"],
        "trust_input"
    );
    assert_eq!(
        report(
            Command::cargo_bin("assay")
                .unwrap()
                .args(["evidence", "verify-incident-package"])
                .arg(inputs.path("original.tar"))
                .arg("--context")
                .arg(inputs.path("context.json")),
            2
        )["reason"],
        "trust_input"
    );
    assert!(!out.exists());
}

#[test]
fn container_limits_refuse_before_opening_objects() {
    for (field, limit) in [("outer_bytes", 1), ("members", 2), ("outer_path_bytes", 71)] {
        let inputs = Inputs::new();
        let mut value = serde_json::to_value(assay_evidence::ContextInput::default()).unwrap();
        value["limits"][field] = limit.into();
        let context = serde_json::from_value(value).unwrap();
        inputs.rewrite(&context, |_, _| {});
        std::fs::remove_file(inputs.path("source")).unwrap();
        let out = inputs.path("out.tar");
        assert_eq!(
            report(
                inputs
                    .command(&out)
                    .arg("--context")
                    .arg(inputs.path("context.json")),
                2
            )["reason"],
            "resource_selected",
            "{field}"
        );
        assert!(!out.exists());
    }
}

#[test]
fn fresh_report_budget_is_checked_before_publication() {
    let inputs = Inputs::new();
    let mut context = assay_evidence::ContextInput::default();
    context.limits.fresh_result_bytes = 1;
    inputs.rewrite(&context, |_, _| {});
    let out = inputs.path("out.tar");
    assert_eq!(
        report(
            inputs
                .command(&out)
                .arg("--context")
                .arg(inputs.path("context.json")),
            2
        )["reason"],
        "resource_selected"
    );
    assert!(!out.exists());
}
