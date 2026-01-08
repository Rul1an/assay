use anyhow::Context;
use assert_cmd::prelude::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
fn read_json_line(reader: &mut BufReader<std::process::ChildStdout>, timeout: Duration) -> anyhow::Result<Value> {
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

fn extract_structured_contract(resp: &Value) -> Option<&Value> {
    resp.get("result")
        .and_then(|r| r.get("structuredContent").or_else(|| r.get("structured_content")))
        .or_else(|| {
            resp.get("payload")
                .and_then(|p| p.get("result"))
                .and_then(|r| r.get("structuredContent").or_else(|| r.get("structured_content")))
        })
}

fn extract_error_code(resp: &Value) -> Option<String> {
    extract_structured_contract(resp)
        .and_then(|c| c.get("error_code"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
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

    let _ = child.kill();
    let _ = child.wait();
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

    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}
