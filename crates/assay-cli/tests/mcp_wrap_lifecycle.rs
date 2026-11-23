//! Bounded session end for `assay mcp wrap` in either order of EOF (#3629 review follow-up).
//!
//! Each test waits for the proxy with a deadline and kills it on expiry, so a regression fails the
//! test instead of hanging the run. No test asserts on timing beyond that deadline.
use std::{
    fs,
    io::Write as _,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn wrap(d: &TempDir, script: &str) -> Child {
    fs::write(
        d.path().join("policy.yaml"),
        "version: '2.0'\nname: lifecycle-test\ntools:\n  deny: [blocked]\n",
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(d.path())
        .args([
            "mcp",
            "wrap",
            "--policy",
            "policy.yaml",
            "--event-source",
            "assay://lifecycle-test",
            "--",
            python(),
            "-c",
            script,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn assay mcp wrap (is python installed?)")
}

/// Send one request and close stdin, so the client reaches EOF first.
fn send_and_close(child: &mut Child) {
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"t","version":"1"}}}}}}"#
    )
    .unwrap();
}

fn wait_bounded(child: &mut Child, limit: Duration, what: &str) {
    let until = Instant::now() + limit;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{what}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn normal_termination_relays_the_reply_and_ends_successfully() {
    // Client EOF first; the upstream replies, reads to EOF and exits normally.
    let d = TempDir::new().unwrap();
    let script = "import sys; sys.stdin.readline(); \
        sys.stdout.write('{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"normal_reply\":true}}\\n'); \
        sys.stdout.flush(); sys.stdin.read()";
    let mut child = wrap(&d, script);
    send_and_close(&mut child);
    wait_bounded(
        &mut child,
        Duration::from_secs(15),
        "wrap did not end after the upstream exited normally",
    );
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("normal_reply"), "stdout={stdout:?}");
}

#[test]
fn slow_reader_receives_the_whole_reply() {
    // The client keeps reading, slowly. A reply larger than the pipe buffers must arrive whole and
    // the session end normally: the polled relay write waits for the reader instead of failing.
    use std::io::Read as _;
    const PAD: usize = 512 * 1024;
    let d = TempDir::new().unwrap();
    let script = format!(
        "import sys; sys.stdin.readline(); \
        sys.stdout.write('{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"pad\":\"' + 'x' * {PAD} + '\"}}}}\\n'); \
        sys.stdout.flush()"
    );
    let mut child = wrap(&d, &script);
    send_and_close(&mut child);
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut got = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            std::thread::sleep(Duration::from_millis(10));
            match stdout.read(&mut buf).unwrap() {
                0 => return got,
                n => got.extend_from_slice(&buf[..n]),
            }
        }
    });
    wait_bounded(
        &mut child,
        Duration::from_secs(20),
        "wrap did not end while a slow client was still reading",
    );
    let got = reader.join().unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr={:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let line = String::from_utf8(got).unwrap();
    let reply: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(reply["result"]["pad"].as_str().map(str::len), Some(PAD));
}

#[test]
fn upstream_that_closes_stdout_and_keeps_running_does_not_hold_the_session() {
    // Client EOF first, then the upstream replies, closes its stdout and keeps running. The response
    // stream has ended, so the session must end within a bound instead of waiting for the process.
    let d = TempDir::new().unwrap();
    let script = "import os, sys, time; sys.stdin.readline(); \
        sys.stdout.write('{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"lifecycle_reply\":true}}\\n'); \
        sys.stdout.flush(); os.close(1); time.sleep(30)";
    let mut child = wrap(&d, script);
    send_and_close(&mut child);
    wait_bounded(
        &mut child,
        Duration::from_secs(15),
        "wrap waited without bound for an upstream that had closed its stdout",
    );
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("lifecycle_reply"),
        "the reply sent before stdout closed was not relayed; stdout={stdout:?} stderr={:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn drain_deadline_returns_while_the_client_stops_reading() {
    // Client EOF first, but the client keeps its read end open without reading. The upstream writes
    // one reply larger than any pipe buffer and exits, so relaying it blocks. The drain deadline
    // must still end the session with its error instead of waiting on the blocked write.
    let d = TempDir::new().unwrap();
    let script = "import sys; sys.stdin.readline(); \
        sys.stdout.write('{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"pad\":\"' + 'x' * (4 * 1024 * 1024) + '\"}}\\n'); \
        sys.stdout.flush()";
    let mut child = wrap(&d, script);
    send_and_close(&mut child);
    // Deliberately never read child stdout.
    let mut unread_stdout = child.stdout.take();
    wait_bounded(
        &mut child,
        Duration::from_secs(20),
        "the drain deadline did not return while a write to the client was blocked",
    );
    drop(unread_stdout.take());
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "an unfinished relay must not end as success"
    );
    assert!(
        stderr.contains("drain deadline"),
        "expected the drain-deadline error; stderr={stderr:?}"
    );
}
