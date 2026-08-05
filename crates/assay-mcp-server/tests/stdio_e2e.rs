use serde_json::Value;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

#[test]
fn test_stdio_flow() {
    let policy_root = "../../tests/fixtures/mcp"; // Relative to crates/assay-mcp-server CWD

    // Cargo builds this crate's bin target before running its integration tests and hands us the
    // path, so there is nothing to build here. Spawning `cargo run` instead would inherit the
    // Cargo environment, whose CARGO_MANIFEST_DIR is tracked by ring's build script and so marks
    // the whole rustls/reqwest stack dirty on every alternation with a shell build.
    let child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args(["--policy-root", policy_root])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn server");

    // No startup allowance: the binary is already built, so the first exchange is as fast as the
    // rest. The longer budget this used to carry existed only because `cargo run` could still be
    // compiling.
    let mut conn = Conn::attach(child);

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
    conn.send(req_init);

    let resp: Value = conn.read_json();
    assert!(resp.get("result").is_some(), "Init failed: {:?}", resp);

    // The unit tests in `server.rs` pin `initialize_result()`, but the original defect was an
    // inline literal at the call site, so a helper the dispatcher no longer returns would leave
    // those tests green. This is the same boundary checked where it actually matters: on the
    // wire, out of a real handshake against a real process.
    let result = &resp["result"];
    let wire = serde_json::to_string(result).expect("serializable");
    for forbidden in [
        "certified",
        "certification",
        "partner",
        "compliant",
        "compliance",
        "approved",
        "endorsed",
        "accredited",
    ] {
        assert!(
            !wire.to_ascii_lowercase().contains(forbidden),
            "initialize asserted `{forbidden}` on the wire without a checkable basis: {wire}"
        );
    }
    assert!(
        result.get("meta").is_none(),
        "bare `meta` key returned on the wire: {wire}"
    );
    assert_eq!(
        result["serverInfo"]["name"].as_str(),
        Some("assay-mcp-server")
    );
    assert_eq!(
        result["serverInfo"]["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "wire version must be the crate version, not a hand-written literal"
    );

    // 2. List Tools
    let req_list = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "params": {},
        "id": 2
    });
    conn.send(req_list);

    let resp: Value = conn.read_json();
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
    conn.send(req_check_args);

    let resp: Value = conn.read_json();
    let content_text = resp["result"]["content"][0]["text"]
        .as_str()
        .expect("Missing content text in MCP response");
    let tool_res: Value = serde_json::from_str(content_text).unwrap();
    assert_eq!(tool_res["allowed"], true);

    // 4. Shut the server down: stdin EOF, then a bounded reap.
    let _ = conn.shutdown();
}
