#![allow(unsafe_code)] // Linux child-lifecycle assertions use waitpid/kill on owned processes.

use super::*;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::time::{Duration, Instant};

fn spawn(script: &str) -> McpProxy {
    McpProxy::spawn(
        "/bin/sh",
        &["-c".into(), script.into()],
        McpPolicy::default(),
        ProxyConfig::try_from_raw(ProxyConfigRaw::default()).unwrap(),
    )
    .unwrap()
}

// Removing child reaping from Drop must leave an observable zombie here.
#[test]
fn drop_reaps_owned_child() {
    let proxy = spawn("exec sleep 30");
    let pid = proxy.child.id() as libc::pid_t;
    drop(proxy);
    assert_reaped(pid);
}

fn assert_reaped(pid: libc::pid_t) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let mut status = 0;
        // SAFETY: waitpid writes to a live local integer; this is our own child.
        let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if result == -1 {
            assert_eq!(
                io::Error::last_os_error().raw_os_error(),
                Some(libc::ECHILD)
            );
            break;
        }
        assert_eq!(result, 0, "Drop killed the child but left it unreaped");
        assert!(Instant::now() < deadline, "child remained alive after Drop");
        thread::sleep(Duration::from_millis(10));
    }
}

// A separate libtest process supplies private stdin to the real proxy run path.
#[test]
fn proxy_worker() {
    let Ok(mode) = std::env::var("ASSAY_LIFECYCLE_TEST_MODE") else {
        return;
    };
    let script = match mode.as_str() {
        "exit" => "cat >/dev/null; printf 'final-response\\n'; exit 7",
        "bad-input" | "panic-input" => "exec sleep 30",
        "slow-exit" => "cat >/dev/null; sleep 3; exit 9",
        "bad-output" => "printf '\\377\\n'; exec sleep 30",
        _ => panic!("unknown test mode"),
    };
    let proxy = spawn(script);
    let pid = proxy.child.id() as libc::pid_t;
    if mode == "panic-input" {
        let cache = proxy.identity_cache.clone();
        let _ = thread::spawn(move || {
            let _lock = cache.lock().unwrap();
            panic!("poison synthetic cache to exercise client thread panic");
        })
        .join();
    }
    let result = proxy.run();
    match mode.as_str() {
        "exit" => assert_eq!(result.unwrap(), 7),
        "slow-exit" => assert_eq!(result.unwrap(), 9),
        "panic-input" => assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Other),
        "bad-input" => assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData),
        "bad-output" => assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData),
        _ => unreachable!(),
    }
    assert_reaped(pid);
}

fn run_worker(mode: &str) -> std::process::Output {
    let mut worker = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mcp::proxy::lifecycle_tests::proxy_worker",
            "--nocapture",
        ])
        .env("ASSAY_LIFECYCLE_TEST_MODE", mode)
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = worker.stdin.take().unwrap();
    if mode == "bad-input" {
        input.write_all(b"\xff\n").unwrap();
    }
    if mode == "panic-input" {
        input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"test\",\"arguments\":{}}}\n").unwrap();
    }
    drop(input);
    let deadline = Instant::now() + Duration::from_secs(10);
    let timed_out = loop {
        if worker.try_wait().unwrap().is_some() {
            break false;
        }
        if Instant::now() >= deadline {
            break true;
        }
        thread::sleep(Duration::from_millis(10));
    };
    if timed_out {
        // SAFETY: the live worker owns a dedicated process group created above.
        // Kill its synthetic descendants too, before reaping the group leader.
        unsafe {
            libc::kill(-(worker.id() as libc::pid_t), libc::SIGKILL);
        }
    }
    let output = worker.wait_with_output().unwrap();
    assert!(!timed_out, "proxy did not finish after client EOF ({mode})");
    assert!(output.status.success(), "worker failed: {output:?}");
    output
}

#[test]
fn client_eof_preserves_exit_code_and_final_response() {
    let output = run_worker("exit");
    assert!(String::from_utf8_lossy(&output.stdout).contains("final-response\n"));
}

#[test]
fn client_read_failure_reaps_child() {
    run_worker("bad-input");
}

#[test]
fn server_read_failure_is_returned_without_waiting_for_server_exit() {
    run_worker("bad-output");
}

#[test]
fn output_initialization_failure_reaps_child() {
    let dir = tempfile::tempdir().unwrap();
    let mut proxy = spawn("exec sleep 30");
    let pid = proxy.child.id() as libc::pid_t;
    proxy.config.decision_log_path = Some(dir.path().join("missing/log.ndjson"));
    assert_eq!(proxy.run().unwrap_err().kind(), io::ErrorKind::NotFound);
    assert_reaped(pid);
}

#[test]
fn client_thread_panic_reaps_child() {
    run_worker("panic-input");
}

#[test]
fn client_eof_keeps_waiting_for_successful_slow_server() {
    run_worker("slow-exit");
}
