use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result};
use serde_json::Value;
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("assay-cli must live below the workspace root")
        .to_path_buf()
}

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn python_executable() -> Result<String> {
    let output = Command::new(python())
        .args(["-c", "import sys; print(sys.executable)"])
        .output()
        .context("failed to resolve the Python executable")?;
    anyhow::ensure!(output.status.success(), "Python executable probe failed");
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn copy_quickstart(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for name in ["run.py", "mock_server.py", "policy.yaml"] {
        fs::copy(source.join(name), destination.join(name))
            .with_context(|| format!("quickstart release asset is missing: {name}"))?;
    }
    Ok(())
}

fn run_quickstart(root: &Path) -> Result<Output> {
    let script = root.join("mcp-quickstart/run.py");
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir)?;
    let binary_name = if cfg!(windows) { "assay.exe" } else { "assay" };
    fs::copy(env!("CARGO_BIN_EXE_assay"), bin_dir.join(binary_name))?;
    let mut paths = vec![bin_dir];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    let path = std::env::join_paths(paths)?;
    Command::new(python())
        .arg(&script)
        .current_dir(root)
        .env("PATH", path)
        .env("ASSAY_BIN", root.join("must-not-be-executed"))
        .env("ASSAY_QUICKSTART_TIMEOUT_SECONDS", "10")
        .output()
        .with_context(|| format!("failed to run {}", script.display()))
}

#[test]
fn released_quickstart_runs_from_an_empty_directory_and_records_what_happened() -> Result<()> {
    let temp = TempDir::new()?;
    let staged = temp.path().join("mcp-quickstart");
    copy_quickstart(&repo_root().join("examples/mcp-quickstart"), &staged)?;

    let output = run_quickstart(temp.path())?;
    assert!(
        output.status.success(),
        "quickstart failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?,
        concat!(
            "assay quickstart: PASS\n",
            "mcp_requests=initialize,tools/list,tools/call\n",
            "decision=allow tool=read_file\n",
            "decision_artifact=.assay/quickstart/decisions.ndjson\n",
            "non_claim=forwarded_to_local_mock_only\n",
        )
    );
    assert_eq!(output.stderr, b"");

    let evidence = temp.path().join(".assay/quickstart");
    let invocation: Value =
        serde_json::from_slice(&fs::read(evidence.join("mock-invocation.json"))?)?;
    let source = staged.canonicalize()?;
    let root = temp.path().canonicalize()?;
    assert_eq!(invocation["cwd"], root.to_string_lossy().as_ref());
    assert_eq!(invocation["argv"], serde_json::json!([]));

    let raw_stdout = fs::read_to_string(evidence.join("mcp.stdout.ndjson"))?;
    assert_eq!(
        raw_stdout,
        concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{\"tools\":{}},\"serverInfo\":{\"name\":\"assay-quickstart-mock\",\"version\":\"1\"}}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[{\"name\":\"read_file\",\"description\":\"Read one file from the local demo directory\",\"inputSchema\":{\"type\":\"object\",\"additionalProperties\":false,\"properties\":{\"path\":{\"type\":\"string\"}},\"required\":[\"path\"]},\"tool_identity\":{\"server_id\":\"default-mcp-server\",\"tool_name\":\"read_file\",\"schema_hash\":\"39b714704935190561ed407980480b9a4a0b346b97346e0bff71fb9ace820194\",\"meta_hash\":\"f8d8a73543bf72a2225321124aa37b5af180214981824cdecb3fa24baccb7f18\"}}]}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"mock-read-ok; no external effect\"}],\"isError\":false}}\n",
        )
    );
    let responses: Vec<Value> = raw_stdout
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_eq!(responses.len(), 3, "expected one response per MCP request");
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[1]["id"], 2);
    assert_eq!(responses[2]["id"], 3);

    let python = python_executable()?;
    let child_args = vec![
        "-u".to_owned(),
        source.join("mock_server.py").display().to_string(),
    ];
    let expected_stderr = format!(
        "[assay] loading policy from {}\n[assay] wrapping command: {} {:?}\n[assay] ALLOW read_file\n",
        source.join("policy.yaml").display(),
        python,
        child_args,
    );
    let raw_stderr = fs::read_to_string(evidence.join("mcp.stderr.txt"))?;
    assert_eq!(raw_stderr, expected_stderr);

    let decisions = fs::read_to_string(evidence.join("decisions.ndjson"))?;
    let rows: Vec<Value> = decisions
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["type"], "assay.tool.decision");
    assert_eq!(rows[0]["data"]["tool"], "read_file");
    assert_eq!(rows[0]["data"]["decision"], "allow");
    Ok(())
}
