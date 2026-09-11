//! `assay demo` write failures (#2902), valid first-run (#2905), and Calculate evaluation (#2916).
//!
//! The write-error gap was `let _ = fs::write(...)` on policy.yaml, assay.yaml,
//! and traces.jsonl, followed by an unconditional "Created demo environment".
//! The parse gap was a shipped policy in the old `tools: Search:` shape, which
//! `Policy` rejects (`allow`, `deny`, `require_args`, `arg_constraints`). The
//! Calculate evaluation gap was a trace line without a prompt matched by any
//! test in assay.yaml, leaving its schema rule unexercised. The negative pins
//! rewrite the Search query and Calculate operation after a passing first run so
//! vacuous schemas cannot stay green. These tests drive the built binary and
//! read exit code, stdout, and stderr.

use assert_cmd::Command;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const DEMO_FILES: [&str; 3] = ["policy.yaml", "assay.yaml", "traces.jsonl"];

fn run_demo(out: &Path) -> (i32, String, String) {
    let output = Command::cargo_bin("assay")
        .unwrap()
        .current_dir(out)
        .arg("demo")
        .arg("--out")
        .arg(out)
        .output()
        .expect("spawn assay demo");
    let code = output.status.code().expect("assay demo exit code");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (code, stdout, stderr)
}

fn run_printed_validate(stdout: &str) -> (i32, String, String) {
    let line = stdout
        .lines()
        .find(|line| line.contains("assay validate --config"))
        .unwrap_or_else(|| panic!("demo stdout missing next-step validate command:\n{stdout}"));
    let command = line
        .find("assay validate")
        .map(|idx| &line[idx..])
        .unwrap_or(line);
    let mut parts = command.split_whitespace();
    assert_eq!(parts.next(), Some("assay"));
    assert_eq!(parts.next(), Some("validate"));
    assert_eq!(parts.next(), Some("--config"));
    let config = parts
        .next()
        .unwrap_or_else(|| panic!("next-step missing --config path:\n{line}"));
    assert_eq!(parts.next(), Some("--trace-file"));
    let trace = parts
        .next()
        .unwrap_or_else(|| panic!("next-step missing --trace-file path:\n{line}"));

    let output = Command::cargo_bin("assay")
        .unwrap()
        .arg("validate")
        .arg("--config")
        .arg(config)
        .arg("--trace-file")
        .arg(trace)
        .output()
        .expect("spawn assay validate");
    let code = output.status.code().expect("assay validate exit code");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (code, stdout, stderr)
}

fn rewrite_search_query(traces: &str, query: &str) -> String {
    let needle = r#""query": "assay rules""#;
    let replacement = format!(r#""query": "{query}""#);
    assert!(
        traces.contains(needle),
        "traces.jsonl missing the demo Search query to rewrite:\n{traces}"
    );
    traces.replacen(needle, &replacement, 1)
}

fn rewrite_calculate_operation(traces: &str, operation: &str) -> String {
    let needle = r#""operation": "add""#;
    let replacement = format!(r#""operation": "{operation}""#);
    assert!(
        traces.contains(needle),
        "traces.jsonl missing the demo Calculate operation to rewrite:\n{traces}"
    );
    traces.replacen(needle, &replacement, 1)
}

#[test]
fn demo_completes_and_printed_validate_passes() {
    let dir = tempdir().unwrap();
    let out = dir.path();
    let (code, stdout, stderr) = run_demo(out);

    assert_eq!(
        code, 0,
        "assay demo --out should exit 0\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Validation Passed"),
        "assay demo should report Validation Passed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Search.query"),
        "assay demo stdout should name checked rule Search.query\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Calculate.operation"),
        "assay demo stdout should name checked rule Calculate.operation\nstdout:\n{stdout}"
    );

    let (vcode, vstdout, vstderr) = run_printed_validate(&stdout);
    assert_eq!(
        vcode, 0,
        "printed next-step validate should exit 0\nstdout:\n{vstdout}\nstderr:\n{vstderr}"
    );
    assert!(
        vstderr.contains("Validation OK"),
        "printed next-step validate should report Validation OK\nstdout:\n{vstdout}\nstderr:\n{vstderr}"
    );
}

#[test]
fn printed_validate_rejects_forbidden_search_query() {
    let dir = tempdir().unwrap();
    let out = dir.path();
    let (code, stdout, stderr) = run_demo(out);

    assert_eq!(
        code, 0,
        "assay demo --out should exit 0\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let traces = out.join("traces.jsonl");
    let rewritten = rewrite_search_query(&fs::read_to_string(&traces).unwrap(), "assay;rules");
    fs::write(&traces, rewritten).unwrap();

    let (vcode, vstdout, vstderr) = run_printed_validate(&stdout);
    assert_ne!(
        vcode, 0,
        "printed next-step validate should reject a forbidden Search query\nstdout:\n{vstdout}\nstderr:\n{vstderr}"
    );
    let combined = format!("{vstdout}{vstderr}");
    assert!(
        combined.contains("E_ARG_SCHEMA"),
        "validate output should name E_ARG_SCHEMA\nstdout:\n{vstdout}\nstderr:\n{vstderr}"
    );
}

#[test]
fn printed_validate_rejects_forbidden_calculate_operation() {
    let dir = tempdir().unwrap();
    let out = dir.path();
    let (code, stdout, stderr) = run_demo(out);

    assert_eq!(
        code, 0,
        "assay demo --out should exit 0\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let traces = out.join("traces.jsonl");
    let rewritten = rewrite_calculate_operation(&fs::read_to_string(&traces).unwrap(), "multiply");
    fs::write(&traces, rewritten).unwrap();

    let (vcode, vstdout, vstderr) = run_printed_validate(&stdout);
    assert_ne!(
        vcode, 0,
        "printed next-step validate should reject a forbidden Calculate operation\nstdout:\n{vstdout}\nstderr:\n{vstderr}"
    );
    let combined = format!("{vstdout}{vstderr}");
    assert!(
        combined.contains("E_ARG_SCHEMA"),
        "validate output should name E_ARG_SCHEMA\nstdout:\n{vstdout}\nstderr:\n{vstderr}"
    );
    assert!(
        combined.contains("Calculate"),
        "validate output should name tool Calculate\nstdout:\n{vstdout}\nstderr:\n{vstderr}"
    );
    assert!(
        combined.contains("operation"),
        "validate output should name property operation\nstdout:\n{vstdout}\nstderr:\n{vstderr}"
    );
}

#[test]
fn demo_reports_write_failure_when_target_is_a_directory() {
    for name in DEMO_FILES {
        let dir = tempdir().unwrap();
        let out = dir.path();
        let blocked = out.join(name);
        fs::create_dir(&blocked).unwrap();

        let (code, stdout, stderr) = run_demo(out);

        assert_ne!(
            code, 0,
            "{name}: expected non-zero exit, got {code}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(
            !stdout.contains("Created demo environment"),
            "{name}: stdout claimed success after a failed write:\n{stdout}"
        );
        assert!(
            stderr.contains("failed to write demo file"),
            "{name}: stderr missing write-failure context:\n{stderr}"
        );
        assert!(
            stderr.contains(&blocked.display().to_string()),
            "{name}: stderr missing path {}:\n{stderr}",
            blocked.display()
        );
    }
}

#[cfg(unix)]
#[test]
fn demo_does_not_report_validation_passed_on_stale_unwritable_files() {
    let dir = tempdir().unwrap();
    let out = dir.path();

    let (seed_code, seed_stdout, seed_stderr) = run_demo(out);
    assert_eq!(
        seed_code, 0,
        "stale-file seed must be the demo's own passing output\nstdout:\n{seed_stdout}\nstderr:\n{seed_stderr}"
    );
    assert!(
        seed_stdout.contains("Validation Passed"),
        "stale-file seed must be content that already printed Validation Passed:\n{seed_stdout}"
    );

    let restore = RestoreWritable {
        dir: out.to_path_buf(),
    };
    for name in DEMO_FILES {
        let path = out.join(name);
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&path, perms).unwrap();
    }
    let mut dir_perms = fs::metadata(out).unwrap().permissions();
    dir_perms.set_mode(0o555);
    fs::set_permissions(out, dir_perms).unwrap();

    if fs::write(out.join("probe_write"), "x").is_ok() {
        eprintln!(
            "skipping stale-file write-failure test: probe write succeeded (running as root)"
        );
        return;
    }

    let (code, stdout, stderr) = run_demo(out);
    assert_ne!(
        code, 0,
        "expected non-zero exit on unwritable stale files\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("Validation Passed"),
        "stdout reported validation success from stale files:\n{stdout}"
    );
    drop(restore);
}

#[cfg(unix)]
struct RestoreWritable {
    dir: std::path::PathBuf,
}

#[cfg(unix)]
impl Drop for RestoreWritable {
    fn drop(&mut self) {
        let mut dir_perms = match fs::metadata(&self.dir) {
            Ok(meta) => meta.permissions(),
            Err(_) => return,
        };
        dir_perms.set_mode(0o755);
        let _ = fs::set_permissions(&self.dir, dir_perms);
        for name in DEMO_FILES {
            let path = self.dir.join(name);
            if let Ok(meta) = fs::metadata(&path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o644);
                let _ = fs::set_permissions(&path, perms);
            }
        }
        let probe = self.dir.join("probe_write");
        if probe.exists() {
            let _ = fs::remove_file(probe);
        }
    }
}
