//! What `assay mcp wrap` writes for a denied call must follow the revision the request declares.
//! MCP 2026-07-28 requires `resultType` on results; earlier revisions must keep their exact bytes.
use assert_cmd::Command;
use serde_json::Value;
use std::{fs, path::Path, time::Duration};
use tempfile::TempDir;

const SERVER: &str = "import sys\nfor line in sys.stdin:\n pass\n";

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn request(meta: &str) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/call\",\"params\":{{\"name\":\"blocked_tool\",\"arguments\":{{}}{meta}}}}}\n"
    )
}

fn modern_meta(version: &str) -> String {
    format!(
        ",\"_meta\":{{\"io.modelcontextprotocol/protocolVersion\":\"{version}\",\"io.modelcontextprotocol/clientCapabilities\":{{}}}}"
    )
}

/// Run one denied call and return the raw deny line plus the one receipt.
fn deny(dir: &Path, line: &str) -> (String, Value) {
    fs::write(
        dir.join("policy.yaml"),
        "version: '2.0'\nname: modern-wire-test\ntools:\n  deny: [blocked_tool]\n",
    )
    .unwrap();
    fs::write(dir.join("server.py"), SERVER).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir)
        .timeout(Duration::from_secs(15))
        .args([
            "mcp",
            "wrap",
            "--policy",
            "policy.yaml",
            "--event-source",
            "assay://modern-wire-test",
            "--decision-log",
            "decisions.jsonl",
            "--request-receipts-out",
            "receipts.jsonl",
            "--",
            python(),
            "-I",
            "server.py",
        ])
        .write_stdin(line)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let receipts = fs::read_to_string(dir.join("receipts.jsonl")).unwrap();
    let receipt = serde_json::from_str(receipts.lines().next().unwrap()).unwrap();
    (String::from_utf8(output.stdout).unwrap(), receipt)
}

#[test]
fn modern_request_gets_a_typed_deny_and_a_receipt_naming_the_revision() {
    let d = TempDir::new().unwrap();
    let (stdout, receipt) = deny(d.path(), &request(&modern_meta("2026-07-28")));
    let response: Value = serde_json::from_str(stdout.trim_end()).unwrap();
    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(receipt["declared_protocol_version_state"], "present");
    assert_eq!(receipt["declared_protocol_version"], "2026-07-28");
}

#[test]
fn later_dated_revision_keeps_the_requirement() {
    let d = TempDir::new().unwrap();
    let (stdout, receipt) = deny(d.path(), &request(&modern_meta("2027-01-15")));
    let response: Value = serde_json::from_str(stdout.trim_end()).unwrap();
    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(receipt["declared_protocol_version"], "2027-01-15");
}

/// The expected bytes are spelled out: comparing against a second run would pass if both changed.
#[test]
fn earlier_absent_and_malformed_declarations_keep_the_exact_legacy_bytes() {
    const LEGACY: &str = "{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"Content blocked by policy\"}],\"isError\":true,\"structuredContent\":";
    for (meta, state, version) in [
        (String::new(), "absent", Value::Null),
        (
            modern_meta("2025-11-25"),
            "present",
            Value::from("2025-11-25"),
        ),
        (modern_meta("2026-99-99"), "malformed", Value::Null),
        (",\"_meta\":7".to_string(), "malformed", Value::Null),
        (
            ",\"_meta\":{\"io.modelcontextprotocol/protocolVersion\":20260728}".to_string(),
            "malformed",
            Value::Null,
        ),
    ] {
        let d = TempDir::new().unwrap();
        let (stdout, receipt) = deny(d.path(), &request(&meta));
        assert!(stdout.starts_with(LEGACY), "{meta}: {stdout}");
        assert!(!stdout.contains("resultType"), "{meta}: {stdout}");
        assert_eq!(receipt["declared_protocol_version_state"], state, "{meta}");
        assert_eq!(receipt["declared_protocol_version"], version, "{meta}");
    }
}
