use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub(crate) const PROXY_DENIED: i64 = -32042;

pub(crate) const ALLOW_ACME: &str = r#"
caller:
  id: "ci-agent"
upstream_credential:
  alias: "gh-deploy"
  scopes: ["repo:deploy_key:write"]
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

pub(crate) const ALLOW_ACME_INSUFFICIENT_CRED: &str = r#"
caller:
  id: "ci-agent"
upstream_credential:
  alias: "gh-ro"
  scopes: ["repo:read"]
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

pub(crate) const ALLOW_ACME_NO_CRED: &str = r#"
caller:
  id: "ci-agent"
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

pub(crate) const DEPLOY_KEY_CALL_ID: i64 = 9;

pub(crate) fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

pub(crate) fn mock_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proxy/mock_upstream.py")
}

/// The committed P60a per-tool baseline. Its `github.add_deploy_key` tool_digest equals what the mock's
/// `p60a` mode produces (the same canonical tools), so it is the approved baseline for the happy path.
pub(crate) fn approved_baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp_manifest_drift/declared_per_tool_baseline.json")
}

pub(crate) fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, content).expect("write");
    p
}

/// Spawn the enforcing proxy with a policy + baseline and the given mock mode.
pub(crate) fn spawn_enforce(log: &Path, policy: &Path, baseline: &Path, mode: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args([
            "proxy-enforce",
            "--upstream-command",
            python(),
            "--upstream-arg",
            "-u",
            "--upstream-arg",
            mock_path().to_str().unwrap(),
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            baseline.to_str().unwrap(),
        ])
        .env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy (is python installed?)")
}

/// Spawn the enforcing proxy that also writes the per-call enforcement-decision NDJSON (P61e-d).
pub(crate) fn spawn_enforce_with_decisions(
    log: &Path,
    policy: &Path,
    baseline: &Path,
    mode: &str,
    decisions: &Path,
) -> Child {
    Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args([
            "proxy-enforce",
            "--upstream-command",
            python(),
            "--upstream-arg",
            "-u",
            "--upstream-arg",
            mock_path().to_str().unwrap(),
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            baseline.to_str().unwrap(),
            "--enforcement-decision-out",
            decisions.to_str().unwrap(),
        ])
        .env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy (is python installed?)")
}

/// Spawn the enforcing proxy writing BOTH the enforcement-decision NDJSON and the manifest-establish
/// carrier NDJSON, with an optional establish budget (ms).
pub(crate) fn spawn_enforce_recording(
    log: &Path,
    policy: &Path,
    baseline: &Path,
    mode: &str,
    decisions: &Path,
    establish: &Path,
    budget_ms: Option<u64>,
) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    cmd.arg("proxy-enforce")
        .args(["--upstream-command", python()])
        .args(["--upstream-arg", "-u"])
        .args(["--upstream-arg", mock_path().to_str().unwrap()])
        .args(["--enforce-policy", policy.to_str().unwrap()])
        .args(["--declared-mcp-manifest", baseline.to_str().unwrap()])
        .args(["--enforcement-decision-out", decisions.to_str().unwrap()])
        .args(["--manifest-establish-out", establish.to_str().unwrap()]);
    if let Some(ms) = budget_ms {
        cmd.args(["--manifest-establish-budget-ms", &ms.to_string()]);
    }
    cmd.env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy (is python installed?)")
}

/// Spawn the enforcing proxy writing the enforcement-decision NDJSON and, optionally, the
/// tool-annotation conformance carrier NDJSON, with an optional establish budget (ms).
pub(crate) fn spawn_enforce_conformance(
    log: &Path,
    policy: &Path,
    baseline: &Path,
    mode: &str,
    decisions: &Path,
    conformance: Option<&Path>,
    budget_ms: Option<u64>,
) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    cmd.arg("proxy-enforce")
        .args(["--upstream-command", python()])
        .args(["--upstream-arg", "-u"])
        .args(["--upstream-arg", mock_path().to_str().unwrap()])
        .args(["--enforce-policy", policy.to_str().unwrap()])
        .args(["--declared-mcp-manifest", baseline.to_str().unwrap()])
        .args(["--enforcement-decision-out", decisions.to_str().unwrap()]);
    if let Some(c) = conformance {
        cmd.args(["--tool-conformance-out", c.to_str().unwrap()]);
    }
    if let Some(ms) = budget_ms {
        cmd.args(["--manifest-establish-budget-ms", &ms.to_string()]);
    }
    cmd.env("MOCK_UPSTREAM_LOG", log)
        .env("MOCK_UPSTREAM_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforce proxy (is python installed?)")
}

pub(crate) fn send(stdin: &mut ChildStdin, v: Value) {
    writeln!(stdin, "{v}").expect("write");
    stdin.flush().expect("flush");
}

pub(crate) fn read_response(reader: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).expect("read");
        assert!(n > 0, "proxy closed stdout before responding");
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(t).expect("parse JSON");
        if v.get("method").is_some() {
            continue;
        }
        return v;
    }
}

pub(crate) fn init() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}
    })
}

pub(crate) fn read_methods(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

pub(crate) fn shutdown(mut child: Child, stdin: ChildStdin) {
    drop(stdin);
    let _ = child.wait();
}

/// init -> tools/call (NO tools/list first), with the approved baseline. Returns the deny reason and
/// the upstream method log. Asserts proxy_denied and that tools/call never reached the upstream.
pub(crate) fn deny_reason_for(policy_yaml: &str, call_params: Value) -> (String, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("methods.log");
    let policy = write_file(dir.path(), "enforce.yaml", policy_yaml);
    let mut child = spawn_enforce(&log, &policy, &approved_baseline_path(), "normal");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, init());
    let _ = read_response(&mut out);
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": call_params}),
    );
    let r = read_response(&mut out);
    assert_eq!(r["id"], 9);
    assert_eq!(
        r["error"]["code"], PROXY_DENIED,
        "every deny outcome is proxy_denied; got {r}"
    );
    assert_eq!(r["error"]["data"]["origin"], "assay-proxy");
    let reason = r["error"]["data"]["reason"]
        .as_str()
        .expect("reason string")
        .to_string();

    shutdown(child, stdin);
    let methods = read_methods(&log);
    assert!(
        !methods.contains(&"tools/call".to_string()),
        "INVARIANT: a denied tools/call must never reach the upstream: {methods:?}"
    );
    (reason, methods)
}

pub(crate) fn deploy_key_call() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": DEPLOY_KEY_CALL_ID, "method": "tools/call",
        "params": {"name": "github.add_deploy_key", "arguments": {"owner": "acme", "repo": "prod-app"}}
    })
}

/// Read every remaining stdout line to EOF (used to prove nothing leaked to the client).
pub(crate) fn drain_stdout(reader: &mut BufReader<ChildStdout>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut buf = String::new();
    loop {
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let t = buf.trim();
                if !t.is_empty() {
                    lines.push(t.to_string());
                }
            }
        }
    }
    lines
}

pub(crate) fn read_establish_records(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("establish record JSON"))
        .collect()
}

pub(crate) fn read_conformance_records(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("conformance record JSON"))
        .collect()
}

/// The carrier carries no raw scope/target/token/credential/caller/args — journey only.
pub(crate) fn assert_no_secrets(rec: &Value) {
    let s = serde_json::to_string(rec).unwrap();
    for forbidden in [
        "target_digest",
        "scope",
        "token",
        "credential",
        "caller",
        "arguments",
        "owner",
        "repo",
    ] {
        assert!(
            !s.contains(forbidden),
            "manifest_establish record must not carry `{forbidden}`: {s}"
        );
    }
    // Exactly the five v0 fields, nothing more.
    let obj = rec.as_object().expect("record is an object");
    let mut keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "action_class",
            "establish_attempted",
            "establish_path",
            "run_outcome",
            "schema"
        ]
    );
}

pub(crate) fn startup_status(args: &[&str]) -> std::process::ExitStatus {
    Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn")
}

/// The fixed upstream-command args shared by the startup-failure cases.
pub(crate) fn upstream_args() -> Vec<String> {
    vec![
        "proxy-enforce".into(),
        "--upstream-command".into(),
        python().into(),
        "--upstream-arg".into(),
        "-u".into(),
        "--upstream-arg".into(),
        mock_path().to_str().unwrap().into(),
    ]
}

pub(crate) fn run_startup(extra: &[&str]) -> std::process::ExitStatus {
    let mut args = upstream_args();
    for e in extra {
        args.push((*e).into());
    }
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    startup_status(&arg_refs)
}
