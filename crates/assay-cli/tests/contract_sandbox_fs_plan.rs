//! Real Linux enforcement contracts. Unsupported Landlock is a failure, never a skip.
#![cfg(target_os = "linux")]

use assert_cmd::Command;
use std::path::{Path, PathBuf};

struct Fixture {
    root: tempfile::TempDir,
    launch: PathBuf,
    child: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let launch = root.path().join("launch");
        let child = root.path().join("child");
        std::fs::create_dir(&launch).unwrap();
        std::fs::create_dir(&child).unwrap();
        std::fs::write(launch.join("secret"), "parent-secret").unwrap();
        std::fs::write(child.join("canary"), "child-readable").unwrap();
        Self {
            root,
            launch,
            child,
        }
    }

    fn base_command(&self, allow: &[String], deny: &[String]) -> Command {
        let policy = self.root.path().join("policy.yaml");
        std::fs::write(
            &policy,
            serde_json::json!({"fs": {"allow": allow, "deny": deny}}).to_string(),
        )
        .unwrap();
        let mut cmd = Command::cargo_bin("assay").unwrap();
        cmd.current_dir(&self.launch)
            .env("HOME", &self.launch)
            .env("USER", "fixture-user")
            .env("XDG_DATA_HOME", self.root.path().join("data"))
            .args(["sandbox", "--policy"])
            .arg(policy)
            .args(["--workdir", "../child"]);
        cmd
    }

    fn command(&self, allow: &[String], deny: &[String]) -> Command {
        let mut cmd = self.base_command(allow, deny);
        cmd.arg("--enforce");
        cmd
    }

    fn refused(&self, allow: &[String], deny: &[String]) {
        let out = self
            .command(allow, deny)
            .args(["--", "/bin/echo", "CHILD_STARTED"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{stderr}");
        assert!(
            stderr.contains("E_POLICY_CONFLICT_DENY_WINS_UNENFORCEABLE"),
            "{stderr}"
        );
        assert!(!String::from_utf8_lossy(&out.stdout).contains("CHILD_STARTED"));
    }
}

#[test]
fn implicit_runtime_and_system_file_denies_refuse_before_spawn() {
    for deny in ["/usr", "/etc/hosts", "/etc", "/bin/sh"] {
        Fixture::new().refused(&[], &[deny.into()]);
    }
}

#[test]
fn ancestor_deny_and_expansion_aliases_refuse_before_spawn() {
    let f = Fixture::new();
    let allowed = f.launch.join("allowed");
    std::fs::create_dir(&allowed).unwrap();
    f.refused(
        &[allowed.display().to_string()],
        &[f.launch.display().to_string()],
    );
    f.refused(
        &["${HOME}/allowed".into()],
        &["~/allowed/future-secret".into()],
    );
    f.refused(
        &["${CWD}/allowed".into()],
        &["allowed/future-secret".into()],
    );
    let named = f.launch.join("fixture-user");
    std::fs::create_dir(&named).unwrap();
    f.refused(&["${CWD}/${USER}".into()], &[named.display().to_string()]);
}

#[test]
fn symlink_aliases_cannot_hide_a_conflict() {
    let f = Fixture::new();
    let alias = f.launch.join("alias");
    std::os::unix::fs::symlink(&f.child, &alias).unwrap();
    f.refused(&[], &[alias.join("future-secret").display().to_string()]);
}

#[test]
fn dangling_deny_alias_cannot_become_readable_after_target_creation() {
    let f = Fixture::new();
    let allowed = f.launch.join("allowed");
    std::fs::create_dir(&allowed).unwrap();
    let alias = f.launch.join("alias");
    std::os::unix::fs::symlink(allowed.join("future"), &alias).unwrap();
    let out = f
        .command(
            &[allowed.display().to_string()],
            &[alias.display().to_string()],
        )
        .args(["--", "/bin/echo", "CHILD_STARTED"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "unresolved deny alias must refuse");
    assert!(String::from_utf8_lossy(&out.stderr).contains("unresolved symlink"));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("CHILD_STARTED"));
}

#[test]
fn audit_mode_does_not_require_resolvable_filesystem_rules() {
    let f = Fixture::new();
    let alias = f.launch.join("loop");
    std::os::unix::fs::symlink("loop", &alias).unwrap();
    let out = f
        .base_command(&[], &[alias.display().to_string()])
        .args(["--dry-run", "--", "/bin/echo", "AUDIT_CHILD_STARTED"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("AUDIT_CHILD_STARTED"));
}

#[test]
fn workdir_is_readable_launch_secret_is_denied_and_tmp_is_writable() {
    let f = Fixture::new();
    let out = f
        .command(&[], &[f.launch.join("secret").display().to_string()])
        .args([
            "--",
            "/usr/bin/python3",
            "-c",
            r#"
import errno, os, pathlib
try:
    pathlib.Path('../launch/secret').read_text()
except OSError as e:
    assert e.errno == errno.EACCES, e
else:
    raise AssertionError('launch directory was implicitly granted')
assert pathlib.Path('canary').read_text() == 'child-readable'
p = pathlib.Path(os.environ['TMPDIR']) / 'probe'
p.write_text('tmp-writable')
assert p.read_text() == 'tmp-writable'
try:
    pathlib.Path('unexpected-write').write_text('no')
except OSError as e:
    assert e.errno == errno.EACCES, e
else:
    raise AssertionError('implicit child workdir must stay read-only')
print('ENFORCEMENT_PROVEN')
"#,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("ENFORCEMENT_PROVEN"));
}

#[test]
fn cwd_token_and_relative_policy_paths_still_use_launch_directory() {
    for allow in ["${CWD}/allowed", "allowed"] {
        let f = Fixture::new();
        std::fs::create_dir(f.launch.join("allowed")).unwrap();
        std::fs::write(f.launch.join("allowed/canary"), "launch-allowed").unwrap();
        let out = f
            .command(&[allow.into()], &[])
            .args(["--", "/bin/cat", "../launch/allowed/canary"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), "launch-allowed");
    }
}

#[test]
fn sibling_prefixes_are_not_conflicts() {
    let f = Fixture::new();
    let allowed = f.launch.join("safe");
    std::fs::create_dir(&allowed).unwrap();
    std::fs::write(allowed.join("canary"), "safe").unwrap();
    let out = f
        .command(
            &[allowed.display().to_string()],
            &[f.launch.join("safe-old").display().to_string()],
        )
        .args(["--", "/bin/cat"])
        .arg(Path::new("../launch/safe/canary"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "safe");
}

#[test]
fn nonexistent_allow_is_not_lexically_materialized() {
    let f = Fixture::new();
    let out = f
        .command(&["${CWD}/missing/../secret".into()], &[])
        .args([
            "--",
            "/usr/bin/python3",
            "-c",
            r#"
import errno, pathlib
try:
    pathlib.Path('../launch/secret').read_text()
except OSError as e:
    assert e.errno == errno.EACCES, e
else:
    raise AssertionError('missing allow path became a real grant')
print('MISSING_ALLOW_DENIED')
"#,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("MISSING_ALLOW_DENIED"));
}
