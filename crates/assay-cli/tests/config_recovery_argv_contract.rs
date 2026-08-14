//! Binary contract for argument-safe recovery from `E_CFG_PARSE` (#2200).

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, ProcessLimits};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

const LIMITS: ProcessLimits = ProcessLimits::new(Duration::from_secs(5), 64 * 1024, 64 * 1024);

fn assay_command_with_environment(
    cwd: &Path,
    environment_names: impl IntoIterator<Item = OsString>,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    for name in environment_names {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_")
        {
            command.env_remove(name);
        }
    }
    command.current_dir(cwd).env("NO_COLOR", "1");
    command
}

fn assay_command(cwd: &Path) -> Command {
    assay_command_with_environment(cwd, std::env::vars_os().map(|(name, _)| name))
}

#[test]
fn assay_command_scrubs_assay_environment_case_insensitively() {
    let command = assay_command_with_environment(
        Path::new("."),
        [
            OsString::from("ASSAY_EXIT_CODES"),
            OsString::from("assay_vcr_dir"),
            OsString::from("NO_COLOR"),
        ],
    );
    let env = command.get_envs().collect::<Vec<_>>();

    assert!(env
        .iter()
        .any(|(name, value)| *name == "ASSAY_EXIT_CODES" && value.is_none()));
    assert!(env
        .iter()
        .any(|(name, value)| *name == "assay_vcr_dir" && value.is_none()));
    assert!(env
        .iter()
        .any(|(name, value)| *name == "NO_COLOR" && *value == Some("1".as_ref())));
}

fn assay<T: AsRef<OsStr>>(cwd: &Path, args: &[T], context: &str) -> Output {
    let mut command = assay_command(cwd);
    command.args(args);
    run_bounded(command, b"", LIMITS, context).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn config_parse_recovery_argv_executes_without_a_shell() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_name = "cfg file;$(touch should-not-exist).yaml";
    std::fs::write(dir.path().join(config_name), "version: [\n").expect("write malformed config");

    let failed = assay(
        dir.path(),
        &["run", "--format", "json", "--config", config_name],
        "produce config recovery",
    );
    assert_eq!(
        failed.status.code(),
        Some(2),
        "malformed config must be a config error: {}",
        String::from_utf8_lossy(&failed.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&failed.stdout).expect("failure stdout must be JSON");
    assert_eq!(summary["reason_code"], "E_CFG_PARSE");

    let encoded = summary["next_step"]
        .as_str()
        .expect("config failure next_step")
        .strip_prefix("Run argv: ")
        .expect("config recovery must publish JSON argv");
    let recovery: Vec<String> =
        serde_json::from_str(encoded).expect("config recovery argv must parse");
    assert_eq!(
        recovery,
        vec![
            "assay".to_string(),
            "doctor".to_string(),
            format!("--config={config_name}"),
            "--format".to_string(),
            "json".to_string()
        ],
        "the hostile path must remain one fused argv element"
    );

    let recovered = assay(dir.path(), &recovery[1..], "execute config recovery");
    assert_eq!(
        recovered.status.code(),
        failed.status.code(),
        "the published recovery must not reclassify the failure it recovers from: {}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert_recovery_reached_cfg_parse_diagnosis(&recovered, "hostile-path recovery");
    assert!(!String::from_utf8_lossy(&recovered.stdout).contains("Usage:"));
    assert!(
        !dir.path().join("should-not-exist").exists(),
        "recovery must not execute shell metacharacters"
    );
}

fn recovery_argv(document: &serde_json::Value) -> Vec<String> {
    let encoded = document["next_step"]
        .as_str()
        .expect("failure next_step")
        .strip_prefix("Run argv: ")
        .expect("config recovery must publish JSON argv");
    serde_json::from_str(encoded).expect("config recovery argv must parse")
}

fn assert_recovery_reached_cfg_parse_diagnosis(recovered: &Output, context: &str) {
    let stderr = String::from_utf8_lossy(&recovered.stderr);
    assert!(
        !stderr.contains("unexpected argument")
            && !stderr.contains("a value is required for '--config"),
        "{context}: published argv was refused by clap before the config was read:\n{stderr}"
    );
    assert!(
        !recovered.stdout.is_empty(),
        "{context}: recovery produced no diagnosis (empty stdout); stderr:\n{stderr}"
    );
    let diagnosis: serde_json::Value =
        serde_json::from_slice(&recovered.stdout).unwrap_or_else(|error| {
            panic!(
                "{context}: recovery must produce a parseable diagnosis, not human text or clap \
                 usage: {error}\nstdout:\n{}\nstderr:\n{stderr}",
                String::from_utf8_lossy(&recovered.stdout)
            )
        });
    assert_eq!(
        diagnosis["reason_code"], "E_CFG_PARSE",
        "{context}: recovery must reach the config diagnosis, not a different class: {diagnosis}"
    );
}

/// A `-prefixed` path publishes a next_step that, when executed without a shell, reaches
/// the same config diagnosis as a plain path. Exit `2` alone is not enough: clap refusal
/// is also `2` (#2216).
#[test]
fn dash_prefixed_config_recovery_reaches_the_diagnosis() {
    let cases = ["-weird.yaml", "--config", "-h"];
    for config_name in cases {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(config_name), "not: [broken\n")
            .expect("write malformed dash-prefixed config");

        let failed = assay(
            dir.path(),
            &[
                "doctor",
                "--format",
                "json",
                &format!("--config={config_name}"),
            ],
            &format!("produce recovery for {config_name}"),
        );
        assert_eq!(
            failed.status.code(),
            Some(2),
            "{config_name} must be a config error: {}",
            String::from_utf8_lossy(&failed.stderr)
        );
        let summary: serde_json::Value =
            serde_json::from_slice(&failed.stdout).expect("failure stdout must be JSON");
        assert_eq!(summary["reason_code"], "E_CFG_PARSE");

        let recovery = recovery_argv(&summary);
        let recovered = assay(
            dir.path(),
            &recovery[1..],
            &format!("execute recovery for {config_name}"),
        );
        assert_recovery_reached_cfg_parse_diagnosis(
            &recovered,
            &format!("doctor recovery for {config_name}"),
        );
    }
}

/// `assay run` and `assay doctor` read one registry string, so a `-prefixed` path
/// must recover through the same published argv on both commands (#2216).
#[test]
fn run_and_doctor_publish_the_same_executable_recovery_for_a_dash_prefixed_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_name = "-weird.yaml";
    std::fs::write(dir.path().join(config_name), "not: [broken\n")
        .expect("write malformed dash-prefixed config");
    let fused = format!("--config={config_name}");

    let doctor = assay(
        dir.path(),
        &["doctor", "--format", "json", &fused],
        "doctor dash-prefixed config",
    );
    let run = assay(
        dir.path(),
        &["run", "--format", "json", &fused],
        "run dash-prefixed config",
    );
    let doctor_json: serde_json::Value =
        serde_json::from_slice(&doctor.stdout).expect("doctor failure is JSON");
    let run_json: serde_json::Value =
        serde_json::from_slice(&run.stdout).expect("run failure is JSON");
    assert_eq!(doctor_json["reason_code"], "E_CFG_PARSE");
    assert_eq!(run_json["reason_code"], "E_CFG_PARSE");
    assert_eq!(
        doctor_json["next_step"], run_json["next_step"],
        "one registry string: run and doctor must publish the same recovery"
    );

    let recovery = recovery_argv(&run_json);
    let recovered = assay(dir.path(), &recovery[1..], "execute shared recovery");
    assert_recovery_reached_cfg_parse_diagnosis(&recovered, "shared run/doctor recovery");
}

/// A JSON consumer that follows the published recovery must get a JSON diagnosis
/// back. The format is decided in the registry, so text and JSON publish the
/// same argv (#2216).
#[test]
fn json_and_text_publish_the_same_recovery_and_executing_it_stays_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_name = "bad.yaml";
    std::fs::write(dir.path().join(config_name), "not: [broken\n").expect("write malformed config");

    let json_failed = assay(
        dir.path(),
        &["run", "--format", "json", "--config", config_name],
        "json-format config failure",
    );
    let text_failed = assay(
        dir.path(),
        &["run", "--format", "text", "--config", config_name],
        "text-format config failure",
    );
    let json_doc: serde_json::Value =
        serde_json::from_slice(&json_failed.stdout).expect("json failure is JSON");
    let summary_path = dir.path().join("summary.json");
    let text_summary: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&summary_path).expect("text run still writes summary.json"),
    )
    .expect("summary.json parses");
    assert_eq!(
        json_doc["next_step"], text_summary["next_step"],
        "text and JSON must publish the same registry recovery, not two remediations"
    );
    assert!(
        text_failed.stdout.is_empty(),
        "text format keeps the diagnosis off stdout; this test reads summary.json"
    );

    let recovery = recovery_argv(&json_doc);
    let recovered = assay(
        dir.path(),
        &recovery[1..],
        "execute format-neutral recovery",
    );
    assert_recovery_reached_cfg_parse_diagnosis(&recovered, "format-parity recovery");
}
