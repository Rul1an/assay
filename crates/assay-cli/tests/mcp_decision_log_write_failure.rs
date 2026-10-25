//! A decision log that cannot be written is reported, not silently short.
#![cfg(target_os = "linux")]
use assert_cmd::Command;
use serde_json::Value;
use std::{fs, time::Duration};
use tempfile::TempDir;

const SERVER: &str = "import sys,json\nfor line in sys.stdin:\n r=json.loads(line); print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'ok':True}}),flush=True)\n";
const REQUEST: &str = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"any_tool\",\"arguments\":{}}}\n";

#[test]
fn unwritable_decision_log_is_reported_and_proxying_continues() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("policy.yaml"),
        "version: '2.0'\nname: write-failure-test\ntools:\n  allow: [any_tool]\n",
    )
    .unwrap();
    // Every write to /dev/full fails with ENOSPC, as on a full disk.
    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(d.path())
        .timeout(Duration::from_secs(15))
        .args([
            "mcp",
            "wrap",
            "--policy",
            "policy.yaml",
            "--event-source",
            "assay://write-failure-test",
            "--decision-log",
            "/dev/full",
            "--",
            "python3",
            "-c",
            SERVER,
        ])
        .write_stdin(REQUEST)
        .assert()
        .success()
        .stderr(predicates::str::contains("decision log incomplete"))
        .get_output()
        .stdout
        .clone();
    let reply: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(reply["id"], 1);
    assert_eq!(
        reply["result"]["ok"], true,
        "the server reply must still be forwarded"
    );
}
