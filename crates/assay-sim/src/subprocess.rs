//! Subprocess-based bundle verification for panic=abort safety.
//!
//! Since the workspace uses `panic = "abort"` in dev and release profiles,
//! `catch_unwind` does not work. Instead, we verify mutated bundles by
//! writing them to a temp file and invoking `assay evidence verify` as a
//! subprocess. This provides:
//! - Panic isolation (abort in child does not crash the harness)
//! - A shared deadline for child exit and stderr drain, followed by direct-child kill/reap
//! - Signal-fault resilience (SIGSEGV, etc.)

use anyhow::{Context, Result};
use std::io::Write;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;

/// Outcome of a subprocess verification.
#[derive(Debug)]
pub struct SubprocessResult {
    /// Whether the verification passed (exit code 0).
    pub valid: bool,
    /// Exit code, if the process completed.
    pub exit_code: Option<i32>,
    /// Lossy UTF-8 rendering of at most the first 4096 stderr bytes.
    /// The remainder is drained without retention; this is not a complete transcript.
    pub stderr: String,
    /// Whether the deadline elapsed waiting for the process or stderr EOF.
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

    // The sync API is also called inside the CLI's Tokio runtime. A scoped worker
    // keeps this private current-thread runtime out of that runtime's block_on.
    // Async pipes are cancellable: no detached blocking reader survives a timeout.
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("creating subprocess runtime")?
                    .block_on(verify_child(&assay_bin, tmp.path(), timeout))
            })
            .join()
            .map_err(|_| anyhow::anyhow!("subprocess worker panicked"))?
    })
}

async fn verify_child(
    assay_bin: &std::path::Path,
    bundle: &std::path::Path,
    timeout: Duration,
) -> Result<SubprocessResult> {
    let mut child = tokio::process::Command::new(assay_bin)
        .args([
            std::ffi::OsStr::new("evidence"),
            std::ffi::OsStr::new("verify"),
            bundle.as_os_str(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawning assay binary: {}", assay_bin.display()))?;
    let mut stderr = child.stderr.take().context("capturing subprocess stderr")?;
    const STDERR_CAP: usize = 4096;
    let mut prefix = Vec::with_capacity(STDERR_CAP);
    // Drain beyond the retained prefix so verbose legitimate rejections can exit.
    // Both waiting and EOF share the deadline, even if a descendant holds the pipe.
    let outcome = tokio::time::timeout(timeout, async {
        tokio::try_join!(child.wait(), async {
            let mut chunk = [0u8; 4096];
            loop {
                let n = stderr.read(&mut chunk).await?;
                if n == 0 {
                    return Ok::<_, std::io::Error>(());
                }
                let keep = n.min(STDERR_CAP - prefix.len());
                prefix.extend_from_slice(&chunk[..keep]);
            }
        })
    })
    .await;
    drop(stderr);
    let diagnostic = String::from_utf8_lossy(&prefix).into_owned();
    match outcome {
        Ok(Ok((status, ()))) => Ok(SubprocessResult {
            valid: status.success(),
            exit_code: status.code(),
            stderr: diagnostic,
            timed_out: false,
        }),
        other => {
            // kill().await kills AND reaps the direct child. An already reaped child
            // needs no signal; cleanup errors are infrastructure errors, never passes.
            if child
                .try_wait()
                .with_context(|| format!("checking subprocess before cleanup: {diagnostic}"))?
                .is_none()
            {
                child
                    .kill()
                    .await
                    .with_context(|| format!("killing and reaping subprocess: {diagnostic}"))?;
            }
            match other {
                Err(_) => Ok(SubprocessResult {
                    valid: false,
                    exit_code: None,
                    stderr: diagnostic,
                    timed_out: true,
                }),
                Ok(Err(error)) => {
                    Err(error).with_context(|| format!("waiting/draining subprocess: {diagnostic}"))
                }
                Ok(Ok(_)) => unreachable!(),
            }
        }
    }
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
