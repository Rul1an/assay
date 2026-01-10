use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn kill_by_proc_id_works() {
    // Start a long-running process
    let mut child = Command::new(if cfg!(windows) { "cmd" } else { "sleep" })
        .args(if cfg!(windows) { &["/C", "timeout", "/T", "300"][..] } else { &["300"][..] })
        .spawn()
        .expect("spawn");

    let pid = child.id();
    assert!(pid > 0);

    // Verify it is running (optional, but good sanity)
    // On Unix we can send signal 0, but std doesn't expose that easily.
    // relying on `kill` command to succeed is the test.

    // Kill it using assay kill
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.args(["kill", &format!("proc-{}", pid)]);

    // We expect success
    cmd.assert().success();

    // Verify process is gone
    // wait() should return immediately if it was killed.
    // Actually, on Unix, if we wait() on a child that was killed by SIGKILL, it returns status (signal).
    // If it wasn't killed, wait() would block (timeout is 300s).
    // So if this test finishes, it means it was killed.
    // But to be sure, we can check result.
    match child.wait() {
        Ok(status) => {
            // Check if killed by signal
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                // SIGKILL is 9
                assert_eq!(status.signal(), Some(9));
                // Note: If kill logic uses graceful (SIGTERM), it might be 15.
                // Default is immediate (SIGKILL).
            }
        },
        Err(e) => panic!("wait failed: {}", e),
    }
}
