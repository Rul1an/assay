//! `assay doctor --format json` must say whether the config was checked at all (#2160).
//!
//! The published reading instruction tells an agent to read `data_diagnostics[].severity` rather
//! than the exit code. That instruction is only safe if the machine channel can express "the
//! config was never read": before this contract existed, an unconfigured directory produced exit
//! `0` with `data_diagnostics` absent and no skip token anywhere in the document, while the text
//! channel printed `Policy Check: SKIPPED`. A consumer that looked for error severities found none
//! and concluded the config was fine. `AGENTS.md` names that move directly: never turn absence of
//! evidence into a clean result.
//!
//! So the three states the command can be in — checked, skipped, failed — are each represented by
//! one always-present key, and the reading instruction is pinned to the field name it tells a
//! consumer to read, so the two cannot drift apart.

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use serde_json::Value;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

fn workspace_root() -> &'static Path {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("assay-cli must live two components below the workspace root");
    assert!(
        root.join("Cargo.toml").is_file(),
        "workspace root does not contain Cargo.toml: {}",
        root.display()
    );
    root
}

/// Drives the binary in `cwd` and returns its exit code plus parsed stdout.
fn doctor_json<S: AsRef<OsStr>>(cwd: &Path, extra: &[S]) -> (i32, Value) {
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
    command
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .args(["doctor", "--format", "json"])
        .args(extra);
    let output = run_bounded(
        command,
        b"",
        GOLDEN_PATH_LIMITS,
        "assay doctor --format json",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let code = output
        .status
        .code()
        .expect("assay process terminated without an exit code");
    let document: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "doctor stdout is not JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (code, document)
}

fn doctor_text<S: AsRef<OsStr>>(cwd: &Path, extra: &[S]) -> (i32, String, String) {
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
    command
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .args(["doctor", "--format", "text"])
        .args(extra);
    let output = run_bounded(
        command,
        b"",
        GOLDEN_PATH_LIMITS,
        "assay doctor --format text",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let code = output
        .status
        .code()
        .expect("assay process terminated without an exit code");
    (
        code,
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// github-token shape from `render_safety/rules.rs` (`ghp_` + 36 alphanumerics).
/// Space-separated padding stays outside that rule so the extra bytes survive
/// redaction and force the visible truncation marker.
fn long_secret_yaml() -> (String, String) {
    let token = format!("ghp_{}", "A".repeat(36));
    let yaml = format!("version: \"{token} {}\"\n", "x".repeat(300));
    (token, yaml)
}

fn yaml_location_mark(yaml: &str) -> String {
    let err = serde_yaml::from_str::<assay_core::model::EvalConfig>(yaml)
        .expect_err("fixture must fail YAML parse");
    let loc = err
        .location()
        .expect("serde_yaml must report a location for this fixture");
    format!("at line {} column {}", loc.line(), loc.column())
}

fn init_project(cwd: &Path) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    command.current_dir(cwd).env("NO_COLOR", "1").args([
        "init",
        "--preset",
        "dev",
        "--hello-trace",
    ]);
    let output = run_bounded(command, b"", GOLDEN_PATH_LIMITS, "assay init")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        output.status.code(),
        Some(0),
        "init must seed a loadable config: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn an_unchecked_config_is_not_reported_as_a_clean_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, document) = doctor_json::<&str>(dir.path(), &[]);

    assert_eq!(code, 0, "the skipped check keeps its exit class");
    assert_eq!(
        document["config_check"]["status"],
        "skipped",
        "no config was read, so the JSON channel must say so instead of leaving a consumer to \
         read an absent data_diagnostics as an absence of problems; document:\n{}",
        serde_json::to_string_pretty(&document).expect("re-serialize")
    );
    assert!(
        document["config_check"]["reason"]
            .as_str()
            .is_some_and(|reason| !reason.trim().is_empty()),
        "a skipped check must name why it was skipped"
    );
    assert!(
        document.get("data_diagnostics").is_none(),
        "nothing was checked, so there are no diagnostics to publish"
    );
}

#[test]
fn a_checked_config_is_distinguishable_from_a_skipped_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_project(dir.path());
    let (code, document) = doctor_json::<&str>(dir.path(), &[]);

    assert_eq!(code, 0, "a loadable config with no findings stays exit 0");
    assert_eq!(
        document["config_check"]["status"],
        "checked",
        "a config that was read must not carry the same marker as one that was not; document:\n{}",
        serde_json::to_string_pretty(&document).expect("re-serialize")
    );
    assert!(
        document["data_diagnostics"].is_array(),
        "a checked config publishes the diagnostics array the reading instruction names"
    );
}

#[test]
fn a_config_that_will_not_load_is_neither_checked_nor_skipped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("broken.yaml");
    std::fs::write(&config, "version: [\n").expect("write malformed config");
    let (code, document) = doctor_json(
        dir.path(),
        &["--config", config.to_str().expect("UTF-8 path")],
    );

    assert_eq!(
        code, 2,
        "an unloadable explicit config stays the config class"
    );
    assert_eq!(
        document["config_check"]["status"],
        "failed",
        "a failed load is its own state; document:\n{}",
        serde_json::to_string_pretty(&document).expect("re-serialize")
    );
    assert!(
        document["config_check"].get("reason").is_none(),
        "failed config_check publishes status only; detail lives on config_error.message; document:\n{}",
        serde_json::to_string_pretty(&document).expect("re-serialize")
    );
    let message = document["config_error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("config_error.message must be a string: {document}"));
    assert!(
        message.contains("failed to parse YAML"),
        "concise malformed YAML must stay diagnosed: {message}"
    );
    let mark = yaml_location_mark("version: [\n");
    assert!(
        message.contains(&mark),
        "short diagnosis must keep the location mark: {message}"
    );
    assert_eq!(
        message.matches(&mark).count(),
        1,
        "short diagnosis must not duplicate the location mark: {message}"
    );
    assert_eq!(
        document["reason_code"], "E_CFG_PARSE",
        "unloadable explicit config stays the parse class: {document}"
    );
    let next = document["next_step"]
        .as_str()
        .unwrap_or_else(|| panic!("next_step must be a string: {document}"));
    assert!(
        !next.trim().is_empty(),
        "parse recovery must name a next step"
    );
}

/// A parse error that quotes a long secret-shaped scalar must reach doctor
/// redacted and bounded. This is the parse-Display fold, not a read ceiling,
/// and not a claim that every YAML error echoes input.
#[test]
fn a_long_secret_parse_error_is_redacted_bounded_and_not_duplicated() {
    let (token, yaml) = long_secret_yaml();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("secret.yaml");
    std::fs::write(&config, &yaml).expect("write long-secret config");
    let config_arg = config.to_str().expect("UTF-8 path");

    let (code, document) = doctor_json(dir.path(), &["--config", config_arg]);
    assert_eq!(code, 2, "unloadable explicit config stays the config class");
    assert_eq!(
        document["config_check"]["status"],
        "failed",
        "a failed load is its own state; document:\n{}",
        serde_json::to_string_pretty(&document).expect("re-serialize")
    );
    assert!(
        document["config_check"].get("reason").is_none(),
        "failed config_check must not restate config_error.message; document:\n{}",
        serde_json::to_string_pretty(&document).expect("re-serialize")
    );
    assert_eq!(
        document["reason_code"], "E_CFG_PARSE",
        "long-secret parse stays the parse class: {document}"
    );
    let next = document["next_step"]
        .as_str()
        .unwrap_or_else(|| panic!("next_step must be a string: {document}"));
    assert!(
        !next.trim().is_empty(),
        "parse recovery must name a next step"
    );

    let message = document["config_error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("config_error.message must be a string: {document}"));
    assert!(
        message.contains("failed to parse YAML"),
        "parse failures must stay diagnosed: {message}"
    );
    assert!(
        !message.contains(&token),
        "raw credential must not appear in config_error.message: {message}"
    );
    assert!(
        message.contains("<redacted:"),
        "redaction placeholder must be visible: {message}"
    );
    assert!(
        message.contains("(truncated)"),
        "truncation must be visible: {message}"
    );
    let mark = yaml_location_mark(&yaml);
    assert!(
        message.contains(&mark),
        "location must survive the same total budget: {message}"
    );
    assert_eq!(
        message.matches(&mark).count(),
        1,
        "location mark must appear once: {message}"
    );
    let trunc_at = message
        .find("(truncated)")
        .expect("truncation marker already asserted");
    let mark_at = message.find(&mark).expect("location mark already asserted");
    assert!(
        mark_at > trunc_at,
        "location is reserved after the bound excerpt, not swallowed by truncate: {message}"
    );
    assert!(
        message.chars().count() <= assay_core::render_safety::MAX_RENDER_FIELD + 80,
        "config_error.message is prefix + bounded excerpt ({} chars): {message}",
        message.chars().count()
    );

    let blob = serde_json::to_string(&document).expect("serialize doctor json");
    assert!(
        !blob.contains(&token),
        "raw credential must not appear anywhere in the doctor JSON document"
    );

    let (text_code, stdout, stderr) = doctor_text(dir.path(), &["--config", config_arg]);
    assert_eq!(text_code, 2, "text channel stays the config class");
    let text = format!("{stdout}\n{stderr}");
    assert!(
        text.contains("Config Status: FAILED") || text.contains("failed to parse YAML"),
        "text channel must stay diagnosed:\n{text}"
    );
    assert!(
        !text.contains(&token),
        "raw credential must not appear on the text channel:\n{text}"
    );
    assert!(
        text.contains("<redacted:"),
        "redaction placeholder must be visible on the text channel:\n{text}"
    );
    assert!(
        text.contains("(truncated)"),
        "truncation must be visible on the text channel:\n{text}"
    );
}

/// The instruction and the field are one rule answered in two places, so pin them together.
///
/// A reading instruction that names a field the binary does not emit is worse than no instruction:
/// it reads as a checkable discriminator and is not one. The generated contract is the published
/// form of that instruction, so the assertion is against the artifact a consumer actually reads.
#[test]
fn the_published_reading_instruction_names_the_field_the_binary_emits() {
    let path = workspace_root().join("docs/generated/agent-golden-path.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let contract: Value = serde_json::from_str(&raw).expect("golden-path contract is JSON");
    let non_claims = contract["non_claims"]
        .as_array()
        .expect("contract non_claims array");

    let dir = tempfile::tempdir().expect("tempdir");
    let (_, document) = doctor_json::<&str>(dir.path(), &[]);
    let status = document["config_check"]["status"]
        .as_str()
        .expect("doctor must publish config_check.status");

    let names_the_field = non_claims
        .iter()
        .filter_map(Value::as_str)
        .any(|claim| claim.contains("config_check") && claim.contains(status));
    assert!(
        names_the_field,
        "no published non-claim tells a consumer to read config_check for the {status:?} state, \
         so the instruction still resolves an unchecked config into a clean one"
    );
}
