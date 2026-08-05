use anyhow::Context;

use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

mod common;
use common::{cargo_bin, strip_cargo_crate_env};

/// How long a single JSON-RPC response may take before the proxy is declared wedged.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Path to the `assay` binary under test.
///
/// `assay` is a bin target of this package, so Cargo injects the path at compile
/// time and guarantees the binary is built and current before the test runs.
fn assay_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_assay"))
}

/// Path to a freshly built `assay-mcp-server` binary.
///
/// Cargo only injects `CARGO_BIN_EXE_*` for bins declared in the *same* package,
/// and `assay-mcp-server` is a separate workspace member — so there is no env var
/// to read here. The previous approach guessed at `<workspace>/target/debug/
/// assay-mcp-server` and asserted the path existed, which fails on a tree that
/// has never built the server, reading as a test failure rather than as a
/// missing build step. It also could not tell a current binary from a stale one
/// left over by an older tree.
///
/// Instead, ask Cargo to build it and to report where it put it. Cargo owns the
/// staleness computation, so this also covers changes in transitive dependencies
/// such as `assay-core` that an mtime comparison against `assay-mcp-server/src`
/// would miss. On an already-current tree this is a no-op freshness check.
///
/// What this does and does not buy, because the difference is easy to overstate:
/// it guarantees the binary these tests spawn is built from the current source.
/// It does *not* make the tests fail when it would not be. Only
/// `owasp_mcp01_token_args_do_not_leak_to_proxy_logs` depends on the server
/// answering at all; `e2e_wrap_denies_wildcard_contains` and
/// `e2e_wrap_denies_schema_violation` assert verdicts the proxy reaches before
/// dispatching upstream. They pass against anything that spawns and then exits
/// when its stdin closes — `/usr/bin/true` included, so the upstream need not
/// speak the protocol, or read a byte, to satisfy them. (#1981 added an
/// exit-status assert, which does make a server that hangs past five seconds
/// fail them; that bounds how the upstream *terminates*, not what it answers.)
/// Detecting a wrong server in those two needs an assertion only a correct
/// server can satisfy, which is a change to what they test rather than to how
/// the binary is found, and is what #1988 is for.
///
/// Panics (never skips) if the build fails: a skipped e2e test that reads as
/// green is a worse failure than a loud one.
fn assay_mcp_server_bin() -> PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| build_assay_mcp_server().expect("build assay-mcp-server for e2e wrap tests"))
        .clone()
}

/// Where the parent build put its artifacts: the cross-compilation target triple
/// if there is one, and the profile directory.
///
/// Build the nested binary into the same place, so `cargo test --release` does
/// not trigger a second full dependency build in the other profile.
///
/// `CARGO_BIN_EXE_assay` is `<target-dir>/<profile-dir>/assay`, or
/// `<target-dir>/<triple>/<profile-dir>/assay` when the parent passed `--target`.
/// Reading the parent directory name alone cannot tell those apart — under
/// `--target` it still yields `debug`, and the nested build would then quietly
/// produce a *host* binary, in a different artifact tree from the one under
/// test. Stripping the target directory shows which shape it is.
///
/// The target directory comes from `cargo metadata`, not from reading
/// `CARGO_TARGET_DIR`, and both paths are canonicalised before comparing, because
/// `strip_prefix` is lexical. Two ways the env var alone gets this wrong, both
/// verified against real Cargo output: a *relative* value is resolved by Cargo
/// against the directory cargo was invoked from, not the package root a test runs
/// in, so `target/wt` would canonicalise here to `crates/assay-cli/target/wt` and
/// never match; and a symlinked one may be reported canonicalised by Cargo (on
/// macOS `/tmp` is `/private/tmp`) while the env var still holds the symlink. A
/// `build.target-dir` in `.cargo/config.toml` is invisible to the env var
/// entirely. Each of those silently dropped the triple.
///
/// Falls back to the profile-dir-only reading if even that does not line up.
/// That reading is correct whenever `--target` is absent, which is every
/// invocation in this repo today, and wrong only under `--target`, where it
/// would build a host binary while the test runs a cross-compiled one. The
/// fallback is therefore stated rather than hidden — but asking Cargo removes
/// the cases that actually reached it.
fn target_and_profile(workspace_root: &Path) -> (Option<String>, String) {
    let bin_exe_raw = Path::new(env!("CARGO_BIN_EXE_assay"));
    let dir_name = |p: Option<&Path>| {
        p.and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_owned)
    };
    // `debug` is the fallback rather than an error: a wrong profile costs build
    // time, and there is no reading of this path that should fail a test.
    let profile_only = || {
        (
            None,
            dir_name(bin_exe_raw.parent()).unwrap_or_else(|| "debug".into()),
        )
    };

    // Ask Cargo where the target directory is, rather than reading the env var
    // and hoping. A relative `CARGO_TARGET_DIR` is resolved by Cargo against the
    // directory *cargo* was invoked from, which is not the package root a test
    // runs in — so reading it here and canonicalising against our own cwd turns
    // `target/wt` into `crates/assay-cli/target/wt` and the prefix never matches.
    // `cargo metadata` reports an absolute `target_directory` and settles it,
    // including a `build.target-dir` set in config.toml, which the env var alone
    // never sees. It is one extra process per test binary, inside the cached
    // build, and this is a file about not guessing at Cargo's layout.
    let target_dir =
        resolved_target_dir(workspace_root).unwrap_or_else(|| workspace_root.join("target"));
    // Canonicalising needs the paths to exist. They do: the binary is what Cargo
    // just built, and the target dir contains it. Fall back rather than fail.
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let (bin_exe, target_dir) = (canon(bin_exe_raw), canon(&target_dir));
    let Ok(rel) = bin_exe.strip_prefix(&target_dir) else {
        return profile_only();
    };

    // rel is `<profile>/assay` or `<triple>/<profile>/assay`.
    let mut components: Vec<_> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    components.pop(); // the file name
    match components[..] {
        [profile] => (None, profile.to_owned()),
        [triple, profile] => (Some(triple.to_owned()), profile.to_owned()),
        _ => profile_only(),
    }
}

/// Cargo's own answer for where build artifacts go, absolute and already
/// accounting for `CARGO_TARGET_DIR` and `build.target-dir`.
///
/// Returns `None` rather than failing: a wrong target directory costs a rebuild
/// in the wrong place, and no reading of this should turn into a test failure.
fn resolved_target_dir(workspace_root: &Path) -> Option<PathBuf> {
    let mut cmd = Command::new(cargo_bin());
    cmd.current_dir(workspace_root).args([
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
        "--offline",
    ]);
    strip_cargo_crate_env(&mut cmd);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let meta: Value = serde_json::from_slice(&out.stdout).ok()?;
    meta.get("target_directory")?.as_str().map(PathBuf::from)
}

fn build_assay_mcp_server() -> anyhow::Result<PathBuf> {
    // crates/assay-cli -> crates -> workspace root
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .context("failed to resolve workspace root from CARGO_MANIFEST_DIR")?;

    let (target_triple, profile_dir) = target_and_profile(workspace_root);

    let cargo = cargo_bin();
    let mut cmd = Command::new(cargo);
    cmd.current_dir(workspace_root).args([
        "build",
        "-p",
        "assay-mcp-server",
        "--bin",
        "assay-mcp-server",
        // The parent resolved the graph already; a test has no business editing
        // Cargo.lock. `cargo test --locked` (ci.yml) would otherwise not extend
        // its guarantee to the build this test performs.
        "--locked",
    ]);
    if let Some(triple) = &target_triple {
        cmd.args(["--target", triple]);
    }
    if profile_dir != "debug" {
        cmd.args(["--profile", &profile_dir]);
    }
    // json on stdout for the artifact path, human-readable diagnostics on stderr.
    cmd.arg("--message-format=json-render-diagnostics");
    strip_cargo_crate_env(&mut cmd);

    let out = cmd
        .output()
        .with_context(|| format!("failed to run `{cargo} build -p assay-mcp-server`"))?;
    if !out.status.success() {
        anyhow::bail!(
            "`cargo build -p assay-mcp-server` failed with status {}.\n\
             The e2e wrap tests drive the real server binary; run it yourself to \
             see the errors:\n    cargo build -p assay-mcp-server\n--- cargo stderr ---\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Take the `executable` of the bin artifact. Cargo emits this for fresh
    // (already up-to-date) units too, so it is authoritative either way.
    let executable = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|msg| {
            msg["reason"] == "compiler-artifact" && msg["target"]["name"] == "assay-mcp-server"
        })
        .filter_map(|msg| msg["executable"].as_str().map(PathBuf::from))
        .next_back()
        .context(
            "`cargo build -p assay-mcp-server` reported no bin artifact; \
             has the `assay-mcp-server` bin target been renamed or removed?",
        )?;

    Ok(executable)
}

/// Write one JSON line to stdin (newline delimited JSON-RPC).
fn send_line(stdin: &mut dyn Write, v: &Value) -> anyhow::Result<()> {
    let s = serde_json::to_string(v)?;
    stdin.write_all(s.as_bytes())?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    Ok(())
}

/// Line reader for the proxy's stdout whose timeout survives a proxy that never answers.
///
/// `BufRead::read_line` blocks until a newline, EOF, or error, so a deadline checked around it is
/// only ever evaluated once a line has already arrived. The blocking read therefore runs on a
/// worker thread and the test waits on a channel instead, which bounds the wait whether or not the
/// proxy writes anything. (A read timeout on the pipe itself would do as well, but `ChildStdout`
/// is not a socket, so that needs platform-specific code on both Unix and Windows.)
///
/// The worker exits on EOF, on a read error, or once the receiver is gone. If the proxy is wedged
/// it stays parked in `read_line`; killing the child closes the pipe and releases it, which is why
/// [`read_json_line`] kills on timeout rather than leaving the process behind.
struct JsonLines {
    rx: mpsc::Receiver<io::Result<String>>,
}

impl JsonLines {
    fn new(stdout: ChildStdout) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    // EOF: dropping `tx` is what reports it to the receiver.
                    Ok(0) => break,
                    // A send error means the test has moved on; stop reading.
                    Ok(_) => {
                        if tx.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(Err(err));
                        break;
                    }
                }
            }
        });
        Self { rx }
    }

    /// Wait up to `timeout` for the next JSON line.
    ///
    /// `what` names the thing being awaited, so a timeout points at the request that went
    /// unanswered instead of reporting a bare "timeout".
    ///
    /// The deadline is checked explicitly at the top of each iteration and not left to
    /// `recv_timeout` alone. `recv_timeout` attempts an optimistic `try_recv` first, so it returns
    /// an already-queued line even when the remaining duration is zero; a child that writes
    /// non-JSON faster than this loop skips it therefore keeps the channel non-empty and runs
    /// unbounded past the deadline. `json_lines_deadline_holds_against_a_flood_of_non_json_lines`
    /// covers that case.
    fn next_json(&mut self, timeout: Duration, what: &str) -> anyhow::Result<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                anyhow::bail!(
                    "timed out after {timeout:?} waiting for {what}: no JSON response line arrived"
                );
            }
            let remaining = deadline.saturating_duration_since(now);
            let line = match self.rx.recv_timeout(remaining) {
                Ok(Ok(line)) => line,
                Ok(Err(err)) => {
                    return Err(anyhow::Error::new(err)
                        .context(format!("read error while waiting for {what}")))
                }
                Err(RecvTimeoutError::Timeout) => anyhow::bail!(
                    "timed out after {timeout:?} waiting for {what}: the proxy wrote no response line"
                ),
                Err(RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("EOF from proxy while waiting for {what}")
                }
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Ignore log lines if any
            if !line.starts_with('{') {
                continue;
            }
            return serde_json::from_str::<Value>(line)
                .with_context(|| format!("malformed JSON line while waiting for {what}: {line}"));
        }
    }
}

/// Read one JSON line from the proxy, killing it if the response does not arrive in time.
///
/// A timed-out read means the proxy is wedged, and leaving it running orphans it (and the server
/// it wrapped) onto the inherited stderr for as long as the test binary lives.
fn read_json_line(
    child: &mut Child,
    lines: &mut JsonLines,
    timeout: Duration,
    what: &str,
) -> anyhow::Result<Value> {
    lines.next_json(timeout, what).inspect_err(|_| {
        let _ = child.kill();
        let _ = child.wait();
    })
}

/// A child that floods stdout with non-JSON must not be able to outrun the deadline.
///
/// The silent-child case and this one fail differently: there, no line ever arrives and
/// `recv_timeout` reports `Timeout`; here lines arrive faster than they are skipped, so the
/// channel is never empty when the receive is attempted, and `recv_timeout`'s optimistic
/// `try_recv` hands back a queued line even at zero remaining duration. Only the explicit deadline
/// check in `next_json` ends this run.
///
/// `yes` and not a shell `while` loop on purpose: a shell loop produces more slowly than this
/// consumer skips, so the channel drains, the receive finds it empty, and the run bounds itself
/// even without the deadline check. Reproducing the defect needs a producer that outruns the
/// consumer, which is what makes this a regression test rather than a test that happens to pass.
#[cfg(unix)]
#[test]
fn json_lines_deadline_holds_against_a_flood_of_non_json_lines() -> anyhow::Result<()> {
    let timeout = Duration::from_secs(1);
    let mut child = Command::new("yes")
        .arg("not-json")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut lines = JsonLines::new(child.stdout.take().expect("stdout"));

    let start = Instant::now();
    let err = lines
        .next_json(timeout, "a response that never comes")
        .expect_err("a flood of non-JSON lines must not satisfy the read");
    let elapsed = start.elapsed();

    let _ = child.kill();
    let _ = child.wait();

    // Generous headroom over the 1s deadline: this asserts the deadline is enforced at all, not
    // that it is precise, so a loaded CI machine does not turn a real bound into a flaky one.
    assert!(
        elapsed < timeout * 5,
        "the deadline did not bound the read: gave up only after {elapsed:?} (timeout was {timeout:?})"
    );
    assert!(
        err.to_string().contains("timed out"),
        "expected a timeout failure, got: {err}"
    );
    Ok(())
}

fn wait_child_with_timeout(child: &mut Child, timeout: Duration) -> anyhow::Result<ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }

        if start.elapsed() > timeout {
            let _ = child.kill();
            let status = child.wait()?;
            anyhow::bail!(
                "child did not exit within {:?}; killed with status {status}",
                timeout
            );
        }

        std::thread::sleep(Duration::from_millis(25));
    }
}

fn extract_structured_contract(resp: &Value) -> Option<&Value> {
    resp.get("result")
        .and_then(|r| {
            r.get("structuredContent")
                .or_else(|| r.get("structured_content"))
        })
        .or_else(|| {
            resp.get("payload")
                .and_then(|p| p.get("result"))
                .and_then(|r| {
                    r.get("structuredContent")
                        .or_else(|| r.get("structured_content"))
                })
        })
}

fn extract_error_code(resp: &Value) -> Option<String> {
    extract_structured_contract(resp)
        .and_then(|c| c.get("error_code"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Parse the JSON payload the server returns as `result.content[0].text`.
///
/// `assay_check_args` reports its verdict in that text block (not in `structuredContent`), so a
/// test that wants to distinguish "the policy was evaluated" from "the call errored out" has to
/// look here.
fn extract_tool_payload(resp: &Value) -> anyhow::Result<Value> {
    let text = resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|c| c.get("text"))
        .and_then(|v| v.as_str())
        .with_context(|| format!("response has no result.content[0].text: {resp}"))?;
    serde_json::from_str::<Value>(text)
        .with_context(|| format!("result.content[0].text is not JSON: {text}"))
}

/// Mask the fixture's token-like argument in an assertion's failure message.
///
/// Only the rendered text is masked; every assertion still compares against the raw value, so a
/// leak still fails the test. The surrounding record is kept intact, so a failure still shows
/// which log line and which field carried the token — just with the canary itself replaced. A
/// leak-detection fixture should not print its own canary into CI output.
fn mask_secret(rendered: &str, secret: &str) -> String {
    rendered.replace(secret, "<REDACTED-FIXTURE-TOKEN>")
}

#[test]
fn owasp_mcp01_token_args_do_not_leak_to_proxy_logs() -> anyhow::Result<()> {
    let assay = assay_bin();
    let server = assay_mcp_server_bin();

    let tmp = TempDir::new()?;
    let policy_path = tmp.path().join("proxy-policy.yaml");
    let policy_root = tmp.path().join("policy-root");
    let audit_log = tmp.path().join("audit.ndjson");
    let decision_log = tmp.path().join("decisions.ndjson");
    std::fs::create_dir_all(&policy_root)?;

    std::fs::write(
        &policy_path,
        r#"
version: "2.0"
name: "owasp-mcp01-token-log-fixture"
tools:
  allow: ["assay_check_args"]
enforcement:
  unconstrained_tools: allow
"#,
    )?;

    // The `policy` argument of `assay_check_args` is a path RELATIVE TO --policy-root, not an
    // inline policy document. Passing YAML text there resolves to a nonexistent file and the call
    // errors out with E_POLICY_NOT_FOUND before any policy is evaluated, which would leave this
    // regression test asserting redaction only on the error path.
    std::fs::write(
        policy_root.join("read-file.yaml"),
        r#"
version: "2.0"
name: "owasp-mcp01-inner"
tools:
  allow: ["read_file"]
enforcement:
  unconstrained_tools: allow
"#,
    )?;

    let mut child = Command::new(&assay)
        .args([
            "mcp",
            "wrap",
            "--policy",
            policy_path.to_string_lossy().as_ref(),
            "--event-source",
            "assay://tests/owasp-mcp01",
            "--audit-log",
            audit_log.to_string_lossy().as_ref(),
            "--decision-log",
            decision_log.to_string_lossy().as_ref(),
            "--",
            server.to_string_lossy().as_ref(),
            "--policy-root",
            policy_root.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn {}", assay.display()))?;

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut lines = JsonLines::new(stdout);
    let secret = "ghp_assay_fixture_DO_NOT_LEAK_0123456789";

    // Both calls carry the same token-like argument. The first resolves its policy and is actually
    // evaluated (normal path); the second names a policy that does not exist (error path).
    let call = |id: &str, policy: &str| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": "assay_check_args",
                "arguments": {
                    "tool": "read_file",
                    "arguments": {
                        "path": "/workspace/report.md",
                        "authorization": secret
                    },
                    "policy": policy
                }
            }
        })
    };

    send_line(&mut stdin, &call("token-log-allowed", "read-file.yaml"))?;
    let allowed_resp = read_json_line(
        &mut child,
        &mut lines,
        RESPONSE_TIMEOUT,
        "the tools/call response to request token-log-allowed (the normal, policy-resolving path)",
    )?;
    send_line(
        &mut stdin,
        &call("token-log-missing", "does-not-exist.yaml"),
    )?;
    let error_resp = read_json_line(
        &mut child,
        &mut lines,
        RESPONSE_TIMEOUT,
        "the tools/call response to request token-log-missing (the unresolvable-policy path)",
    )?;
    drop(stdin);
    let status = wait_child_with_timeout(&mut child, Duration::from_secs(5))?;
    assert!(status.success(), "proxy exited with status {status}");

    // Normal path: the policy was found and evaluated, so redaction below is asserted on a
    // successful tool call and not merely on an early error.
    let allowed_rendered = mask_secret(&allowed_resp.to_string(), secret);
    let allowed_payload = extract_tool_payload(&allowed_resp)?;
    assert_eq!(
        allowed_payload.get("allowed"),
        Some(&Value::Bool(true)),
        "expected the policy to be evaluated and allow the call, got {allowed_rendered}"
    );
    assert_eq!(
        allowed_resp
            .get("result")
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "expected a non-error result on the normal path, got {allowed_rendered}"
    );

    // Error path (retained): an unresolvable policy still reports E_POLICY_NOT_FOUND.
    let error_rendered = mask_secret(&error_resp.to_string(), secret);
    let error_payload = extract_tool_payload(&error_resp)?;
    assert_eq!(
        error_payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|v| v.as_str()),
        Some("E_POLICY_NOT_FOUND"),
        "expected E_POLICY_NOT_FOUND on the error path, got {error_rendered}"
    );

    // Neither response may echo the token back to the caller either.
    for resp in [&allowed_resp, &error_resp] {
        let rendered = resp.to_string();
        assert!(
            !rendered.contains(secret),
            "response leaked raw token-like argument: {}",
            mask_secret(&rendered, secret)
        );
    }

    let audit = std::fs::read_to_string(&audit_log)?;
    let decisions = std::fs::read_to_string(&decision_log)?;
    assert!(
        !audit.contains(secret),
        "audit log leaked raw token-like argument: {}",
        mask_secret(&audit, secret)
    );
    assert!(
        !decisions.contains(secret),
        "decision log leaked raw token-like argument: {}",
        mask_secret(&decisions, secret)
    );
    assert!(audit.contains("assay_check_args"));
    assert!(decisions.contains("assay_check_args"));

    // Both calls must be present in both logs, so the no-leak assertions above cover the normal
    // path and the error path rather than one standing in for the other.
    for id in ["token-log-allowed", "token-log-missing"] {
        assert!(
            audit.contains(id),
            "audit log is missing the record for request {id}: {}",
            mask_secret(&audit, secret)
        );
        assert!(
            decisions.contains(id),
            "decision log is missing the record for request {id}: {}",
            mask_secret(&decisions, secret)
        );
    }

    Ok(())
}

#[test]
fn e2e_wrap_denies_wildcard_contains() -> anyhow::Result<()> {
    let assay = assay_bin();
    let server = assay_mcp_server_bin();

    let tmp = TempDir::new()?;
    let policy_path = tmp.path().join("proxy-policy.yaml");
    let policy_root = tmp.path().join("policy-root");
    std::fs::create_dir_all(&policy_root)?;

    // Proxy policy: wildcard deny *kill*
    std::fs::write(
        &policy_path,
        r#"
version: "2.0"
name: "e2e-proxy"
tools:
  allow: ["*"]
  deny: ["exec*", "*sh", "*kill*"]
enforcement:
  unconstrained_tools: allow
"#,
    )?;

    // Spawn the proxy wrap, pointing to the server binary directly (no PATH).
    let mut child = Command::new(&assay)
        .args([
            "mcp",
            "wrap",
            "--policy",
            policy_path.to_string_lossy().as_ref(),
            "--",
            server.to_string_lossy().as_ref(),
            "--policy-root",
            policy_root.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn {}", assay.display()))?;

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut lines = JsonLines::new(stdout);

    // tools/call -> "skill_check" should match *kill* and be denied by proxy
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "skill_check", "arguments": {} }
    });

    send_line(&mut stdin, &req)?;
    let resp = read_json_line(
        &mut child,
        &mut lines,
        RESPONSE_TIMEOUT,
        "the tools/call response to the denied skill_check request",
    )?;

    // Accept both transitional codes (old/new) while you converge
    let code = extract_error_code(&resp).unwrap_or_default();
    assert!(
        code == "E_TOOL_DENIED" || code == "MCP_TOOL_DENIED" || code == "E_TOOL_NOT_ALLOWED",
        "expected deny-ish error_code, got '{code}'. resp={resp}"
    );

    // Close stdin and reap the proxy instead of killing it: a kill only reaches the `assay`
    // parent, orphaning the wrapped assay-mcp-server, which then races TempDir teardown and fails
    // its own --policy-root canonicalization on a directory that has just been deleted.
    drop(stdin);
    let status = wait_child_with_timeout(&mut child, Duration::from_secs(5))?;
    assert!(status.success(), "proxy exited with status {status}");
    Ok(())
}

#[test]
fn e2e_wrap_denies_schema_violation() -> anyhow::Result<()> {
    let assay = assay_bin();
    let server = assay_mcp_server_bin();

    let tmp = TempDir::new()?;
    let policy_path = tmp.path().join("proxy-policy.yaml");
    let policy_root = tmp.path().join("policy-root");
    std::fs::create_dir_all(&policy_root)?;

    // Proxy policy: schema for read_file must be /workspace/*
    std::fs::write(
        &policy_path,
        r#"
version: "2.0"
name: "e2e-schema"
tools:
  allow: ["read_file"]
schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/workspace/.*"
        minLength: 1
        maxLength: 4096
    required: ["path"]
enforcement:
  unconstrained_tools: deny
"#,
    )?;

    let mut child = Command::new(&assay)
        .args([
            "mcp",
            "wrap",
            "--policy",
            policy_path.to_string_lossy().as_ref(),
            "--",
            server.to_string_lossy().as_ref(),
            "--policy-root",
            policy_root.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut lines = JsonLines::new(stdout);

    // Violating path -> should be denied by schema
    let req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "read_file", "arguments": { "path": "/etc/passwd" } }
    });

    send_line(&mut stdin, &req)?;
    let resp = read_json_line(
        &mut child,
        &mut lines,
        RESPONSE_TIMEOUT,
        "the tools/call response to the schema-violating read_file request",
    )?;

    let code = extract_error_code(&resp).unwrap_or_default();
    assert!(
        code == "E_ARG_SCHEMA" || code == "MCP_ARG_CONSTRAINT",
        "expected schema/constraint error_code, got '{code}'. resp={resp}"
    );

    // Close stdin and reap the proxy instead of killing it: a kill only reaches the `assay`
    // parent, orphaning the wrapped assay-mcp-server, which then races TempDir teardown and fails
    // its own --policy-root canonicalization on a directory that has just been deleted.
    drop(stdin);
    let status = wait_child_with_timeout(&mut child, Duration::from_secs(5))?;
    assert!(status.success(), "proxy exited with status {status}");
    Ok(())
}
