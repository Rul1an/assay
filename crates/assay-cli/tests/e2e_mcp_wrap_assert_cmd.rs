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
/// assay-mcp-server` and asserted the path existed, which is wrong in both
/// directions: it fails on a tree that has simply never built the server (reading
/// as a test failure rather than a missing build step), and it silently *passes*
/// against a stale binary left over from an older tree, so the e2e suite reads
/// green while exercising code that no longer exists.
///
/// Instead, ask Cargo to build it and to report where it put it. Cargo owns the
/// staleness computation, so this also covers changes in transitive dependencies
/// such as `assay-core` that an mtime comparison against `assay-mcp-server/src`
/// would miss. On an already-current tree this is a no-op freshness check.
///
/// Panics (never skips) if the build fails: a skipped e2e test that reads as
/// green is precisely the failure mode this replaces.
fn assay_mcp_server_bin() -> PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| build_assay_mcp_server().expect("build assay-mcp-server for e2e wrap tests"))
        .clone()
}

/// Remove the per-crate variables Cargo injects into *this* test process, so the
/// nested build sees the environment a plain shell `cargo build` would give it.
///
/// Without this the nested build is not just noisy, it is slow on every run:
/// dependency build scripts track these variables, and `ring` in particular goes
/// dirty when `CARGO_MANIFEST_DIR` flips between unset (shell) and
/// `crates/assay-cli` (inherited here). That drags the whole rustls/reqwest stack
/// with it — ~13s of rebuild per alternation, in both directions.
///
/// Variables the *user* set to configure Cargo (`CARGO_HOME`, `CARGO_TARGET_DIR`,
/// `CARGO_NET_OFFLINE`, `RUSTFLAGS`, ...) are deliberately left alone: a shell
/// would pass those through too, and dropping them would change where the build
/// writes or whether it may reach the network.
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

fn build_assay_mcp_server() -> anyhow::Result<PathBuf> {
    // crates/assay-cli -> crates -> workspace root
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .context("failed to resolve workspace root from CARGO_MANIFEST_DIR")?;

    // Build into the same profile directory the test itself was built into, so a
    // `cargo test --release` run does not trigger a second full dependency build.
    // `CARGO_BIN_EXE_assay` is `<target-dir>/<profile-dir>/assay`; the profile dir
    // and the profile name coincide for every profile except dev, whose directory
    // is `debug`.
    let profile_dir = Path::new(env!("CARGO_BIN_EXE_assay"))
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|p| p.to_str())
        .context("failed to resolve profile directory from CARGO_BIN_EXE_assay")?;

    let cargo = option_env!("CARGO").unwrap_or("cargo");
    let mut cmd = Command::new(cargo);
    cmd.current_dir(workspace_root).args([
        "build",
        "-p",
        "assay-mcp-server",
        "--bin",
        "assay-mcp-server",
    ]);
    if profile_dir != "debug" {
        cmd.args(["--profile", profile_dir]);
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
