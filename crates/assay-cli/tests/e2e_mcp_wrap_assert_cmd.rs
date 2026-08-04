use anyhow::Context;

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Try to locate a built binary without relying on PATH.
///
/// Priority:
/// 1) Cargo-injected env var: CARGO_BIN_EXE_<name> (with '-' sometimes '_' in env var)
/// 2) {CARGO_TARGET_DIR}/debug/<name>
/// 3) <workspace_root>/target/debug/<name>
fn bin_path(bin: &str) -> anyhow::Result<PathBuf> {
    // Cargo typically uses underscores in env var keys for hyphenated bin names
    let env_key_underscore = format!("CARGO_BIN_EXE_{}", bin.replace('-', "_"));
    let env_key_hyphen = format!("CARGO_BIN_EXE_{bin}");

    if let Ok(p) = std::env::var(&env_key_underscore).or_else(|_| std::env::var(&env_key_hyphen)) {
        return Ok(PathBuf::from(p));
    }

    let target_dir = if let Ok(td) = std::env::var("CARGO_TARGET_DIR") {
        PathBuf::from(td)
    } else {
        // crates/assay-cli -> crates -> workspace root
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest
            .parent()
            .and_then(|p| p.parent())
            .context("failed to resolve workspace root from CARGO_MANIFEST_DIR")?;
        workspace_root.join("target")
    };

    let candidate = target_dir.join("debug").join(exe_name(bin));
    Ok(candidate)
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

#[test]
fn owasp_mcp01_token_args_do_not_leak_to_proxy_logs() -> anyhow::Result<()> {
    let assay = bin_path("assay")?;
    let server = bin_path("assay-mcp-server")?;
    assert!(assay.exists(), "missing binary: {}", assay.display());
    assert!(server.exists(), "missing binary: {}", server.display());

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
    let mut reader = BufReader::new(stdout);
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
    let allowed_resp = read_json_line(&mut reader, Duration::from_secs(5))?;
    send_line(
        &mut stdin,
        &call("token-log-missing", "does-not-exist.yaml"),
    )?;
    let error_resp = read_json_line(&mut reader, Duration::from_secs(5))?;
    drop(stdin);
    let status = wait_child_with_timeout(&mut child, Duration::from_secs(5))?;
    assert!(status.success(), "proxy exited with status {status}");

    // Normal path: the policy was found and evaluated, so redaction below is asserted on a
    // successful tool call and not merely on an early error.
    let allowed_payload = extract_tool_payload(&allowed_resp)?;
    assert_eq!(
        allowed_payload.get("allowed"),
        Some(&Value::Bool(true)),
        "expected the policy to be evaluated and allow the call, got {allowed_resp}"
    );
    assert_eq!(
        allowed_resp
            .get("result")
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "expected a non-error result on the normal path, got {allowed_resp}"
    );

    // Error path (retained): an unresolvable policy still reports E_POLICY_NOT_FOUND.
    let error_payload = extract_tool_payload(&error_resp)?;
    assert_eq!(
        error_payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|v| v.as_str()),
        Some("E_POLICY_NOT_FOUND"),
        "expected E_POLICY_NOT_FOUND on the error path, got {error_resp}"
    );

    // Neither response may echo the token back to the caller either.
    for resp in [&allowed_resp, &error_resp] {
        let rendered = resp.to_string();
        assert!(
            !rendered.contains(secret),
            "response leaked raw token-like argument: {rendered}"
        );
    }

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

    // Both calls must be present in both logs, so the no-leak assertions above cover the normal
    // path and the error path rather than one standing in for the other.
    for id in ["token-log-allowed", "token-log-missing"] {
        assert!(
            audit.contains(id),
            "audit log is missing the record for request {id}: {audit}"
        );
        assert!(
            decisions.contains(id),
            "decision log is missing the record for request {id}: {decisions}"
        );
    }

    Ok(())
}

#[test]
fn e2e_wrap_denies_wildcard_contains() -> anyhow::Result<()> {
    // Ensure binaries exist (nice error if not built)
    let assay = bin_path("assay")?;
    let server = bin_path("assay-mcp-server")?;

    // In CI: run `cargo build --workspace` before tests so these exist.
    assert!(assay.exists(), "missing binary: {}", assay.display());
    assert!(server.exists(), "missing binary: {}", server.display());

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
    let assay = bin_path("assay")?;
    let server = bin_path("assay-mcp-server")?;
    assert!(assay.exists(), "missing binary: {}", assay.display());
    assert!(server.exists(), "missing binary: {}", server.display());

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

    // Close stdin and reap the proxy instead of killing it: a kill only reaches the `assay`
    // parent, orphaning the wrapped assay-mcp-server, which then races TempDir teardown and fails
    // its own --policy-root canonicalization on a directory that has just been deleted.
    drop(stdin);
    let status = wait_child_with_timeout(&mut child, Duration::from_secs(5))?;
    assert!(status.success(), "proxy exited with status {status}");
    Ok(())
}
