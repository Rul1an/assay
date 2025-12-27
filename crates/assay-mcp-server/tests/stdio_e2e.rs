use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn test_stdio_flow() {
    let policy_root = "../../tests/fixtures/mcp"; // Relative to crates/assay-mcp-server CWD

    // Ensure binary is built
    let status = Command::new("cargo")
        .args(&["build", "-p", "assay-mcp-server"])
        .status()
        .expect("Failed to build server");
    assert!(status.success());

    // Spawn server
    let mut child = Command::new("cargo")
        .args(&[
            "run",
            "-q",
            "-p",
            "assay-mcp-server",
            "--",
            "--policy-root",
            policy_root,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn server");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // Initial log line (Assay MCP Server starting...) - stderr?
    // main.rs uses eprintln! so it goes to stderr (inherited).
    // Stdout should be pure JSON-RPC.

    // 1. Initialize
    let req_init = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "1.0"} },
        "id": 1
    });
    writeln!(stdin, "{}", req_init).unwrap();

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("Failed to read init response");
    if line.trim().is_empty() {
        reader
            .read_line(&mut line)
            .expect("Failed to read init response (retry)");
    }

    let resp: Value = serde_json::from_str(&line).expect("Failed to parse init response");
    assert!(resp.get("result").is_some(), "Init failed: {:?}", resp);

    // 2. List Tools
    let req_list = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "params": {},
        "id": 2
    });
    writeln!(stdin, "{}", req_list).unwrap();

    line.clear();
    reader.read_line(&mut line).unwrap();
    let resp: Value = serde_json::from_str(&line).unwrap();
    let tools = resp["result"]["tools"]
        .as_array()
        .expect("Tools list missing");
    assert!(tools.iter().any(|t| t["name"] == "assay_check_args"));
    assert!(tools.iter().any(|t| t["name"] == "assay_check_sequence"));
    assert!(tools.iter().any(|t| t["name"] == "assay_policy_decide"));

    // 3. Call check_args (Valid)
    let req_check_args = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": {
            "name": "assay_check_args",
            "arguments": {
                "tool": "discount_tool",
                "arguments": { "percent": 10 },
                "policy": "policy.yaml"
            }
        },
        "id": 3
    });
    writeln!(stdin, "{}", req_check_args).unwrap();

    line.clear();
    reader.read_line(&mut line).unwrap();
    let resp: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(resp["result"]["allowed"], true);

    // 4. Kill server
    // Dropping stdin typically signals EOF, but we can kill explicitly.
    drop(stdin);
    let _ = child.wait();
}
