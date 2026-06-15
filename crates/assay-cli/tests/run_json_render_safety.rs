//! MCP01a-5 real-command regression: a real `assay run` invocation must write a render-safe
//! `run.json` to disk.
//!
//! The original gap was precisely the difference between the safe `--format json` stdout renderer and
//! the on-disk artifact writer, so a writer-level unit test alone would not have caught it. This drives
//! the actual binary (`CARGO_BIN_EXE_assay`) over a planted trace whose prompt carries a secret + PII,
//! reads the on-disk `run.json`, and asserts the file redacts (raw absent AND marker present) while
//! staying valid JSON.

use std::process::Command;

#[test]
fn assay_run_writes_render_safe_run_json_to_disk() {
    let secret = format!("ghp_{}", "A".repeat(36));
    let email = "alice@example.com";
    let prompt = format!("leak {secret} {email}");

    let dir = tempfile::tempdir().unwrap();
    // The eval input.prompt is the trace-match key, so config and trace carry the identical string.
    let cfg = format!(
        "version: 1\nsuite: rs_witness\nmodel: trace\nsettings:\n  cache: false\ntests:\n  \
         - id: leak_row\n    input:\n      prompt: {p}\n    expected:\n      type: must_contain\n      \
         must_contain: [\"ABSENT_TOKEN_NEVER_PRESENT\"]\n",
        p = serde_json::to_string(&prompt).unwrap()
    );
    std::fs::write(dir.path().join("eval.yaml"), cfg).unwrap();
    let entry = serde_json::json!({
        "schema_version": 1, "type": "assay.trace", "request_id": "rs_0",
        "prompt": prompt, "response": "benign assistant reply, no secret",
        "model": "trace", "provider": "trace", "meta": {}
    });
    std::fs::write(dir.path().join("trace.jsonl"), format!("{entry}\n")).unwrap();

    // run.json is written to the cwd regardless of pass/fail, so run inside the temp dir.
    let status = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args([
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--db",
            ":memory:",
        ])
        .output()
        .expect("failed to run assay binary");
    // The test asserts on the artifact, not the exit code (a failing eval is exit 1 by design).
    let _ = status;

    let run_json = std::fs::read_to_string(dir.path().join("run.json"))
        .expect("assay run did not write run.json");

    // Structure intact + hostile values gone + redaction actually fired (not silently omitted).
    let _: serde_json::Value = serde_json::from_str(&run_json).unwrap();
    assert!(
        !run_json.contains(&secret),
        "real `assay run` leaked the secret to run.json on disk"
    );
    assert!(!run_json.contains("ghp_"));
    assert!(
        !run_json.contains(email),
        "real `assay run` leaked PII to run.json on disk"
    );
    assert!(
        run_json.contains("<redacted:"),
        "run.json shows no redaction marker (field rendered raw or silently omitted)"
    );
    // Non-vacuity: the planted prompt actually reached the rendered field (its benign part survives).
    assert!(
        run_json.contains("leak "),
        "planted prompt did not reach run.json details"
    );
}
