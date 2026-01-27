//! Subprocess-based bundle verification for panic=abort safety.
//!
//! Since the workspace uses `panic = "abort"` in dev and release profiles,
//! `catch_unwind` does not work. Instead, we verify mutated bundles by
//! writing them to a temp file and invoking `assay evidence verify` as a
//! subprocess. This provides:
//! - Panic isolation (abort in child does not crash the harness)
//! - Hard timeout enforcement via process kill
//! - Signal-fault resilience (SIGSEGV, etc.)

use anyhow::{Context, Result};
use std::io::Write;
use std::process::Command;
use std::time::Duration;

/// Outcome of a subprocess verification.
#[derive(Debug)]
pub struct SubprocessResult {
    /// Whether the verification passed (exit code 0).
    pub valid: bool,
    /// Exit code, if the process completed.
    pub exit_code: Option<i32>,
    /// stderr output (for diagnostics).
    pub stderr: String,
    /// Whether the process was killed due to timeout.
    pub timed_out: bool,
}

/// Verify a bundle by writing it to a temp file and invoking `assay evidence verify`.
///
/// Returns `SubprocessResult` indicating whether the bundle was accepted or rejected.
/// The caller's process is never at risk of panics or aborts from the verifier.
pub fn subprocess_verify(bundle_data: &[u8], timeout: Duration) -> Result<SubprocessResult> {
    let tmp = tempfile::Builder::new()
        .prefix("assay-sim-")
        .suffix(".tar.gz")
        .tempfile()
        .context("creating temp file for subprocess verify")?;

    tmp.as_file()
        .write_all(bundle_data)
        .context("writing bundle to temp file")?;

    let assay_bin = find_assay_binary()?;

    let mut child = Command::new(&assay_bin)
        .args(["evidence", "verify", &tmp.path().to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning assay binary: {}", assay_bin.display()))?;

    // Wait with timeout
    let result = match child.wait_timeout(timeout) {
        Ok(Some(status)) => {
            let stderr = read_stderr(&mut child);
            SubprocessResult {
                valid: status.success(),
                exit_code: status.code(),
                stderr,
                timed_out: false,
            }
        }
        Ok(None) => {
            // Timed out — kill the child
            let _ = child.kill();
            let _ = child.wait(); // reap
            SubprocessResult {
                valid: false,
                exit_code: None,
                stderr: "subprocess timed out".into(),
                timed_out: true,
            }
        }
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e).context("waiting for subprocess");
        }
    };

    Ok(result)
}

/// Find the assay binary. Prefers ASSAY_BIN env var, then current_exe's sibling, then PATH.
fn find_assay_binary() -> Result<std::path::PathBuf> {
    // 1. Explicit env var
    if let Ok(bin) = std::env::var("ASSAY_BIN") {
        let path = std::path::PathBuf::from(bin);
        if path.exists() {
            return Ok(path);
        }
    }

    // 2. Sibling of current executable (works in cargo test/run)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("assay");
            if sibling.exists() {
                return Ok(sibling);
            }
        }
    }

    // 3. Fall back to PATH lookup
    Ok(std::path::PathBuf::from("assay"))
}

fn read_stderr(child: &mut std::process::Child) -> String {
    use std::io::Read;
    let mut buf = String::new();
    if let Some(ref mut stderr) = child.stderr {
        let _ = stderr.read_to_string(&mut buf);
    }
    // Cap stderr to avoid memory issues from malicious output
    buf.truncate(4096);
    buf
}

/// Extension trait to add `wait_timeout` to `Child`.
trait ChildExt {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl ChildExt for std::process::Child {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(50);

        loop {
            match self.try_wait()? {
                Some(status) => return Ok(Some(status)),
                None => {
                    if start.elapsed() >= timeout {
                        return Ok(None);
                    }
                    std::thread::sleep(poll_interval);
                }
            }
        }
    }
}
