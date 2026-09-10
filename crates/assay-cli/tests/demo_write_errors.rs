//! `assay demo` must not claim it created files when the writes failed (#2902).
//!
//! The measured gap was `let _ = fs::write(...)` on policy.yaml, assay.yaml, and
//! traces.jsonl, followed by an unconditional "Created demo environment". These
//! tests drive the built binary and read exit code, stdout, and stderr.

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

    let (_code, _stdout, _) = run_demo(out);
    // The shipped demo policy no longer parses, so leftover files would not
    // print "Validation Passed" on their own. Plant a policy the current
    // loader accepts so an ignored rewrite can still claim success.
    fs::write(
        out.join("policy.yaml"),
        "version: \"1\"\nname: stale-demo\ntools:\n  arg_constraints:\n    Search:\n      type: object\n      properties:\n        query:\n          type: string\n",
    )
    .unwrap();

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
