//! Binary contract: an explicit `--config` is classified from the read I/O cause (#2206).
//!
//! `assay run` and `assay doctor` must publish the same reason identity for the same
//! fixture. Absence is `E_MISSING_CONFIG` only when the config read itself returned
//! `NotFound`. A second `exists()`/`try_exists()` probe is not the classifier: it
//! answers false when metadata is inaccessible, which is how a real file under an
//! EACCES parent was published as missing.

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, ProcessLimits};
use serde_json::Value;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

const LIMITS: ProcessLimits = ProcessLimits::new(Duration::from_secs(5), 64 * 1024, 64 * 1024);
const INIT_RECOVERY: &str = "Run: assay init to create a config file";
const MALFORMED_YAML: &str = "version: [\n";

fn assay_command(cwd: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    for (name, _) in std::env::vars_os() {
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

fn assay<T: AsRef<OsStr>>(cwd: &Path, args: &[T], context: &str) -> Output {
    let mut command = assay_command(cwd);
    command.args(args);
    run_bounded(command, b"", LIMITS, context).unwrap_or_else(|error| panic!("{error}"))
}

fn json_document(output: &Output, context: &str) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{context}: stdout must be JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn run_text_summary(cwd: &Path) -> Value {
    let path = cwd.join("summary.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("text run must write {}: {error}", path.display()));
    serde_json::from_str(&raw).expect("summary.json parses")
}

fn assert_class(document: &Value, reason: &str, context: &str) {
    assert_eq!(
        document["reason_code"], reason,
        "{context}: reason_code must be {reason}: {document}"
    );
    if document.get("config_error").is_some() {
        assert_eq!(
            document["config_error"]["code"], reason,
            "{context}: config_error.code must match the published reason: {document}"
        );
    }
}

fn assert_missing_recovery(document: &Value, context: &str) {
    assert_eq!(
        document["next_step"], INIT_RECOVERY,
        "{context}: a proven-absent config recovers with assay init, not a doctor self-loop: {document}"
    );
    let next = document["next_step"].as_str().unwrap_or("");
    assert!(
        !next.contains("doctor"),
        "{context}: missing-config recovery must not re-run doctor: {next}"
    );
}

fn assert_parse_recovery(document: &Value, config: &str, context: &str) {
    let next = document["next_step"]
        .as_str()
        .unwrap_or_else(|| panic!("{context}: next_step must be a string: {document}"));
    let encoded = next
        .strip_prefix("Run argv: ")
        .unwrap_or_else(|| panic!("{context}: parse recovery must publish JSON argv: {next}"));
    let argv: Vec<String> =
        serde_json::from_str(encoded).unwrap_or_else(|error| panic!("{context}: {error}: {next}"));
    assert_eq!(
        argv,
        vec![
            "assay".to_string(),
            "doctor".to_string(),
            format!("--config={config}"),
            "--format".to_string(),
            "json".to_string(),
        ],
        "{context}: parse recovery must stay the fused #2370 doctor argv"
    );
}

fn drive_json_pair(cwd: &Path, config: &str, context: &str) -> (Value, Value) {
    let run = assay(
        cwd,
        &["run", "--format", "json", "--config", config],
        &format!("{context} run json"),
    );
    let doctor = assay(
        cwd,
        &["doctor", "--format", "json", "--config", config],
        &format!("{context} doctor json"),
    );
    assert_eq!(
        run.status.code(),
        Some(2),
        "{context} run json exit: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        doctor.status.code(),
        Some(2),
        "{context} doctor json exit: {}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    (
        json_document(&run, &format!("{context} run json")),
        json_document(&doctor, &format!("{context} doctor json")),
    )
}

fn drive_text_pair(cwd: &Path, config: &str, context: &str) -> (Value, Output) {
    let run = assay(
        cwd,
        &["run", "--format", "text", "--config", config],
        &format!("{context} run text"),
    );
    let doctor = assay(
        cwd,
        &["doctor", "--format", "text", "--config", config],
        &format!("{context} doctor text"),
    );
    assert_eq!(
        run.status.code(),
        Some(2),
        "{context} run text exit: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        doctor.status.code(),
        Some(2),
        "{context} doctor text exit: {}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let text = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        text.contains("Config Status: FAILED"),
        "{context} doctor text must name the failed load:\n{text}"
    );
    (run_text_summary(cwd), doctor)
}

fn assert_run_doctor_class_parity(
    cwd: &Path,
    config: &str,
    reason: &str,
    context: &str,
) -> (Value, Value) {
    let (run_json, doctor_json) = drive_json_pair(cwd, config, context);
    assert_class(&run_json, reason, &format!("{context} run json"));
    assert_class(&doctor_json, reason, &format!("{context} doctor json"));
    assert_eq!(
        run_json["reason_code"], doctor_json["reason_code"],
        "{context}: run and doctor must publish one class for one fixture"
    );

    let (run_text, _) = drive_text_pair(cwd, config, context);
    assert_class(&run_text, reason, &format!("{context} run text"));
    assert_eq!(
        run_text["reason_code"], run_json["reason_code"],
        "{context}: run JSON/text must agree on class"
    );
    (run_json, doctor_json)
}

#[test]
fn r_miss_absent_explicit_config_is_missing_on_run_and_doctor() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = "missing.yaml";
    assert!(!dir.path().join(config).exists(), "fixture must be absent");

    let (run_json, doctor_json) =
        assert_run_doctor_class_parity(dir.path(), config, "E_MISSING_CONFIG", "R-miss");
    assert_missing_recovery(&run_json, "R-miss run");
    assert_missing_recovery(&doctor_json, "R-miss doctor");
}

#[test]
fn r_yaml_malformed_existing_config_stays_parse_with_fused_recovery() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = "bad.yaml";
    std::fs::write(dir.path().join(config), MALFORMED_YAML).expect("write malformed yaml");

    let (run_json, doctor_json) =
        assert_run_doctor_class_parity(dir.path(), config, "E_CFG_PARSE", "R-yaml");
    assert_parse_recovery(&run_json, config, "R-yaml run");
    assert_parse_recovery(&doctor_json, config, "R-yaml doctor");
}

#[test]
fn r_dir_directory_as_config_is_unloadable_not_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = "config-dir";
    std::fs::create_dir(dir.path().join(config)).expect("create directory fixture");

    let (run_json, doctor_json) =
        assert_run_doctor_class_parity(dir.path(), config, "E_CFG_PARSE", "R-dir");
    assert_parse_recovery(&run_json, config, "R-dir run");
    assert_parse_recovery(&doctor_json, config, "R-dir doctor");
}

/// Existing file under an EACCES parent. Unix non-root only: as root the fixture
/// is readable and the permission signal is gone. Includes macOS (darwin) as a
/// unix host running the same fixture. No Windows claim.
#[cfg(unix)]
#[test]
fn r_eacces_existing_file_under_inaccessible_parent_is_never_missing() {
    if effective_uid_is_root() {
        eprintln!("SKIP R-eacces: fixture is invalid as root (geteuid==0)");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let parent = dir.path().join("denied");
    std::fs::create_dir(&parent).expect("create denied parent");
    let config_name = "denied/secret.yaml";
    let config_path = dir.path().join(config_name);
    std::fs::write(&config_path, MALFORMED_YAML).expect("write existing config");
    assert!(config_path.is_file(), "R-eacces fixture must exist");

    // Do not call exists()/is_file() after the lock: those probes are the lie
    // this contract forbids. The file was created above; the parent search is
    // what the read will now fail on.
    let _restore = UnixModeGuard::lock(&parent, 0o000);

    let (run_json, doctor_json) =
        assert_run_doctor_class_parity(dir.path(), config_name, "E_CFG_PARSE", "R-eacces");
    assert_ne!(
        run_json["reason_code"], "E_MISSING_CONFIG",
        "R-eacces run must not treat an existing unreadable file as absent: {run_json}"
    );
    assert_ne!(
        doctor_json["reason_code"], "E_MISSING_CONFIG",
        "R-eacces doctor must not treat an existing unreadable file as absent: {doctor_json}"
    );
    assert_parse_recovery(&run_json, config_name, "R-eacces run");
    assert_parse_recovery(&doctor_json, config_name, "R-eacces doctor");
}

#[cfg(unix)]
fn effective_uid_is_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.trim().parse::<u32>().ok())
        == Some(0)
}

#[cfg(unix)]
struct UnixModeGuard {
    path: PathBuf,
    original: u32,
}

#[cfg(unix)]
impl UnixModeGuard {
    fn lock(path: &Path, mode: u32) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let original = std::fs::metadata(path)
            .expect("stat parent")
            .permissions()
            .mode();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .expect("chmod parent");
        Self {
            path: path.to_path_buf(),
            original,
        }
    }
}

#[cfg(unix)]
impl Drop for UnixModeGuard {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ =
            std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(self.original));
    }
}
