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

#[test]
fn emitted_event_must_fit_the_reader_line_limit() {
    let dir = tempfile::tempdir().unwrap();
    let limit = 16 * 1024 * 1024;
    let source = |n| {
        serde_json::json!({"type":"episode_end","episode_id":"r","timestamp":1,"final_output":"x".repeat(n)}).to_string() + "\n"
    };
    let run = |command, n, output| {
        fs::write(dir.path().join("source"), source(n)).unwrap();
        Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(dir.path())
            .args(["trace", command, "--input", "source", "--output", output])
            .output()
            .unwrap()
    };
    assert!(run("migrate", 0, "empty.jsonl").status.success());
    let overhead = fs::metadata(dir.path().join("empty.jsonl")).unwrap().len() as usize;
    assert!(overhead > source(0).len());
    assert!(run("migrate", limit - overhead, "fits.jsonl")
        .status
        .success());
    assert_eq!(
        fs::metadata(dir.path().join("fits.jsonl")).unwrap().len(),
        limit as u64
    );
    fs::remove_file(dir.path().join("fits.jsonl")).unwrap();
    for size in [limit - overhead + 1, limit - source(0).len()] {
        let result = run("migrate", size, "over.jsonl");
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("output line exceeds 16 MiB"));
        assert!(!dir.path().join("over.jsonl").exists());
    }
}

#[test]
fn migration_refuses_to_replace_recorded_native_digests() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let long = "x".repeat(4600);
    let rows = [
        json!({"type":"step","episode_id":"r","step_id":"s","timestamp":1,"kind":"model","content":long}),
        json!({"type":"tool_call","episode_id":"r","step_id":"s","timestamp":1,"tool_name":"lookup","args":{"query":long}}),
        json!({"type":"tool_call","episode_id":"r","step_id":"s","timestamp":1,"tool_name":"lookup","args":{},"result":long}),
    ];
    for row in rows {
        fs::write(dir.join("source"), row.to_string()).unwrap();
        assert!(trace(dir, "ingest", "short.jsonl").status.success());
        let input = fs::read(dir.join("short.jsonl")).unwrap();
        let mut source = b"{\"prompt\":\"first\",\"response\":\"ok\"}\n".to_vec();
        source.extend_from_slice(&input);
        fs::write(dir.join("source"), &source).unwrap();
        let result = trace(dir, "migrate", "migrated.jsonl");
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("recorded digest"));
        assert!(!dir.join("migrated.jsonl").exists());
        assert_eq!(fs::read(dir.join("source")).unwrap(), source);
    }
    // A newly calculated digest can be kept on a second migration.
    let short = json!({"type":"step","episode_id":"r","step_id":"s","timestamp":1,"kind":"model","content":"ok"});
    fs::write(dir.join("source"), short.to_string()).unwrap();
    assert!(trace(dir, "migrate", "once.jsonl").status.success());
    let first: Value = serde_json::from_slice(&fs::read(dir.join("once.jsonl")).unwrap()).unwrap();
    assert!(first["content_sha256"].is_string());
    fs::write(dir.join("source"), first.to_string()).unwrap();
    assert!(trace(dir, "migrate", "twice.jsonl").status.success());
    let second: Value =
        serde_json::from_slice(&fs::read(dir.join("twice.jsonl")).unwrap()).unwrap();
    assert_eq!(first["content_sha256"], second["content_sha256"]);
}
