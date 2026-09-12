//! ADR-050 required tests. Each names the production mutation it kills.
//!
//! Test 12 (Wave 0 semver, no major for assay-core) is cargo semver-checks,
//! not a rustc test; the PR body records its output.

use assay_core::mcp::{mcp_events_to_v2_trace, parse_mcp_transcript, McpInputFormat};
use assay_core::storage::store::Store;
use assay_core::trace::observation::{
    bound_sha256, parse_observed_line, read_observed, read_truncation, tool_call_target_key,
    ObservedTraceEvent, TruncationObservation, TruncationReading, UPGRADER_STAGE,
};
use assay_core::trace::otel_ingest::{convert_spans_to_episodes, OtelSpan};
use assay_core::trace::schema::{
    EpisodeStart, StepEntry, ToolCallEntry, TraceEvent, TruncationMeta,
};
use assay_core::trace::truncation::{compute_sha256_str, INGEST_STRING_CEILING};
use assay_core::trace::upgrader::StreamUpgrader;
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Cursor;

const TRUSTED: &[&str] = &[UPGRADER_STAGE];

fn meta_loss(field: &str, original_len: usize, kept_len: usize, sha: &str) -> TruncationMeta {
    TruncationMeta {
        field: field.into(),
        original_len,
        kept_len,
        sha256: sha.into(),
        strategy: "head".into(),
    }
}

fn short_step(truncations: Vec<TruncationMeta>) -> StepEntry {
    StepEntry {
        episode_id: "e1".into(),
        step_id: "s1".into(),
        idx: 0,
        timestamp: 1,
        kind: "llm_completion".into(),
        name: Some("model".into()),
        content: Some("hello".into()),
        content_sha256: Some("abc".into()),
        truncations,
        meta: Value::Null,
    }
}

fn observed_step(
    truncations: Vec<TruncationMeta>,
    observation: TruncationObservation,
) -> ObservedTraceEvent {
    ObservedTraceEvent::new(TraceEvent::Step(short_step(truncations)), vec![observation])
}

fn clean_observation(scope: &[&str], losses: Vec<TruncationMeta>) -> TruncationObservation {
    TruncationObservation::upgrader(scope.iter().map(|s| (*s).to_string()).collect(), losses)
}

fn line_step(observations: Option<Value>, truncations: Value) -> String {
    let mut v = json!({
        "type": "step",
        "episode_id": "e1",
        "step_id": "s1",
        "idx": 0,
        "timestamp": 1,
        "kind": "llm_completion",
        "name": "model",
        "content": "hello",
        "content_sha256": "abc",
        "truncations": truncations,
        "meta": null
    });
    if let Some(obs) = observations {
        v.as_object_mut()
            .expect("object")
            .insert("observations".into(), obs);
    }
    v.to_string()
}

fn upgrade_observed(jsonl: &str) -> ObservedTraceEvent {
    StreamUpgrader::new(Cursor::new(jsonl))
        .observed()
        .next()
        .expect("one event")
        .expect("parse")
}

fn ensure_episode(store: &Store, episode_id: &str) -> anyhow::Result<()> {
    store.insert_event(
        &TraceEvent::EpisodeStart(EpisodeStart {
            episode_id: episode_id.into(),
            timestamp: 1,
            input: json!({"prompt": "hi"}),
            meta: Value::Null,
        }),
        None,
        None,
    )
}

/// Frozen copy of `storage::schema::DDL` before this change. Test 9 runs it against
/// a database that already holds `trace_observations`.
const PRE_CHANGE_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  suite TEXT NOT NULL,
  started_at TEXT NOT NULL,
  status TEXT NOT NULL,
  config_json TEXT
);
CREATE TABLE IF NOT EXISTS results (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id INTEGER NOT NULL REFERENCES runs(id),
  test_id TEXT NOT NULL,
  outcome TEXT NOT NULL,
  score REAL,
  duration_ms INTEGER,
  attempts_json TEXT,
  output_json TEXT,
  fingerprint TEXT,
  skip_reason TEXT
);
CREATE TABLE IF NOT EXISTS attempts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  result_id INTEGER NOT NULL REFERENCES results(id),
  attempt_number INTEGER NOT NULL,
  outcome TEXT NOT NULL,
  score REAL,
  duration_ms INTEGER,
  output_json TEXT,
  error_message TEXT
);
CREATE TABLE IF NOT EXISTS quarantine (
  suite TEXT NOT NULL,
  test_id TEXT NOT NULL,
  reason TEXT,
  added_at TEXT NOT NULL,
  PRIMARY KEY (suite, test_id)
);
CREATE TABLE IF NOT EXISTS cache (
  key TEXT PRIMARY KEY,
  response_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS embeddings (
  key TEXT PRIMARY KEY,
  model TEXT NOT NULL,
  dims INTEGER NOT NULL,
  vec BLOB NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS judge_cache (
  key TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  rubric_id TEXT NOT NULL,
  rubric_version TEXT NOT NULL,
  created_at TEXT NOT NULL,
  payload_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS episodes (
    id TEXT PRIMARY KEY,
    run_id INTEGER,
    test_id TEXT,
    timestamp INTEGER NOT NULL,
    prompt TEXT,
    outcome TEXT,
    meta_json TEXT,
    FOREIGN KEY(run_id) REFERENCES runs(id)
);
CREATE TABLE IF NOT EXISTS steps (
    id TEXT PRIMARY KEY,
    episode_id TEXT NOT NULL,
    idx INTEGER NOT NULL,
    kind TEXT,
    name TEXT,
    content TEXT,
    content_sha256 TEXT,
    truncations_json TEXT,
    meta_json TEXT,
    FOREIGN KEY(episode_id) REFERENCES episodes(id),
    UNIQUE(episode_id, idx)
);
CREATE TABLE IF NOT EXISTS tool_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    step_id TEXT NOT NULL,
    episode_id TEXT NOT NULL,
    tool_name TEXT,
    call_index INTEGER,
    args TEXT,
    args_sha256 TEXT,
    result TEXT,
    result_sha256 TEXT,
    error TEXT,
    truncations_json TEXT,
    meta_json TEXT,
    FOREIGN KEY(step_id) REFERENCES steps(id),
    UNIQUE(step_id, call_index)
);
CREATE INDEX IF NOT EXISTS idx_steps_episode ON steps(episode_id, idx);
CREATE INDEX IF NOT EXISTS idx_tool_calls_episode ON tool_calls(episode_id);
CREATE INDEX IF NOT EXISTS idx_tool_calls_step ON tool_calls(step_id);
"#;

// --- Test 1 ---
// Mutation: the reader returns MeasuredClean when no observation covers the field.
// Control: the same line with a trusted empty-loss observation reads MeasuredClean.

#[test]
fn test_01_absent_observation_reads_unmeasured() -> anyhow::Result<()> {
    let no_key = line_step(None, json!([]));
    let parsed = parse_observed_line(&no_key)?;
    assert_eq!(
        read_observed(&parsed, "/content", TRUSTED),
        TruncationReading::Unmeasured,
        "missing observations key must not read clean"
    );

    let explicit_trunc = line_step(None, json!([]));
    let parsed = parse_observed_line(&explicit_trunc)?;
    assert_eq!(
        read_observed(&parsed, "/content", TRUSTED),
        TruncationReading::Unmeasured,
        "truncations: [] without an observation must not read clean"
    );

    let store = Store::memory()?;
    store.init_schema()?;
    ensure_episode(&store, "e1")?;
    store.insert_event(&TraceEvent::Step(short_step(vec![])), None, None)?;
    assert_eq!(
        store.read_truncation("step", "s1", "/content", TRUSTED)?,
        TruncationReading::Unmeasured,
        "sqlite step with truncations_json=[] and no observation row must not read clean"
    );

    let control = observed_step(vec![], clean_observation(&["/content", "/meta"], vec![]));
    assert_eq!(
        read_observed(&control, "/content", TRUSTED),
        TruncationReading::MeasuredClean {
            stage: UPGRADER_STAGE.into(),
            ceiling: INGEST_STRING_CEILING,
        },
        "trusted empty-loss observation is the clean control"
    );
    Ok(())
}

// --- Test 2 ---
// Mutation: an empty list is treated as one empty-loss observation.

#[test]
fn test_02_empty_observations_list_reads_unmeasured() -> anyhow::Result<()> {
    let line = line_step(Some(json!([])), json!([]));
    let parsed = parse_observed_line(&line)?;
    assert_eq!(
        read_observed(&parsed, "/content", TRUSTED),
        TruncationReading::Unmeasured,
        "observations: [] must read unmeasured"
    );
    Ok(())
}

// --- Test 3 ---
// Mutation (reader): consult observation losses only.
// Mutation (producer): build observation losses from a second call or an empty Vec.

#[test]
fn test_03_reported_loss_dominates_and_producer_parity() -> anyhow::Result<()> {
    let loss = meta_loss("/content", 4595, 4096, "deadbeef");
    let obs = clean_observation(&["/content", "/meta"], vec![]);
    let observed = observed_step(vec![loss.clone()], obs);
    assert_eq!(
        read_observed(&observed, "/content", TRUSTED),
        TruncationReading::Lossy,
        "loss in truncations with observation omitting it must stay lossy"
    );

    let over = "x".repeat(INGEST_STRING_CEILING + 499);
    let line = json!({
        "type": "step",
        "episode_id": "e1",
        "step_id": "s1",
        "idx": 0,
        "timestamp": 1,
        "kind": "llm_completion",
        "name": "model",
        "content": over,
        "truncations": [],
        "meta": null
    })
    .to_string();
    let produced = upgrade_observed(&line);
    let TraceEvent::Step(step) = produced.event() else {
        panic!("expected step");
    };
    let appended: Vec<_> = step
        .truncations
        .iter()
        .filter(|t| t.field == "/content")
        .cloned()
        .collect();
    assert!(!appended.is_empty(), "upgrader must append content loss");
    let obs_losses = &produced.observations()[0].losses;
    assert_eq!(
        obs_losses, &step.truncations,
        "observation losses must equal the entries appended to truncations"
    );
    Ok(())
}

// --- Test 4 ---
// Mutation: the trust check is skipped.

#[test]
fn test_04_unnamed_or_untrusted_stage_reads_unmeasured() -> anyhow::Result<()> {
    let foreign = TruncationObservation::new(
        "other.stage",
        INGEST_STRING_CEILING,
        vec!["/content".into(), "/meta".into()],
        vec![],
    );
    let observed = observed_step(vec![], foreign);
    assert_eq!(
        read_observed(&observed, "/content", TRUSTED),
        TruncationReading::Unmeasured
    );

    let empty_stage = TruncationObservation::new(
        "",
        INGEST_STRING_CEILING,
        vec!["/content".into(), "/meta".into()],
        vec![],
    );
    let observed = observed_step(vec![], empty_stage);
    assert_eq!(
        read_observed(&observed, "/content", TRUSTED),
        TruncationReading::Unmeasured
    );

    let trusted_obs = observed_step(vec![], clean_observation(&["/content", "/meta"], vec![]));
    assert_eq!(
        read_observed(&trusted_obs, "/content", &[]),
        TruncationReading::Unmeasured,
        "empty trusted set yields no MeasuredClean"
    );
    assert_eq!(
        read_observed(&trusted_obs, "/content", TRUSTED),
        TruncationReading::MeasuredClean {
            stage: UPGRADER_STAGE.into(),
            ceiling: INGEST_STRING_CEILING,
        }
    );
    Ok(())
}

// --- Test 5 ---
// Mutation: restore the discard at trace/upgrader.rs EpisodeStart arm.

#[test]
fn test_05_episode_start_loss_retained_jsonl_and_sqlite() -> anyhow::Result<()> {
    let prompt = "p".repeat(4595);
    let nested = "n".repeat(4595);
    let digest = compute_sha256_str(&prompt);
    let nested_digest = compute_sha256_str(&nested);
    let line = json!({
        "type": "episode_start",
        "episode_id": "ep-loss",
        "timestamp": 1,
        "input": {"prompt": prompt},
        "meta": {"a": {"b": nested}}
    })
    .to_string();

    let observed = upgrade_observed(&line);
    let TraceEvent::EpisodeStart(start) = observed.event() else {
        panic!("expected episode_start");
    };
    let emitted_prompt = start.input["prompt"].as_str().expect("prompt string");
    let losses = &observed.observations()[0].losses;
    let prompt_loss = losses
        .iter()
        .find(|l| l.field == "/input/prompt")
        .expect("loss at /input/prompt");
    assert_eq!(prompt_loss.original_len, 4595);
    assert_eq!(prompt_loss.kept_len, emitted_prompt.len());
    assert_eq!(prompt_loss.sha256, digest);
    let nested_loss = losses
        .iter()
        .find(|l| l.field == "/meta/a/b")
        .expect("loss at /meta/a/b");
    assert_eq!(nested_loss.original_len, 4595);
    assert_eq!(nested_loss.sha256, nested_digest);

    let tmp = tempfile::tempdir()?;
    let input = tmp.path().join("in.jsonl");
    let jsonl_out = tmp.path().join("out.jsonl");
    let db_out = tmp.path().join("out.sqlite");
    std::fs::write(&input, format!("{line}\n"))?;
    assay_core::trace::ingest::ingest_file(&input, &jsonl_out)?;
    let round_line = std::fs::read_to_string(&jsonl_out)?;
    let jsonl_round = parse_observed_line(round_line.trim())?;
    assert_eq!(
        jsonl_round.observations()[0].losses,
        observed.observations()[0].losses
    );

    assay_core::trace::ingest::ingest_file(&input, &db_out)?;
    let store = Store::open(&db_out)?;
    assert_eq!(
        store.read_truncation("episode_start", "ep-loss", "/input/prompt", TRUSTED)?,
        TruncationReading::Lossy
    );
    assert_eq!(
        store.read_truncation("episode_start", "ep-loss", "/meta/a/b", TRUSTED)?,
        TruncationReading::Lossy
    );
    Ok(())
}

// --- Test 6 ---
// Mutation: attach an upgrader observation.

#[test]
fn test_06_bypass_producers_emit_no_observation() -> anyhow::Result<()> {
    let mut attrs = HashMap::new();
    attrs.insert("gen_ai.operation.name".into(), json!("chat"));
    let spans = vec![OtelSpan {
        trace_id: "otel-1".into(),
        span_id: "span-1".into(),
        parent_span_id: None,
        name: "chat".into(),
        start_time_unix_nano: "1000000".into(),
        end_time_unix_nano: "2000000".into(),
        attributes: Some(attrs),
    }];
    let otel_events = convert_spans_to_episodes(spans);
    let start = otel_events
        .iter()
        .find(|e| matches!(e, TraceEvent::EpisodeStart(_)))
        .expect("otel episode_start");
    let otel_line = serde_json::to_string(start)?;
    let otel_observed = parse_observed_line(&otel_line)?;
    assert!(
        otel_observed.observations().is_empty(),
        "otel ingest must emit no observation"
    );
    assert_eq!(
        read_observed(&otel_observed, "/input/prompt", TRUSTED),
        TruncationReading::Unmeasured
    );

    let mcp_in = r#"{"jsonrpc":"2.0","id":"req1","method":"tools/call","params":{"name":"T","arguments":{}}}
{"jsonrpc":"2.0","id":"req1","result":1}
"#;
    let mcp_events = parse_mcp_transcript(mcp_in, McpInputFormat::JsonRpc)?;
    let mcp_trace =
        mcp_events_to_v2_trace(mcp_events, "mcp-ep".into(), None, Some("prompt".into()));
    let mcp_line = serde_json::to_string(&mcp_trace[0])?;
    let mcp_observed = parse_observed_line(&mcp_line)?;
    assert!(
        mcp_observed.observations().is_empty(),
        "mcp import must emit no observation"
    );
    assert_eq!(
        read_observed(&mcp_observed, "/input/prompt", TRUSTED),
        TruncationReading::Unmeasured
    );

    let store = Store::memory()?;
    store.init_schema()?;
    store.insert_event(&mcp_trace[0], None, None)?;
    assert_eq!(
        store.read_truncation("episode_start", "mcp-ep", "/input/prompt", TRUSTED)?,
        TruncationReading::Unmeasured
    );
    Ok(())
}

// --- Test 7 ---
// Mutation: skip the binding check (steps, or episodes only).

#[test]
fn test_07_binding_and_tool_call_first_write_wins() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?;
    ensure_episode(&store, "e1")?;

    let clean = ObservedTraceEvent::new(
        TraceEvent::Step(short_step(vec![])),
        vec![clean_observation(&["/content", "/meta"], vec![])],
    );
    store.insert_observed_event(&clean, None, None)?;
    assert_eq!(
        store.read_truncation("step", "s1", "/content", TRUSTED)?,
        TruncationReading::MeasuredClean {
            stage: UPGRADER_STAGE.into(),
            ceiling: INGEST_STRING_CEILING,
        }
    );

    {
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO steps (id, episode_id, idx, kind, name, content, content_sha256, truncations_json, meta_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET content=excluded.content, content_sha256=excluded.content_sha256, truncations_json=excluded.truncations_json, meta_json=excluded.meta_json",
            params![
                "s1",
                "e1",
                0i32,
                "llm_completion",
                "model",
                "rewritten-by-old-sql",
                "new-sha",
                "[]",
                "null",
            ],
        )?;
    }
    assert_ne!(
        store.read_truncation("step", "s1", "/content", TRUSTED)?,
        TruncationReading::MeasuredClean {
            stage: UPGRADER_STAGE.into(),
            ceiling: INGEST_STRING_CEILING,
        },
        "rewritten step must not read MeasuredClean"
    );

    let ep = ObservedTraceEvent::new(
        TraceEvent::EpisodeStart(EpisodeStart {
            episode_id: "ep-bind".into(),
            timestamp: 1,
            input: json!({"prompt": "original"}),
            meta: Value::Null,
        }),
        vec![clean_observation(&["/input", "/meta"], vec![])],
    );
    store.insert_observed_event(&ep, None, None)?;
    assert_eq!(
        store.read_truncation("episode_start", "ep-bind", "/input/prompt", TRUSTED)?,
        TruncationReading::MeasuredClean {
            stage: UPGRADER_STAGE.into(),
            ceiling: INGEST_STRING_CEILING,
        }
    );
    {
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO episodes (id, run_id, test_id, timestamp, prompt, meta_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                run_id=COALESCE(excluded.run_id, episodes.run_id),
                test_id=COALESCE(excluded.test_id, episodes.test_id),
                timestamp=excluded.timestamp,
                prompt=excluded.prompt,
                meta_json=excluded.meta_json",
            params!["ep-bind", Option::<i64>::None, Option::<String>::None, 1i64, "changed-prompt", "null"],
        )?;
    }
    assert_ne!(
        store.read_truncation("episode_start", "ep-bind", "/input/prompt", TRUSTED)?,
        TruncationReading::MeasuredClean {
            stage: UPGRADER_STAGE.into(),
            ceiling: INGEST_STRING_CEILING,
        },
        "rewritten episode must not read MeasuredClean"
    );

    let step_for_tool = StepEntry {
        episode_id: "e1".into(),
        step_id: "s-tool".into(),
        idx: 1,
        timestamp: 2,
        kind: "tool".into(),
        name: Some("t".into()),
        content: None,
        content_sha256: None,
        truncations: vec![],
        meta: Value::Null,
    };
    store.insert_event(&TraceEvent::Step(step_for_tool), None, None)?;
    let first_tc = ObservedTraceEvent::new(
        TraceEvent::ToolCall(ToolCallEntry {
            episode_id: "e1".into(),
            step_id: "s-tool".into(),
            timestamp: 3,
            tool_name: "t".into(),
            call_index: Some(0),
            args: json!({"a": 1}),
            args_sha256: None,
            result: None,
            result_sha256: None,
            error: None,
            truncations: vec![],
        }),
        vec![clean_observation(&["/args", "/result"], vec![])],
    );
    store.insert_observed_event(&first_tc, None, None)?;
    let first_reading = store.read_truncation(
        "tool_call",
        &tool_call_target_key("s-tool", 0),
        "/args",
        TRUSTED,
    )?;
    let second_tc = ObservedTraceEvent::new(
        TraceEvent::ToolCall(ToolCallEntry {
            episode_id: "e1".into(),
            step_id: "s-tool".into(),
            timestamp: 4,
            tool_name: "t".into(),
            call_index: Some(0),
            args: json!({"a": 2}),
            args_sha256: None,
            result: None,
            result_sha256: None,
            error: None,
            truncations: vec![],
        }),
        vec![clean_observation(&["/args", "/result"], vec![])],
    );
    store.insert_observed_event(&second_tc, None, None)?;
    assert_eq!(
        store.read_truncation(
            "tool_call",
            &tool_call_target_key("s-tool", 0),
            "/args",
            TRUSTED
        )?,
        first_reading
    );
    let conn = store.conn.lock().unwrap();
    let args: String = conn.query_row(
        "SELECT args FROM tool_calls WHERE step_id = ?1 AND call_index = 0",
        params!["s-tool"],
        |r| r.get(0),
    )?;
    assert!(
        args.contains('1'),
        "retained tool call args must be the first write: {args}"
    );
    Ok(())
}

// --- Test 8 ---
// Mutation: commit observations in a separate transaction.

#[test]
fn test_08_failing_observation_insert_rolls_back_row() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?;
    {
        let conn = store.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_obs BEFORE INSERT ON trace_observations
             BEGIN SELECT RAISE(ABORT, 'injected observation failure'); END;",
        )?;
    }
    let observed = ObservedTraceEvent::new(
        TraceEvent::EpisodeStart(EpisodeStart {
            episode_id: "ep-atomic".into(),
            timestamp: 1,
            input: json!({"prompt": "hi"}),
            meta: Value::Null,
        }),
        vec![clean_observation(&["/input", "/meta"], vec![])],
    );
    assert!(store.insert_observed_event(&observed, None, None).is_err());
    let conn = store.conn.lock().unwrap();
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM episodes WHERE id = 'ep-atomic'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(
        n, 0,
        "episode row must roll back with the observation insert"
    );
    Ok(())
}

// --- Test 9 ---

#[test]
fn test_09_older_readers_ignore_observations_and_accept_old_ddl() -> anyhow::Result<()> {
    let with_obs = line_step(
        Some(json!([{
            "v": 1,
            "stage": UPGRADER_STAGE,
            "ceiling": INGEST_STRING_CEILING,
            "scope": ["/content", "/meta"],
            "losses": []
        }])),
        json!([]),
    );
    let without = line_step(None, json!([]));

    let via_upgrader_with: Vec<TraceEvent> = StreamUpgrader::new(Cursor::new(&with_obs))
        .map(|r| r.unwrap())
        .collect();
    let via_upgrader_without: Vec<TraceEvent> = StreamUpgrader::new(Cursor::new(&without))
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(via_upgrader_with, via_upgrader_without);

    let v_with: Value = serde_json::from_str(&with_obs)?;
    let v_without: Value = serde_json::from_str(&without)?;
    let step_with: StepEntry = serde_json::from_value(v_with)?;
    let step_without: StepEntry = serde_json::from_value(v_without)?;
    assert_eq!(step_with, step_without, "trace_next from_value path");

    let store = Store::memory()?;
    store.init_schema()?;
    {
        let conn = store.conn.lock().unwrap();
        conn.execute_batch(PRE_CHANGE_DDL)?;
    }
    ensure_episode(&store, "e-old")?;
    store.insert_event(
        &TraceEvent::Step(StepEntry {
            episode_id: "e-old".into(),
            step_id: "s-old".into(),
            idx: 0,
            timestamp: 1,
            kind: "llm_completion".into(),
            name: None,
            content: Some("ok".into()),
            content_sha256: None,
            truncations: vec![],
            meta: Value::Null,
        }),
        None,
        None,
    )?;
    assert_eq!(store.count_rows("steps")?, 1);
    Ok(())
}

// --- Test 10 ---

#[test]
fn test_10_rerun_pair_preserves_truncations() -> anyhow::Result<()> {
    let historical = meta_loss("/content", 4595, INGEST_STRING_CEILING, "hist");
    let kept = "x".repeat(INGEST_STRING_CEILING);
    let legacy_lossy = json!({
        "type": "step",
        "episode_id": "e1",
        "step_id": "s-rerun",
        "idx": 0,
        "timestamp": 1,
        "kind": "llm_completion",
        "name": "model",
        "content": kept,
        "truncations": [historical],
        "meta": null
    })
    .to_string();
    let rerun = upgrade_observed(&legacy_lossy);
    let TraceEvent::Step(step) = rerun.event() else {
        panic!("step");
    };
    assert_eq!(step.truncations, vec![historical.clone()]);
    assert_eq!(
        read_observed(&rerun, "/content", TRUSTED),
        TruncationReading::Lossy
    );

    let empty_legacy = json!({
        "type": "step",
        "episode_id": "e1",
        "step_id": "s-clean",
        "idx": 0,
        "timestamp": 1,
        "kind": "llm_completion",
        "name": "model",
        "content": "short",
        "truncations": [],
        "meta": null
    })
    .to_string();
    let rerun_empty = upgrade_observed(&empty_legacy);
    let TraceEvent::Step(step) = rerun_empty.event() else {
        panic!("step");
    };
    assert!(step.truncations.is_empty());
    assert_eq!(
        read_observed(&rerun_empty, "/content", TRUSTED),
        TruncationReading::MeasuredClean {
            stage: UPGRADER_STAGE.into(),
            ceiling: INGEST_STRING_CEILING,
        }
    );
    Ok(())
}

// --- Test 11 ---

#[test]
fn test_11_unknown_version_and_malformed_observations_are_absent() -> anyhow::Result<()> {
    let unknown = line_step(
        Some(json!([{
            "v": 99,
            "stage": UPGRADER_STAGE,
            "ceiling": INGEST_STRING_CEILING,
            "scope": ["/content", "/meta"],
            "losses": []
        }])),
        json!([]),
    );
    let parsed = parse_observed_line(&unknown)?;
    assert!(parsed.observations().is_empty());
    assert_eq!(
        read_observed(&parsed, "/content", TRUSTED),
        TruncationReading::Unmeasured
    );
    let event: TraceEvent = serde_json::from_str(&unknown)?;
    assert!(matches!(event, TraceEvent::Step(_)));

    let malformed = line_step(Some(json!("not-an-array")), json!([]));
    let parsed = parse_observed_line(&malformed)?;
    assert!(parsed.observations().is_empty());
    assert_eq!(
        read_observed(&parsed, "/content", TRUSTED),
        TruncationReading::Unmeasured
    );
    let event: TraceEvent = serde_json::from_str(&malformed)?;
    assert!(matches!(event, TraceEvent::Step(_)));

    let in_memory: TruncationObservation = serde_json::from_value(json!({
        "v": 2,
        "stage": UPGRADER_STAGE,
        "ceiling": INGEST_STRING_CEILING,
        "scope": ["/content", "/meta"],
        "losses": []
    }))?;
    assert_eq!(
        read_truncation("/content", &[], &[in_memory], TRUSTED, true),
        TruncationReading::Unmeasured
    );
    Ok(())
}

// --- Test 13 ---
// Mutations: observation-level clean (any loss vetoes the whole observation);
// string-prefix scope matching.

#[test]
fn test_13_per_field_clean() {
    let loss = meta_loss("/meta/a", 100, 10, "m");
    let obs = clean_observation(&["/content", "/meta"], vec![loss.clone()]);
    let observed = observed_step(vec![loss], obs);

    assert_eq!(
        read_observed(&observed, "/meta", TRUSTED),
        TruncationReading::Lossy
    );
    assert_eq!(
        read_observed(&observed, "/meta/a", TRUSTED),
        TruncationReading::Lossy
    );
    assert_eq!(
        read_observed(&observed, "/content", TRUSTED),
        TruncationReading::MeasuredClean {
            stage: UPGRADER_STAGE.into(),
            ceiling: INGEST_STRING_CEILING,
        }
    );
    assert_eq!(
        read_observed(&observed, "/meta2", TRUSTED),
        TruncationReading::Unmeasured
    );
}

// --- Test 14 ---
// Mutation: ignore observation losses that have no truncations match.
// Control: the test 13 record reads MeasuredClean at /content.

#[test]
fn test_14_extra_observation_loss_counts_and_fails_parity() {
    let loss = meta_loss("/meta/a", 100, 10, "m");
    let obs = clean_observation(&["/content", "/meta"], vec![loss]);
    let observed = observed_step(vec![], obs);

    assert_eq!(
        read_observed(&observed, "/meta/a", TRUSTED),
        TruncationReading::Lossy,
        "observation loss still counts without a truncations match"
    );
    assert_eq!(
        read_observed(&observed, "/content", TRUSTED),
        TruncationReading::Unmeasured,
        "parity failure makes the observation absent for the clean reading"
    );

    let control_loss = meta_loss("/meta/a", 100, 10, "m");
    let control = observed_step(
        vec![control_loss.clone()],
        clean_observation(&["/content", "/meta"], vec![control_loss]),
    );
    assert_eq!(
        read_observed(&control, "/content", TRUSTED),
        TruncationReading::MeasuredClean {
            stage: UPGRADER_STAGE.into(),
            ceiling: INGEST_STRING_CEILING,
        }
    );
}

#[test]
fn bound_digest_is_one_function() {
    let a = bound_sha256(&[Some("p"), Some("null")]);
    let b = bound_sha256(&[Some("p"), Some("null")]);
    assert_eq!(a, b);
    assert_ne!(a, bound_sha256(&[Some("q"), Some("null")]));
}

// --- Test 15 ---
// Mutation: SQLite `unwrap_or_default` empty losses on unparsable `losses_json`.
// JSONL-only assertions stay green under that mutant; the media-parity
// comparison is what must bite.

const PARITY_POINTERS: &[&str] = &["/content", "/meta", "/meta/a", "/other"];

fn jsonl_field_readings(line: &str) -> anyhow::Result<Vec<TruncationReading>> {
    let parsed = parse_observed_line(line)?;
    Ok(PARITY_POINTERS
        .iter()
        .map(|pointer| read_observed(&parsed, pointer, TRUSTED))
        .collect())
}

fn sqlite_field_readings(
    store: &Store,
    kind: &str,
    key: &str,
) -> anyhow::Result<Vec<TruncationReading>> {
    PARITY_POINTERS
        .iter()
        .map(|pointer| store.read_truncation(kind, key, pointer, TRUSTED))
        .collect()
}

fn sqlite_step_from_observed(
    observed: &ObservedTraceEvent,
) -> anyhow::Result<Vec<TruncationReading>> {
    let store = Store::memory()?;
    store.init_schema()?;
    ensure_episode(&store, "e1")?;
    store.insert_observed_event(observed, None, None)?;
    sqlite_field_readings(&store, "step", "s1")
}

#[test]
fn test_15_jsonl_sqlite_media_parity_including_malformed_losses() -> anyhow::Result<()> {
    let loss = meta_loss("/meta/a", 100, 10, "m");
    let cases = [
        (
            "clean empty losses",
            line_step(
                Some(json!([{
                    "v": 1,
                    "stage": UPGRADER_STAGE,
                    "ceiling": INGEST_STRING_CEILING,
                    "scope": ["/content", "/meta"],
                    "losses": []
                }])),
                json!([]),
            ),
        ),
        (
            "reported loss",
            line_step(
                Some(json!([{
                    "v": 1,
                    "stage": UPGRADER_STAGE,
                    "ceiling": INGEST_STRING_CEILING,
                    "scope": ["/content", "/meta"],
                    "losses": [loss]
                }])),
                json!([loss]),
            ),
        ),
        (
            "malformed losses",
            line_step(
                Some(json!([{
                    "v": 1,
                    "stage": UPGRADER_STAGE,
                    "ceiling": INGEST_STRING_CEILING,
                    "scope": ["/content", "/meta"],
                    "losses": "not-an-array"
                }])),
                json!([]),
            ),
        ),
    ];

    for (name, line) in cases {
        let jsonl = jsonl_field_readings(&line)?;
        let parsed = parse_observed_line(&line)?;
        let sqlite = sqlite_step_from_observed(&parsed)?;
        assert_eq!(
            jsonl, sqlite,
            "{name}: JSONL and SQLite must agree at {PARITY_POINTERS:?}"
        );
    }

    let malformed_losses_json = serde_json::to_string(&json!("not-an-array"))?;
    let jsonl_malformed = jsonl_field_readings(&line_step(
        Some(json!([{
            "v": 1,
            "stage": UPGRADER_STAGE,
            "ceiling": INGEST_STRING_CEILING,
            "scope": ["/content", "/meta"],
            "losses": "not-an-array"
        }])),
        json!([]),
    ))?;
    assert!(
        jsonl_malformed
            .iter()
            .all(|reading| *reading == TruncationReading::Unmeasured),
        "JSONL leg alone: malformed losses are absent, so every field is Unmeasured"
    );

    let store = Store::memory()?;
    store.init_schema()?;
    ensure_episode(&store, "e1")?;
    store.insert_observed_event(
        &observed_step(vec![], clean_observation(&["/content", "/meta"], vec![])),
        None,
        None,
    )?;
    {
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "UPDATE trace_observations SET losses_json = ?1
             WHERE target_kind = 'step' AND target_key = 's1'",
            params![malformed_losses_json],
        )?;
    }
    let sqlite_malformed = sqlite_field_readings(&store, "step", "s1")?;
    assert_eq!(
        jsonl_malformed, sqlite_malformed,
        "malformed losses_json on a bound row must match JSONL, not collapse to empty losses"
    );
    Ok(())
}

#[test]
fn probe_a_sqlite_reader_refuses_pointer_outside_stored_column_map() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?;

    let event = TraceEvent::EpisodeStart(EpisodeStart {
        episode_id: "ep-a".into(),
        timestamp: 1,
        input: json!({
            "prompt": "short prompt",
            "system": "also short system prompt"
        }),
        meta: Value::Null,
    });
    let line = serde_json::to_string(&event)?;
    let observed = StreamUpgrader::new(Cursor::new(&line))
        .observed()
        .next()
        .expect("one event")?;
    store.insert_observed_event(&observed, None, None)?;

    let prompt_reading =
        store.read_truncation("episode_start", "ep-a", "/input/prompt", TRUSTED)?;
    assert_eq!(
        prompt_reading,
        TruncationReading::MeasuredClean {
            stage: UPGRADER_STAGE.into(),
            ceiling: INGEST_STRING_CEILING,
        }
    );

    let system_result = store.read_truncation("episode_start", "ep-a", "/input/system", TRUSTED);
    assert!(
        system_result.is_err(),
        "read_truncation for /input/system must return Err because it is outside stored column map, but got: {:?}",
        system_result
    );
    Ok(())
}

#[test]
fn probe_b_loss_at_ancestor_pointer_covers_descendants() -> anyhow::Result<()> {
    let loss = meta_loss("/meta", 5000, 4096, "sha-meta");
    let obs = clean_observation(&["/content", "/meta"], vec![loss.clone()]);
    let step = observed_step(vec![loss], obs);

    let meta_reading = read_observed(&step, "/meta", TRUSTED);
    assert_eq!(meta_reading, TruncationReading::Lossy);

    let child_reading = read_observed(&step, "/meta/a", TRUSTED);
    assert_eq!(
        child_reading,
        TruncationReading::Lossy,
        "loss at /meta must make descendant /meta/a read Lossy, but got: {:?}",
        child_reading
    );
    Ok(())
}

#[test]
fn probe_c_foreign_empty_loss_observation_not_carried_at_ingest() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?;

    let line = json!({
        "type": "step",
        "episode_id": "ep-c",
        "step_id": "s1",
        "idx": 0,
        "timestamp": 1,
        "kind": "llm_completion",
        "name": "model",
        "content": "hello",
        "content_sha256": "abc",
        "truncations": [],
        "meta": null,
        "observations": [{
            "v": 1,
            "stage": "vendor.exporter",
            "ceiling": 4096,
            "scope": ["/content", "/meta"],
            "losses": []
        }]
    })
    .to_string();

    ensure_episode(&store, "ep-c")?;

    let observed = StreamUpgrader::new(Cursor::new(&line))
        .observed()
        .next()
        .expect("one event")?;
    store.insert_observed_event(&observed, None, None)?;

    let count: i64 = store.conn.lock().unwrap().query_row(
        "SELECT COUNT(*) FROM trace_observations WHERE target_kind = 'step' AND target_key = 's1'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(
        count, 1,
        "only the fresh scan observation should be stored; foreign empty-loss observation must be dropped at ingest, but found {count} rows"
    );

    let vendor_reading = store.read_truncation("step", "s1", "/content", &["vendor.exporter"])?;
    assert_ne!(
        vendor_reading,
        TruncationReading::MeasuredClean {
            stage: "vendor.exporter".into(),
            ceiling: 4096,
        },
        "foreign empty-loss observation must not yield MeasuredClean after ingest"
    );

    Ok(())
}
