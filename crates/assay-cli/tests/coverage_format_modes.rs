//! `coverage --format` is read by two commands, and each honours a different set.
//!
//! `coverage/mod.rs` dispatches on `--input`, so the same spelling reaches two renderers. While
//! the argument was a bare `String`, legacy mode matched `json`, `markdown` and `github` and let a
//! `_` arm print text for everything else, which meant `--format md` asked for markdown and
//! silently produced text with no message. These tests pin what each spelling means in each mode
//! so the two can no longer diverge without a test saying so.

use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::TempDir;

const POLICY: &str = "version: 2\nname: format-modes\ntools:\n  allow:\n    - read_file\n";
const TRACE: &str = "{\"tool\":\"read_file\",\"args\":{\"path\":\"/app/x.txt\"}}\n";

struct Fixture {
    _dir: TempDir,
    policy: PathBuf,
    trace: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let policy = dir.path().join("policy.yaml");
    let trace = dir.path().join("trace.jsonl");
    std::fs::write(&policy, POLICY).expect("write policy");
    std::fs::write(&trace, TRACE).expect("write trace");
    Fixture {
        _dir: dir,
        policy,
        trace,
    }
}

/// Legacy mode: no `--input`, so `coverage/mod.rs` routes to the legacy renderer.
fn legacy(format: &str) -> Output {
    let f = fixture();
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["coverage", "--policy"])
        .arg(&f.policy)
        .arg("--trace-file")
        .arg(&f.trace)
        .args(["--format", format])
        .output()
        .expect("failed to run assay")
}

/// The report's own first line names its shape, so the assertions do not depend on exit codes.
/// Legacy exits 1 whenever it finds a high-risk gap, which is orthogonal to the format.
fn shape(out: &Output) -> &'static str {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout.lines().next().unwrap_or("").trim().to_string();
    if first.starts_with("# Coverage Report") {
        "markdown"
    } else if first.starts_with('{') {
        "json"
    } else if first.starts_with("Coverage Report") {
        "text"
    } else if stdout.trim().is_empty() {
        "none"
    } else {
        "unknown"
    }
}

#[test]
fn legacy_mode_honours_each_advertised_spelling() {
    assert_eq!(shape(&legacy("text")), "text");
    assert_eq!(shape(&legacy("json")), "json");
    assert_eq!(shape(&legacy("markdown")), "markdown");
}

/// `github` was an explicit arm of its own before the type; it stays accepted as an alias of
/// `markdown` and renders the same report.
#[test]
fn legacy_mode_still_accepts_the_github_alias() {
    let out = legacy("github");
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("invalid value"),
        "the `github` alias was dropped rather than hidden",
    );
    assert_eq!(shape(&out), "markdown");
}

/// The defect this file exists for. `md` asked for markdown and produced text, at exit 0 or 1 and
/// with nothing on stderr, so a caller could not tell it had not been honoured.
#[test]
fn legacy_mode_rejects_md_by_name_instead_of_printing_text() {
    let out = legacy("md");

    assert_eq!(
        out.status.code(),
        Some(2),
        "`--format md` in legacy mode did not exit with the config-error code",
    );
    assert_eq!(
        shape(&out),
        "none",
        "a rejected run still wrote a report, which is where the wrong shape used to go",
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--format md is only supported with --input mode"),
        "the error does not say which mode honours `md`: {stderr}",
    );
    assert!(
        stderr.contains("--format markdown"),
        "the error does not name the spelling to use instead: {stderr}",
    );
}

/// The property the two modes have to keep: a spelling either means the same thing in both, or the
/// mode that cannot honour it says so. It must never mean two things silently.
///
/// `md` is the case that used to fail this. It means markdown with `--input`, and legacy mode now
/// refuses it rather than answering with a third shape.
#[test]
fn no_spelling_means_two_things_without_saying_so() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out_path = dir.path().join("coverage.md");
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/ci/fixtures/coverage")
        .join("input_basic.jsonl");

    let generated = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["coverage", "--input"])
        .arg(&input)
        .arg("--out")
        .arg(&out_path)
        .args(["--declared-tool", "read_document", "--format", "md"])
        .output()
        .expect("failed to run assay");
    assert!(
        generated.status.success(),
        "`--format md` with --input failed: {}",
        String::from_utf8_lossy(&generated.stderr),
    );
    let written = std::fs::read_to_string(&out_path).expect("markdown should be written");
    assert!(
        written.contains("# Coverage Report"),
        "`--format md` with --input no longer writes markdown",
    );

    let refused = legacy("md");
    assert_ne!(
        refused.status.code(),
        Some(0),
        "legacy mode accepted `md`, so the spelling means two things again",
    );
    assert!(
        !String::from_utf8_lossy(&refused.stderr).is_empty(),
        "legacy mode refused `md` without saying anything",
    );
}
