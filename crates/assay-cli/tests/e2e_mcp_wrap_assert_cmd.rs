use anyhow::Context;

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tempfile::TempDir;

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
/// dispatching upstream, and pass against any executable that holds the pipe
/// open. Detecting a wrong server in those two needs an assertion only a correct
/// server can satisfy, which is a change to what they test, not to how the
/// binary is found.
///
/// Panics (never skips) if the build fails: a skipped e2e test that reads as
/// green is a worse failure than a loud one.
fn assay_mcp_server_bin() -> PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| build_assay_mcp_server().expect("build assay-mcp-server for e2e wrap tests"))
        .clone()
}

/// Remove the per-crate variables Cargo injects into *this* test process, so the
/// nested build sees the environment a plain shell `cargo build` would give it.
///
/// Without this the nested build is not merely noisy, it is slow on every run:
/// build scripts track these variables, so flipping `CARGO_MANIFEST_DIR` between
/// unset (shell) and `crates/assay-cli` (inherited here) marks units dirty in
/// both directions. Measured on this workspace, the alternation rebuilds
/// `assay-evidence`, `assay-adapter-api`, `assay-core`, `assay-metrics` and
/// `assay-mcp-server`, plus `ring`'s build script and the rustls/reqwest stack
/// above it: ~15s each way, on every run. With the strip, both directions are
/// a sub-second freshness check.
///
/// The list below is the set Cargo documents as per-crate injections, not the
/// set observed in any one runner — several never appear under `cargo test` or
/// `cargo nextest` today and are matched defensively. `CARGO_BIN_EXE_*` is set
/// at runtime by nextest but not by cargo, which is why it is matched by prefix
/// rather than assumed absent.
///
/// Two deliberate exclusions. Variables the *user* set to configure Cargo
/// (`CARGO_HOME`, `CARGO_TARGET_DIR`, `CARGO_NET_OFFLINE`, `RUSTFLAGS`, ...) stay:
/// a shell would pass those through too, and dropping them would change where
/// the build writes or whether it may reach the network. The dynamic-library
/// search path Cargo injects (`LD_LIBRARY_PATH`, and nextest's
/// `NEXTEST_DYLD_FALLBACK_LIBRARY_PATH`) also stays: no build-script fingerprint
/// tracks it, so it costs nothing, and removing it could break linking.
fn strip_cargo_crate_env(cmd: &mut Command) {
    for (key, _) in std::env::vars_os() {
        let Some(key) = key.to_str() else { continue };
        let injected = key.starts_with("CARGO_PKG_")
            || key.starts_with("CARGO_BIN_EXE_")
            || matches!(
                key,
                "CARGO_MANIFEST_DIR"
                    | "CARGO_MANIFEST_PATH"
                    | "CARGO_MANIFEST_LINKS"
                    | "CARGO_CRATE_NAME"
                    | "CARGO_BIN_NAME"
                    | "CARGO_PRIMARY_PACKAGE"
                    | "CARGO_TARGET_TMPDIR"
                    | "OUT_DIR"
            );
        if injected {
            cmd.env_remove(key);
        }
    }
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
/// Falls back to the profile-dir-only reading when the layout is not
/// recognisable (a `build.target-dir` redirect, say). That is the pre-existing
/// behaviour and is correct whenever `--target` is absent, which is every
/// invocation in this repo today.
fn target_and_profile(workspace_root: &Path) -> (Option<String>, String) {
    let bin_exe = Path::new(env!("CARGO_BIN_EXE_assay"));
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
            dir_name(bin_exe.parent()).unwrap_or_else(|| "debug".into()),
        )
    };

    let target_dir = match std::env::var_os("CARGO_TARGET_DIR") {
        Some(d) => PathBuf::from(d),
        None => workspace_root.join("target"),
    };
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

fn build_assay_mcp_server() -> anyhow::Result<PathBuf> {
    // crates/assay-cli -> crates -> workspace root
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .context("failed to resolve workspace root from CARGO_MANIFEST_DIR")?;

    let (target_triple, profile_dir) = target_and_profile(workspace_root);

    let cargo = option_env!("CARGO").unwrap_or("cargo");
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

/// Read one JSON line from stdout with timeout (best-effort).
fn read_json_line(
    reader: &mut BufReader<std::process::ChildStdout>,
    timeout: Duration,
) -> anyhow::Result<Value> {
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            anyhow::bail!("timeout waiting for response");
        }
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            anyhow::bail!("EOF from proxy");
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Ignore log lines if any
        if !line.starts_with('{') {
            continue;
        }
        return Ok(serde_json::from_str::<Value>(line)?);
    }
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
    let mut reader = BufReader::new(stdout);
    let secret = "ghp_assay_fixture_DO_NOT_LEAK_0123456789";

    let req = json!({
        "jsonrpc": "2.0",
        "id": "token-log-fixture",
        "method": "tools/call",
        "params": {
            "name": "assay_check_args",
            "arguments": {
                "tool": "read_file",
                "arguments": {
                    "path": "/workspace/report.md",
                    "authorization": secret
                },
                "policy": "version: \"2.0\"\ntools:\n  allow: [\"read_file\"]\nenforcement:\n  unconstrained_tools: allow\n"
            }
        }
    });

    send_line(&mut stdin, &req)?;
    let _ = read_json_line(&mut reader, Duration::from_secs(5))?;
    drop(stdin);
    let status = wait_child_with_timeout(&mut child, Duration::from_secs(5))?;
    assert!(status.success(), "proxy exited with status {status}");

    let audit = std::fs::read_to_string(&audit_log)?;
    let decisions = std::fs::read_to_string(&decision_log)?;
    assert!(
        !audit.contains(secret),
        "audit log leaked raw token-like argument: {audit}"
    );
    assert!(
        !decisions.contains(secret),
        "decision log leaked raw token-like argument: {decisions}"
    );
    assert!(audit.contains("assay_check_args"));
    assert!(decisions.contains("assay_check_args"));

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
    let mut reader = BufReader::new(stdout);

    // tools/call -> "skill_check" should match *kill* and be denied by proxy
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "skill_check", "arguments": {} }
    });

    send_line(&mut stdin, &req)?;
    let resp = read_json_line(&mut reader, Duration::from_secs(5))?;

    // Accept both transitional codes (old/new) while you converge
    let code = extract_error_code(&resp).unwrap_or_default();
    assert!(
        code == "E_TOOL_DENIED" || code == "MCP_TOOL_DENIED" || code == "E_TOOL_NOT_ALLOWED",
        "expected deny-ish error_code, got '{code}'. resp={resp}"
    );

    let _ = child.kill();
    let _ = child.wait();
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
    let mut reader = BufReader::new(stdout);

    // Violating path -> should be denied by schema
    let req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "read_file", "arguments": { "path": "/etc/passwd" } }
    });

    send_line(&mut stdin, &req)?;
    let resp = read_json_line(&mut reader, Duration::from_secs(5))?;

    let code = extract_error_code(&resp).unwrap_or_default();
    assert!(
        code == "E_ARG_SCHEMA" || code == "MCP_ARG_CONSTRAINT",
        "expected schema/constraint error_code, got '{code}'. resp={resp}"
    );

    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}
