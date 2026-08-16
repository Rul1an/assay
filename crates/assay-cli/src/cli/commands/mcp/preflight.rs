//! Named-phase PATH classifier for `assay-mcp-server` (#2195).
//!
//! This command does not start an MCP session for a host and does not change
//! `assay-mcp-server` startup rules (#2408 / PR #2409 own that tree).

use serde_json::json;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::cli::args::{PreflightArgs, PreflightFormat};
use crate::cli::bounded_child::{run_bounded, BoundedChildError};
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_SUCCESS};

const SCHEMA: &str = "assay.mcp_preflight.v0";
const SERVER_COMMAND: &str = "assay-mcp-server";
const PROBE_DEADLINE: Duration = Duration::from_secs(2);
const PROBE_OUTPUT_CAP: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Missing,
    Unstartable,
    WrongVersion,
    InvalidRoot,
    StartupRefused,
    StartupTimeout,
    Ready,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Unstartable => "unstartable",
            Self::WrongVersion => "wrong_version",
            Self::InvalidRoot => "invalid_root",
            Self::StartupRefused => "startup_refused",
            Self::StartupTimeout => "startup_timeout",
            Self::Ready => "ready",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Report {
    pub phase: Phase,
    pub message: String,
    pub next_step: String,
    pub expected_version: String,
    pub actual_version: Option<String>,
    pub policy_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeFailure {
    NotFound,
    OtherSpawn(io::ErrorKind),
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProbeOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
}

trait Probe {
    fn identity(&self) -> Result<ProbeOutput, ProbeFailure>;
    fn startup(&self, policy_root: &Path) -> Result<ProbeOutput, ProbeFailure>;
}

fn classify(probe: &impl Probe, policy_root: &Path, expected_version: &str) -> Report {
    let actual_version = match probe.identity() {
        Err(ProbeFailure::NotFound) => {
            return finish(Phase::Missing, policy_root, expected_version, None);
        }
        Err(ProbeFailure::OtherSpawn(_) | ProbeFailure::Timeout) => {
            return finish(Phase::Unstartable, policy_root, expected_version, None);
        }
        Ok(output) if output.exit_code != 0 => {
            return finish(Phase::Unstartable, policy_root, expected_version, None);
        }
        Ok(output) => match parse_identity_version(&output.stdout) {
            Some(actual) if actual == expected_version => actual,
            Some(actual) => {
                return finish(
                    Phase::WrongVersion,
                    policy_root,
                    expected_version,
                    Some(actual),
                );
            }
            None => {
                return finish(Phase::Unstartable, policy_root, expected_version, None);
            }
        },
    };

    if !policy_root.exists() || !policy_root.is_dir() {
        return finish(
            Phase::InvalidRoot,
            policy_root,
            expected_version,
            Some(actual_version),
        );
    }

    match probe.startup(policy_root) {
        Ok(output) if output.exit_code == 0 => finish(
            Phase::Ready,
            policy_root,
            expected_version,
            Some(actual_version),
        ),
        Err(ProbeFailure::Timeout) => finish(
            Phase::StartupTimeout,
            policy_root,
            expected_version,
            Some(actual_version),
        ),
        Ok(_) | Err(_) => finish(
            Phase::StartupRefused,
            policy_root,
            expected_version,
            Some(actual_version),
        ),
    }
}

fn finish(
    phase: Phase,
    policy_root: &Path,
    expected_version: &str,
    actual_version: Option<String>,
) -> Report {
    let (message, next_step) = diagnosis(phase, expected_version, actual_version.as_deref());
    Report {
        phase,
        message,
        next_step,
        expected_version: expected_version.to_string(),
        actual_version,
        policy_root: policy_root.to_path_buf(),
    }
}

fn diagnosis(phase: Phase, expected: &str, actual: Option<&str>) -> (String, String) {
    match phase {
        Phase::Missing => (
            "assay-mcp-server was not found on PATH".to_string(),
            format!(
                "Install assay-mcp-server on PATH ({}), then re-run assay mcp preflight.",
                install_matching_server(expected)
            ),
        ),
        Phase::Unstartable => (
            "assay-mcp-server on PATH could not be started or did not report a version".to_string(),
            "Replace the assay-mcp-server on PATH with a working binary from the same Assay release."
                .to_string(),
        ),
        Phase::WrongVersion => (
            format!(
                "assay-mcp-server reported {} but this CLI expects {expected}",
                actual.unwrap_or("an unexpected version")
            ),
            format!(
                "Install assay-mcp-server {expected} on PATH ({}).",
                install_matching_server(expected)
            ),
        ),
        Phase::InvalidRoot => (
            "policy root must be an existing directory".to_string(),
            "Pass --policy-root to an existing directory.".to_string(),
        ),
        Phase::StartupRefused => (
            "assay-mcp-server refused to start with this policy root".to_string(),
            "Unset ASSAY_AUTH_* and confirm the server can start with this --policy-root."
                .to_string(),
        ),
        Phase::StartupTimeout => (
            "assay-mcp-server did not exit before the preflight deadline".to_string(),
            "Retry after checking that assay-mcp-server is not blocked at startup.".to_string(),
        ),
        Phase::Ready => (
            "assay-mcp-server on PATH matches this CLI and accepted the policy root".to_string(),
            String::new(),
        ),
    }
}

fn install_matching_server(expected: &str) -> String {
    format!("cargo install assay-mcp-server --version {expected} --locked")
}

fn parse_identity_version(stdout: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(stdout).ok()?;
    let line = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text);
    if line.contains(['\n', '\r']) {
        return None;
    }
    let version = line.strip_prefix(SERVER_COMMAND)?.strip_prefix(' ')?;
    if version.contains([' ', '\t']) {
        return None;
    }
    semver::Version::parse(version).ok()?;
    // Keep the wire token. Co-release compares this string to CARGO_PKG_VERSION.
    Some(version.to_string())
}

fn render_json(report: &Report) -> String {
    let mut document = json!({
        "schema": SCHEMA,
        "phase": report.phase.as_str(),
        "message": report.message,
        "next_step": report.next_step,
        "expected_version": report.expected_version,
        "policy_root": report.policy_root.to_string_lossy(),
    });
    if let Some(actual) = &report.actual_version {
        document
            .as_object_mut()
            .expect("object")
            .insert("actual_version".to_string(), json!(actual));
    }
    serde_json::to_string_pretty(&document).expect("preflight report is JSON")
}

fn render_terminal(report: &Report) -> String {
    if report.next_step.is_empty() {
        format!("{}: {}", report.phase.as_str(), report.message)
    } else {
        format!(
            "{}: {}\n{}",
            report.phase.as_str(),
            report.message,
            report.next_step
        )
    }
}

fn emit(report: &Report, format: PreflightFormat) {
    match format {
        PreflightFormat::Json => println!("{}", render_json(report)),
        PreflightFormat::Terminal => println!("{}", render_terminal(report)),
    }
}

fn exit_code(phase: Phase) -> i32 {
    if phase == Phase::Ready {
        EXIT_SUCCESS
    } else {
        EXIT_CONFIG_ERROR
    }
}

struct PathProbe;

impl Probe for PathProbe {
    fn identity(&self) -> Result<ProbeOutput, ProbeFailure> {
        bounded_probe(Command::new(SERVER_COMMAND).arg("--version"))
    }

    fn startup(&self, policy_root: &Path) -> Result<ProbeOutput, ProbeFailure> {
        bounded_probe(
            Command::new(SERVER_COMMAND)
                .arg("--policy-root")
                .arg(policy_root),
        )
    }
}

fn bounded_probe(command: &mut Command) -> Result<ProbeOutput, ProbeFailure> {
    match run_bounded(command, PROBE_DEADLINE, PROBE_OUTPUT_CAP) {
        Ok(output) => Ok(ProbeOutput {
            exit_code: output.exit_code,
            stdout: output.stdout,
        }),
        Err(BoundedChildError::NotFound) => Err(ProbeFailure::NotFound),
        Err(BoundedChildError::Timeout) => Err(ProbeFailure::Timeout),
        Err(BoundedChildError::Spawn(kind)) => Err(ProbeFailure::OtherSpawn(kind)),
    }
}

pub(super) fn run(args: PreflightArgs) -> anyhow::Result<i32> {
    let report = classify(&PathProbe, &args.policy_root, env!("CARGO_PKG_VERSION"));
    emit(&report, args.format);
    Ok(exit_code(report.phase))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::cell::Cell;
    use std::fs;
    use std::io::ErrorKind;

    struct FakeProbe {
        identity: Result<ProbeOutput, ProbeFailure>,
        startup: Result<ProbeOutput, ProbeFailure>,
        startups: Cell<u32>,
    }

    impl FakeProbe {
        fn identity_ok(stdout: &str) -> Self {
            Self {
                identity: Ok(ProbeOutput {
                    exit_code: 0,
                    stdout: stdout.as_bytes().to_vec(),
                }),
                startup: Ok(ProbeOutput {
                    exit_code: 0,
                    stdout: Vec::new(),
                }),
                startups: Cell::new(0),
            }
        }
    }

    impl Probe for FakeProbe {
        fn identity(&self) -> Result<ProbeOutput, ProbeFailure> {
            self.identity.clone()
        }

        fn startup(&self, _policy_root: &Path) -> Result<ProbeOutput, ProbeFailure> {
            self.startups.set(self.startups.get() + 1);
            self.startup.clone()
        }
    }

    fn expected() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn matching_version_line() -> String {
        format!("{SERVER_COMMAND} {}", expected())
    }

    fn dir_root() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn not_found_is_missing_other_spawn_is_unstartable() {
        let dir = dir_root();
        let missing = FakeProbe {
            identity: Err(ProbeFailure::NotFound),
            startup: Ok(ProbeOutput {
                exit_code: 0,
                stdout: Vec::new(),
            }),
            startups: Cell::new(0),
        };
        let report = classify(&missing, dir.path(), expected());
        assert_eq!(report.phase, Phase::Missing);
        assert_eq!(missing.startups.get(), 0);
        assert_eq!(report.expected_version, expected());
        assert!(report.next_step.contains("PATH"));

        let denied = FakeProbe {
            identity: Err(ProbeFailure::OtherSpawn(ErrorKind::PermissionDenied)),
            startup: Ok(ProbeOutput {
                exit_code: 0,
                stdout: Vec::new(),
            }),
            startups: Cell::new(0),
        };
        let report = classify(&denied, dir.path(), expected());
        assert_eq!(
            report.phase,
            Phase::Unstartable,
            "folding every spawn error into missing hides PermissionDenied"
        );
        assert_eq!(denied.startups.get(), 0);
    }

    #[test]
    fn parseable_mismatch_is_wrong_version() {
        let dir = dir_root();
        let probe = FakeProbe::identity_ok(&format!("{SERVER_COMMAND} 0.0.1"));
        let report = classify(&probe, dir.path(), expected());
        assert_eq!(report.phase, Phase::WrongVersion);
        assert_eq!(report.actual_version.as_deref(), Some("0.0.1"));
        assert_eq!(report.expected_version, expected());
        assert_ne!(report.expected_version, "0.0.1");
        assert!(
            report.next_step.contains(expected()) && report.next_step.contains("--version"),
            "WrongVersion must pin cargo install to this CLI, not latest: {}",
            report.next_step
        );
        assert_eq!(probe.startups.get(), 0);
    }

    #[test]
    fn install_matching_server_pins_expected_and_locked() {
        let command = install_matching_server(expected());
        assert_eq!(
            command,
            format!(
                "cargo install assay-mcp-server --version {} --locked",
                expected()
            )
        );
    }

    #[test]
    fn unparsable_or_bad_prefix_is_unstartable_not_wrong_version() {
        let dir = dir_root();
        let trailing_junk = format!("{SERVER_COMMAND} {} junk", expected());
        let extra_line = format!("{SERVER_COMMAND} {}\nextra", expected());
        for stdout in [
            "not-a-server 1.0.0",
            "assay-mcp-server",
            "assay-mcp-server not-a-version",
            "assay-mcp-server 1foo",
            "assay-mcp-server 5.2",
            "assay-mcp-server 5.2.0-",
            "LEAK_FROM_CHILD",
            trailing_junk.as_str(),
            extra_line.as_str(),
        ] {
            let probe = FakeProbe::identity_ok(stdout);
            let report = classify(&probe, dir.path(), expected());
            assert_eq!(
                report.phase,
                Phase::Unstartable,
                "parse failure must not become wrong_version; stdout={stdout:?}"
            );
            assert!(report.actual_version.is_none());
            assert_eq!(probe.startups.get(), 0);
        }
    }

    #[test]
    fn identity_nonzero_or_timeout_is_unstartable() {
        let dir = dir_root();
        let nonzero = FakeProbe {
            identity: Ok(ProbeOutput {
                exit_code: 3,
                stdout: matching_version_line().into_bytes(),
            }),
            startup: Ok(ProbeOutput {
                exit_code: 0,
                stdout: Vec::new(),
            }),
            startups: Cell::new(0),
        };
        assert_eq!(
            classify(&nonzero, dir.path(), expected()).phase,
            Phase::Unstartable
        );

        let hung = FakeProbe {
            identity: Err(ProbeFailure::Timeout),
            startup: Ok(ProbeOutput {
                exit_code: 0,
                stdout: Vec::new(),
            }),
            startups: Cell::new(0),
        };
        assert_eq!(
            classify(&hung, dir.path(), expected()).phase,
            Phase::Unstartable,
            "omitting the identity timeout folds a hang into a later phase"
        );
        assert_eq!(hung.startups.get(), 0);
    }

    #[test]
    fn missing_path_and_file_root_are_invalid_root_without_startup() {
        let probe = FakeProbe::identity_ok(&matching_version_line());
        let missing = PathBuf::from("2195-preflight-root-does-not-exist");
        let report = classify(&probe, &missing, expected());
        assert_eq!(report.phase, Phase::InvalidRoot);
        assert_eq!(probe.startups.get(), 0);

        let file = tempfile::NamedTempFile::new().expect("file root");
        let report = classify(&probe, file.path(), expected());
        assert_eq!(
            report.phase,
            Phase::InvalidRoot,
            "removing is_dir would start the server for a regular file"
        );
        assert_eq!(probe.startups.get(), 0);
    }

    #[test]
    fn startup_nonzero_is_refused_and_timeout_is_startup_timeout() {
        let dir = dir_root();
        let refused = FakeProbe {
            identity: Ok(ProbeOutput {
                exit_code: 0,
                stdout: matching_version_line().into_bytes(),
            }),
            startup: Ok(ProbeOutput {
                exit_code: 1,
                stdout: b"ignored child bytes".to_vec(),
            }),
            startups: Cell::new(0),
        };
        let report = classify(&refused, dir.path(), expected());
        assert_eq!(report.phase, Phase::StartupRefused);
        assert_eq!(refused.startups.get(), 1);

        let hung = FakeProbe {
            identity: Ok(ProbeOutput {
                exit_code: 0,
                stdout: matching_version_line().into_bytes(),
            }),
            startup: Err(ProbeFailure::Timeout),
            startups: Cell::new(0),
        };
        let report = classify(&hung, dir.path(), expected());
        assert_eq!(
            report.phase,
            Phase::StartupTimeout,
            "omitting the startup timeout cannot distinguish a hang from ready"
        );
        assert_eq!(hung.startups.get(), 1);
    }

    #[test]
    fn matching_version_and_directory_and_zero_exit_is_ready() {
        let dir = dir_root();
        let probe = FakeProbe::identity_ok(&matching_version_line());
        let report = classify(&probe, dir.path(), expected());
        assert_eq!(report.phase, Phase::Ready);
        assert_eq!(report.actual_version.as_deref(), Some(expected()));
        assert_eq!(report.expected_version, expected());
        assert_eq!(probe.startups.get(), 1);
        assert_eq!(exit_code(report.phase), 0);
    }

    #[test]
    fn json_is_one_object_and_drops_child_bytes() {
        let dir = dir_root();
        let probe = FakeProbe {
            identity: Ok(ProbeOutput {
                exit_code: 0,
                stdout: matching_version_line().into_bytes(),
            }),
            startup: Ok(ProbeOutput {
                exit_code: 1,
                stdout: b"LEAK_FROM_CHILD".to_vec(),
            }),
            startups: Cell::new(0),
        };
        let report = classify(&probe, dir.path(), expected());
        let rendered = render_json(&report);
        let document: Value = serde_json::from_str(&rendered).expect("one JSON object");
        assert_eq!(document["schema"], SCHEMA);
        assert_eq!(document["phase"], "startup_refused");
        assert_eq!(document["expected_version"], expected());
        assert!(
            !rendered.contains("LEAK_FROM_CHILD"),
            "child stdout must not enter the preflight document: {rendered}"
        );
        assert_eq!(exit_code(report.phase), 2);
    }

    #[test]
    fn parse_identity_version_requires_exact_one_line() {
        assert_eq!(
            parse_identity_version(matching_version_line().as_bytes()).as_deref(),
            Some(expected())
        );
        assert_eq!(
            parse_identity_version(format!("{}\n", matching_version_line()).as_bytes()).as_deref(),
            Some(expected())
        );
        assert_eq!(parse_identity_version(b"other 1.0.0"), None);
        assert_eq!(
            parse_identity_version(format!("{} junk", matching_version_line()).as_bytes()),
            None,
            "trailing junk must not parse as a version"
        );
        assert_eq!(
            parse_identity_version(format!("{}\nextra\n", matching_version_line()).as_bytes()),
            None,
            "a second line must not parse as a version"
        );
        for unparsable in ["1foo", "5.2", "5.2.0-"] {
            assert_eq!(
                parse_identity_version(format!("{SERVER_COMMAND} {unparsable}").as_bytes()),
                None,
                "digit-leading junk must stay unparsable: {unparsable}"
            );
        }
        assert_eq!(
            parse_identity_version(format!("{SERVER_COMMAND} 0.0.1").as_bytes()).as_deref(),
            Some("0.0.1"),
            "a different full SemVer must stay parseable so wrong_version remains reachable"
        );
    }

    #[test]
    fn file_is_not_a_directory() {
        let file = tempfile::NamedTempFile::new().expect("file");
        assert!(file.path().exists());
        assert!(!file.path().is_dir());
        let dir = dir_root();
        assert!(dir.path().is_dir());
        let _ = fs::metadata(dir.path());
    }

    #[test]
    fn advertised_probe_bounds_are_two_seconds_and_eight_kib_per_stream() {
        assert_eq!(
            PROBE_DEADLINE,
            Duration::from_secs(2),
            "changing PROBE_DEADLINE unpins the advertised 2-second bound"
        );
        assert_eq!(
            PROBE_OUTPUT_CAP,
            8 * 1024,
            "changing PROBE_OUTPUT_CAP unpins the advertised 8 KiB per-stream bound"
        );
    }

    #[test]
    fn recovery_strings_are_pinned_for_every_phase() {
        let expected = expected();
        let install = install_matching_server(expected);
        let cases = [
            (
                Phase::Missing,
                None,
                "assay-mcp-server was not found on PATH".to_string(),
                format!(
                    "Install assay-mcp-server on PATH ({install}), then re-run assay mcp preflight."
                ),
            ),
            (
                Phase::Unstartable,
                None,
                "assay-mcp-server on PATH could not be started or did not report a version"
                    .to_string(),
                "Replace the assay-mcp-server on PATH with a working binary from the same Assay release."
                    .to_string(),
            ),
            (
                Phase::WrongVersion,
                Some("0.0.1"),
                format!("assay-mcp-server reported 0.0.1 but this CLI expects {expected}"),
                format!("Install assay-mcp-server {expected} on PATH ({install})."),
            ),
            (
                Phase::InvalidRoot,
                Some(expected),
                "policy root must be an existing directory".to_string(),
                "Pass --policy-root to an existing directory.".to_string(),
            ),
            (
                Phase::StartupRefused,
                Some(expected),
                "assay-mcp-server refused to start with this policy root".to_string(),
                "Unset ASSAY_AUTH_* and confirm the server can start with this --policy-root."
                    .to_string(),
            ),
            (
                Phase::StartupTimeout,
                Some(expected),
                "assay-mcp-server did not exit before the preflight deadline".to_string(),
                "Retry after checking that assay-mcp-server is not blocked at startup.".to_string(),
            ),
            (
                Phase::Ready,
                Some(expected),
                "assay-mcp-server on PATH matches this CLI and accepted the policy root"
                    .to_string(),
                String::new(),
            ),
        ];
        for (phase, actual, message, next_step) in cases {
            assert_eq!(
                diagnosis(phase, expected, actual),
                (message.to_string(), next_step),
                "recovery strings drifted for {}",
                phase.as_str()
            );
        }
    }

    fn json_keys(document: &Value) -> Vec<String> {
        document
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect()
    }

    fn stable_v0_keys() -> [&'static str; 6] {
        [
            "schema",
            "phase",
            "message",
            "next_step",
            "expected_version",
            "policy_root",
        ]
    }

    #[test]
    fn json_v0_omits_actual_version_until_identity_parses() {
        let dir = dir_root();
        let missing = FakeProbe {
            identity: Err(ProbeFailure::NotFound),
            startup: Ok(ProbeOutput {
                exit_code: 0,
                stdout: Vec::new(),
            }),
            startups: Cell::new(0),
        };
        let missing_doc: Value =
            serde_json::from_str(&render_json(&classify(&missing, dir.path(), expected())))
                .expect("missing json");
        let mut missing_keys = json_keys(&missing_doc);
        missing_keys.sort();
        let mut expected_missing: Vec<String> =
            stable_v0_keys().into_iter().map(str::to_string).collect();
        expected_missing.sort();
        assert_eq!(
            missing_keys, expected_missing,
            "missing must not grow the v0 field set or add actual_version: {missing_doc}"
        );
        assert!(missing_doc.get("actual_version").is_none());

        let unstartable = FakeProbe {
            identity: Err(ProbeFailure::Timeout),
            startup: Ok(ProbeOutput {
                exit_code: 0,
                stdout: Vec::new(),
            }),
            startups: Cell::new(0),
        };
        let unstartable_doc: Value = serde_json::from_str(&render_json(&classify(
            &unstartable,
            dir.path(),
            expected(),
        )))
        .expect("unstartable json");
        assert!(
            unstartable_doc.get("actual_version").is_none(),
            "unstartable must omit actual_version: {unstartable_doc}"
        );
        let mut unstartable_keys = json_keys(&unstartable_doc);
        unstartable_keys.sort();
        assert_eq!(unstartable_keys, expected_missing);
    }

    #[test]
    fn json_v0_includes_actual_version_after_a_parsed_identity() {
        let dir = dir_root();
        let wrong = FakeProbe::identity_ok(&format!("{SERVER_COMMAND} 0.0.1"));
        let wrong_doc: Value =
            serde_json::from_str(&render_json(&classify(&wrong, dir.path(), expected())))
                .expect("wrong_version json");
        assert_eq!(wrong_doc["actual_version"], "0.0.1");
        let mut wrong_keys = json_keys(&wrong_doc);
        wrong_keys.sort();
        let mut expected_with_actual: Vec<String> = stable_v0_keys()
            .into_iter()
            .chain(std::iter::once("actual_version"))
            .map(str::to_string)
            .collect();
        expected_with_actual.sort();
        assert_eq!(
            wrong_keys, expected_with_actual,
            "parsed identity must add only actual_version: {wrong_doc}"
        );

        let ready = FakeProbe::identity_ok(&matching_version_line());
        let ready_doc: Value =
            serde_json::from_str(&render_json(&classify(&ready, dir.path(), expected())))
                .expect("ready json");
        assert_eq!(ready_doc["actual_version"], expected());
        let mut ready_keys = json_keys(&ready_doc);
        ready_keys.sort();
        assert_eq!(ready_keys, expected_with_actual);
        assert_eq!(ready_doc["schema"], SCHEMA);

        let file = tempfile::NamedTempFile::new().expect("file root");
        let invalid = FakeProbe::identity_ok(&matching_version_line());
        let invalid_doc: Value =
            serde_json::from_str(&render_json(&classify(&invalid, file.path(), expected())))
                .expect("invalid_root json");
        assert_eq!(invalid_doc["phase"], "invalid_root");
        assert_eq!(invalid_doc["actual_version"], expected());

        let refused = FakeProbe {
            identity: Ok(ProbeOutput {
                exit_code: 0,
                stdout: matching_version_line().into_bytes(),
            }),
            startup: Ok(ProbeOutput {
                exit_code: 1,
                stdout: Vec::new(),
            }),
            startups: Cell::new(0),
        };
        let refused_doc: Value =
            serde_json::from_str(&render_json(&classify(&refused, dir.path(), expected())))
                .expect("startup_refused json");
        assert_eq!(refused_doc["phase"], "startup_refused");
        assert_eq!(refused_doc["actual_version"], expected());

        let hung = FakeProbe {
            identity: Ok(ProbeOutput {
                exit_code: 0,
                stdout: matching_version_line().into_bytes(),
            }),
            startup: Err(ProbeFailure::Timeout),
            startups: Cell::new(0),
        };
        let hung_doc: Value =
            serde_json::from_str(&render_json(&classify(&hung, dir.path(), expected())))
                .expect("startup_timeout json");
        assert_eq!(hung_doc["phase"], "startup_timeout");
        assert_eq!(hung_doc["actual_version"], expected());
    }
}
