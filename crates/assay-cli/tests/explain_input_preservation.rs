use assert_cmd::Command;
use std::path::Path;

const TRACE: &str = r#"[{"tool":"Search","args":{"query":"hello"}}]"#;
const POLICY: &str = "version: '1.1'\nname: preservation\n";

fn command(trace: &Path, policy: &Path) -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("assay"));
    cmd.args(["explain", "--trace"])
        .arg(trace)
        .arg("--policy")
        .arg(policy)
        .args(["--format", "json"])
        .timeout(std::time::Duration::from_secs(15));
    cmd
}

fn refuses_alias(input: &str, alias: &str) {
    let dir = tempfile::tempdir().unwrap();
    let trace = dir.path().join("trace.json");
    let policy = dir.path().join("policy.yaml");
    std::fs::write(&trace, TRACE).unwrap();
    std::fs::write(&policy, POLICY).unwrap();
    let source = if input == "trace" { &trace } else { &policy };
    let output = if alias == "same" {
        source.clone()
    } else {
        let output = dir.path().join("output.json");
        if alias == "hardlink" {
            std::fs::hard_link(source, &output).unwrap();
        } else {
            #[cfg(unix)]
            std::os::unix::fs::symlink(source, &output).unwrap();
            #[cfg(not(unix))]
            panic!("symlink case is Unix-only");
        }
        output
    };
    let result = command(&trace, &policy)
        .arg("--output")
        .arg(output)
        .output()
        .unwrap();
    assert_eq!(
        std::fs::read(&trace).unwrap(),
        TRACE.as_bytes(),
        "trace changed"
    );
    assert_eq!(
        std::fs::read(&policy).unwrap(),
        POLICY.as_bytes(),
        "policy changed"
    );
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("Failed to write output"), "{error}");
    assert!(!error.contains("Wrote explanation"), "{error}");
}

#[test]
fn trace_same_path_is_preserved() {
    refuses_alias("trace", "same");
}
#[test]
fn policy_same_path_is_preserved() {
    refuses_alias("policy", "same");
}
#[test]
fn trace_hardlink_is_preserved() {
    refuses_alias("trace", "hardlink");
}
#[test]
fn policy_hardlink_is_preserved() {
    refuses_alias("policy", "hardlink");
}
#[cfg(unix)]
#[test]
fn trace_symlink_is_preserved() {
    refuses_alias("trace", "symlink");
}
#[cfg(unix)]
#[test]
fn policy_symlink_is_preserved() {
    refuses_alias("policy", "symlink");
}

#[test]
fn distinct_output_preserves_bytes_and_policy_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let trace = dir.path().join("trace.json");
    let policy = dir.path().join("policy.yaml");
    let output = dir.path().join("output.json");
    std::fs::write(&trace, TRACE).unwrap();
    for (policy_text, exit) in [
        (POLICY.to_owned(), 0),
        (format!("{POLICY}tools:\n  deny: [Search]\n"), 1),
    ] {
        std::fs::write(&policy, &policy_text).unwrap();
        let stdout = command(&trace, &policy)
            .assert()
            .code(exit)
            .get_output()
            .stdout
            .clone();
        // println adds one newline; file output has always omitted that newline.
        let expected = stdout.strip_suffix(b"\n").unwrap();
        command(&trace, &policy)
            .arg("--output")
            .arg(&output)
            .assert()
            .code(exit);
        assert_eq!(std::fs::read(&output).unwrap(), expected);
        std::fs::write(&output, vec![b'x'; expected.len() + 100]).unwrap();
        #[cfg(unix)]
        let before = {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o600)).unwrap();
            std::fs::metadata(&output).unwrap()
        };
        command(&trace, &policy)
            .arg("--output")
            .arg(&output)
            .assert()
            .code(exit);
        assert_eq!(std::fs::read(&output).unwrap(), expected);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let after = std::fs::metadata(&output).unwrap();
            assert_eq!(before.ino(), after.ino());
            assert_eq!(before.mode(), after.mode());
        }
        assert_eq!(std::fs::read(&trace).unwrap(), TRACE.as_bytes());
        assert_eq!(std::fs::read(&policy).unwrap(), policy_text.as_bytes());
        std::fs::remove_file(&output).unwrap();
    }
}

#[test]
fn malformed_trace_is_reported_before_output_is_opened() {
    let dir = tempfile::tempdir().unwrap();
    let trace = dir.path().join("trace.json");
    let policy = dir.path().join("policy.yaml");
    std::fs::write(&trace, "{broken").unwrap();
    std::fs::write(&policy, POLICY).unwrap();
    let result = command(&trace, &policy)
        .arg("--output")
        .arg(&trace)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("Failed to parse trace"), "{error}");
    assert!(!error.contains("Failed to write output"), "{error}");
    assert_eq!(std::fs::read(&trace).unwrap(), b"{broken");
}
