//! `assay_check_sequence` reads a policy file's `sequences` and `aliases`. A file may hold more,
//! `tools` lists for `assay_check_args` and an `on_error`, in one `policy.yaml` that several tools
//! read in parts. The answer names each such field it did not read, so a sequence check that passes
//! does not read as a check of the whole file.

use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

fn spawn_server(policy_root: &Path) -> Conn {
    let child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args([
            "--policy-root",
            policy_root.to_str().expect("utf-8 policy-root"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn assay-mcp-server");
    Conn::attach(child)
}

/// The answer for calling `next_tool` after `history` under a policy file with `contents`.
fn check(contents: &str, history: &[&str], next_tool: &str) -> Value {
    let dir = tempfile::tempdir().expect("policy-root");
    fs::write(dir.path().join("policy.yaml"), contents).expect("write policy");
    let mut conn = spawn_server(dir.path());
    conn.request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "check-sequence-notes", "version": "1.0"}
        }),
        1,
    );
    let resp = conn.request(
        "tools/call",
        json!({
            "name": "assay_check_sequence",
            "arguments": {"history": history, "next_tool": next_tool, "policy": "policy.yaml"}
        }),
        2,
    );
    let _ = conn.shutdown();
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tools/call missing text: {resp}"))
        .to_string();
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
}

fn assert_note(label: &str, body: &Value, needles: &[&str]) {
    let notes: Vec<&str> = body["notes"]
        .as_array()
        .unwrap_or_else(|| panic!("{label}: the answer names nothing; body={body}"))
        .iter()
        .map(|note| note.as_str().expect("note is a string"))
        .collect();
    assert!(
        notes
            .iter()
            .any(|note| needles.iter().all(|needle| note.contains(needle))),
        "{label}: no note mentions {needles:?}; notes={notes:?}"
    );
}

const RULE: &str = "sequences:\n  - { type: require, tool: audit }\n";

#[test]
fn a_tools_list_beside_the_sequences_is_named_with_its_reader() {
    for list in ["deny", "allow"] {
        let body = check(
            &format!("version: \"1.0\"\ntools:\n  {list}: [rm]\n{RULE}"),
            &["audit"],
            "rm",
        );
        assert_eq!(
            body["allowed"], true,
            "{list}: the sequence holds; body={body}"
        );
        assert_note(list, &body, &[&format!("tools.{list}"), "assay_check_args"]);
    }
}

#[test]
fn fields_no_check_reads_are_named() {
    for (field, contents) in [
        (
            "on_error",
            format!("version: \"1.0\"\non_error: block\n{RULE}"),
        ),
        (
            "tools.require_args",
            format!("version: \"1.0\"\ntools:\n  require_args:\n    rm: [path]\n{RULE}"),
        ),
    ] {
        let body = check(&contents, &["audit"], "rm");
        assert_note(field, &body, &[field, "assay_check_sequence"]);
    }
}

/// A refused sequence names the unread fields too: the refusal is not the whole file's answer.
#[test]
fn a_refusal_also_names_what_it_did_not_read() {
    let body = check(
        &format!("version: \"1.0\"\ntools:\n  deny: [rm]\n{RULE}"),
        &[],
        "rm",
    );
    assert_eq!(
        body["allowed"], false,
        "require audit is unmet; body={body}"
    );
    assert_note("refused", &body, &["tools.deny", "assay_check_args"]);
}

/// A file whose every field this check reads gets no notes, and neither does a rule list.
#[test]
fn a_file_it_reads_whole_has_no_notes() {
    for contents in [
        format!("version: \"1.0\"\nname: seq\n{RULE}"),
        format!("version: \"1.0\"\naliases:\n  Audit: [audit]\n{RULE}"),
        "- { type: require, tool: audit }\n".to_string(),
    ] {
        let body = check(&contents, &["audit"], "rm");
        assert!(body.get("notes").is_none(), "{contents}: body={body}");
    }
}
