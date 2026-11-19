use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

fn trace(dir: &Path, cmd: &str, output: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir)
        .args(["trace", cmd, "--input", "source", "--output", output])
        .output()
        .unwrap()
}

#[test]
fn migration_is_explicit_and_preserves_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let source = dir.join("source");
    let mut row = json!({"type":"assay.trace","schema_version":2,"request_id":"id","prompt":"P","response":"ok"});
    fs::write(&source, row.to_string()).unwrap();
    assert!(trace(dir, "ingest", "default.jsonl").status.success());
    assert!(!trace(dir, "migrate", "events.jsonl").status.success());
    assert!(!dir.join("events.jsonl").exists());
    row.as_object_mut().unwrap().remove("request_id");
    row["schema_version"] = json!(1);
    fs::write(&source, row.to_string()).unwrap();
    assert!(!trace(dir, "ingest", "default.jsonl").status.success());
    let migrated = trace(dir, "migrate", "events.jsonl");
    assert!(
        migrated.status.success(),
        "{}",
        String::from_utf8_lossy(&migrated.stderr)
    );
    let old = fs::read_to_string(dir.join("events.jsonl")).unwrap();
    let events: Vec<Value> = old
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(events.len(), 3);
    assert!(events[0]["episode_id"]
        .as_str()
        .unwrap()
        .starts_with("assay:legacy:sha256:"));
    assert!(events[2]["outcome"].is_null());
    fs::write(&source, r#"{"tool":"lookup","result":"answer"}"#).unwrap();
    assert!(!trace(dir, "migrate", "events.jsonl").status.success());
    assert_eq!(fs::read_to_string(dir.join("events.jsonl")).unwrap(), old);
    assert!(!trace(dir, "migrate", "source").status.success());
    fs::write(&source, format!("{}\ninvalid", row)).unwrap();
    assert!(!trace(dir, "migrate", "partial.jsonl").status.success());
    assert!(!dir.join("partial.jsonl").exists());
}

#[test]
fn strict_ci_keeps_its_six_x_admission() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("eval.yaml"), "configVersion: 1\nsuite: test\nmodel: trace\ntests:\n- id: answer\n  input: P\n  expected: {type: must_contain, must_contain: [ok]}\n").unwrap();
    for (input, code) in [
        (
            json!({"type":"assay.trace","schema_version":2,"request_id":"r","prompt":"P","response":"ok"}),
            0,
        ),
        (json!({"prompt":"P","response":"ok"}), 2),
    ] {
        fs::write(dir.path().join("source"), input.to_string()).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(dir.path())
            .args([
                "ci",
                "--replay-strict",
                "--trace-file",
                "source",
                "--db",
                ":memory:",
                "--no-cache",
            ])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn tool_shorthand_and_mixed_identity_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let source = dir.join("source");
    let raw = r#"{"tool":"lookup","result":"answer"}"#;
    fs::write(&source, raw).unwrap();
    assert!(trace(dir, "migrate", "replay.jsonl").status.success());
    let output = fs::read_to_string(dir.join("replay.jsonl")).unwrap();
    let events: Vec<Value> = output
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(events[0]["input"]["prompt"], "ignore");
    assert_eq!(events[0]["meta"]["tool_calls"][0]["tool_name"], "lookup");
    assert_eq!(events[1]["kind"], "legacy_replay");
    assert_eq!(events[1]["content"], "answer");
    let native = json!({"type":"step","episode_id":"other","step_id":events[1]["step_id"],"timestamp":1,"kind":"model"});
    for rows in [format!("{raw}\n{native}"), format!("{native}\n{raw}")] {
        fs::write(&source, rows).unwrap();
        let result = trace(dir, "migrate", "collision.jsonl");
        assert!(!result.status.success());
        assert!(!dir.join("collision.jsonl").exists());
    }
}

#[test]
fn migrated_literal_remains_available_in_the_stored_episode_graph() -> anyhow::Result<()> {
    use assay_core::storage::Store;
    use assay_core::trace::schema::TraceEvent;
    let dir = tempfile::tempdir()?;
    let literal = r#"{"completion":"inner","model":"embedded"}"#;
    let row = json!({"prompt":"P","request_id":"r","response":literal});
    fs::write(dir.path().join("source"), row.to_string())?;
    let result = trace(dir.path(), "migrate", "events.jsonl");
    assert!(result.status.success());
    let encoded = fs::read_to_string(dir.path().join("events.jsonl"))?;
    let events = encoded
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<Vec<TraceEvent>, _>>()?;
    let store = Store::memory()?;
    store.init_schema()?;
    let run = store.insert_run("suite")?;
    store.insert_batch(&events, Some(run), Some("test"))?;
    assert_eq!(
        store.get_episode_graph(run, "test")?.steps[0]
            .content
            .as_deref(),
        Some(literal)
    );
    Ok(())
}

#[test]
fn migration_refuses_new_truncation_before_publication() {
    let dir = tempfile::tempdir().unwrap();
    let tail = format!("{}KEEP_TAIL", "a".repeat(4500));
    for row in [
        json!({"type":"assay.trace","schema_version":1,"request_id":"r","prompt":"P","response":tail}),
        json!({"prompt":tail,"response":"ok"}),
        json!({"type":"step","episode_id":"r","step_id":"s","timestamp":1,"kind":"model","content":tail}),
    ] {
        fs::write(dir.path().join("source"), row.to_string()).unwrap();
        let result = trace(dir.path(), "migrate", "output.jsonl");
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("would truncate"));
        assert!(!dir.path().join("output.jsonl").exists());
    }
}
