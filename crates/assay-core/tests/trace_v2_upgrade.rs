use assay_core::trace::schema::TraceEvent;
use assay_core::trace::upgrader::StreamUpgrader;
use std::io::Cursor;

#[test]
fn test_upgrade_v1_to_v2_stream() {
    // 1. Prepare V1 JSONL input
    let v1_json = r#"{"schema_version":1,"type":"assay.trace","request_id":"req-123","prompt":"Hello","response":"World","meta":{"foo":"bar"}}
{"schema_version":1,"type":"assay.trace","request_id":"req-456","prompt":"Hi","response":"There","meta":{}}"#;

    let reader = Cursor::new(v1_json);

    // 2. Run Upgrader
    let upgrader = StreamUpgrader::new(reader);
    let events: Vec<TraceEvent> = upgrader.map(|res| res.unwrap()).collect();

    // 3. Verify Events
    // Expect 3 events per line * 2 lines = 6 events
    assert_eq!(events.len(), 6);

    // Check first episode logic
    match &events[0] {
        TraceEvent::EpisodeStart(start) => {
            assert_eq!(start.episode_id, "req-123");
            assert_eq!(start.input["prompt"], "Hello");
            assert_eq!(start.meta["foo"], "bar");
        }
        _ => panic!("Expected EpisodeStart"),
    }

    match &events[1] {
        TraceEvent::Step(step) => {
            assert_eq!(step.episode_id, "req-123");
            assert_eq!(step.kind, "llm_completion");
            assert_eq!(step.content.as_deref(), Some("World"));
        }
        _ => panic!("Expected Step"),
    }

    match &events[2] {
        TraceEvent::EpisodeEnd(end) => {
            assert_eq!(end.episode_id, "req-123");
            assert_eq!(
                end.outcome, None,
                "a v1 response carries no episode verdict"
            );
        }
        _ => panic!("Expected EpisodeEnd"),
    }
}

#[test]
fn test_mixed_stream_passthrough() {
    let input = r#"{"schema_version":1,"type":"assay.trace","request_id":"v1-req","prompt":"p","response":"r"}
{"type":"episode_start","episode_id":"v2-ep","timestamp":100}
{"type":"episode_end","episode_id":"v2-ep","timestamp":200,"outcome":"fail"}"#;

    let reader = Cursor::new(input);
    let upgrader = StreamUpgrader::new(reader);
    let events: Vec<TraceEvent> = upgrader.map(|res| res.unwrap()).collect();

    // 3 events from V1 + 2 events from V2 = 5 total
    assert_eq!(events.len(), 5);

    // Check V1 upgrade parts
    assert!(matches!(events[0], TraceEvent::EpisodeStart(_)));
    assert!(matches!(events[1], TraceEvent::Step(_)));
    assert!(matches!(events[2], TraceEvent::EpisodeEnd(_)));

    let TraceEvent::EpisodeEnd(end) = &events[4] else {
        panic!("expected native episode end")
    };
    assert_eq!(end.outcome.as_deref(), Some("fail"));

    // Check V2 passthrough
    match &events[3] {
        TraceEvent::EpisodeStart(start) => assert_eq!(start.episode_id, "v2-ep"),
        _ => panic!("Expected V2 EpisodeStart"),
    }
}

#[tokio::test]
async fn converted_json_response_replays_as_literal_text() -> anyhow::Result<()> {
    use assay_core::providers::{llm::LlmClient, trace::TraceClient};
    use assay_core::trace::ingest::ingest_file;

    let dir = tempfile::tempdir()?;
    let input = dir.path().join("old.jsonl");
    let output = dir.path().join("typed.jsonl");
    let response = r#"{"completion":"different","model":"inside the response"}"#;
    let entry = serde_json::json!({
        "type": "assay.trace", "schema_version": 1, "request_id": "case",
        "prompt": "P", "response": response,
    });
    std::fs::write(&input, entry.to_string())?;
    assert!(TraceClient::from_path(&input).is_err());
    let events =
        StreamUpgrader::new(Cursor::new(entry.to_string())).collect::<Result<Vec<_>, _>>()?;
    let TraceEvent::Step(step) = &events[1] else {
        panic!("expected step")
    };
    assert!(step.meta.get("gen_ai.response.model").is_none());
    assert_eq!(
        step.content_sha256,
        Some(assay_core::trace::truncation::compute_sha256_str(response))
    );

    ingest_file(&input, &output)?;
    let after = TraceClient::from_path(&output)?.complete("P", None).await?;
    assert_eq!(after.text, response);
    assert_eq!(after.model, "trace");
    Ok(())
}

#[test]
fn upgrading_literal_envelopes_at_the_byte_ceiling() {
    use assay_core::trace::truncation::INGEST_STRING_CEILING;

    for size in [INGEST_STRING_CEILING, INGEST_STRING_CEILING + 1] {
        let empty = r#"{"completion":""}"#;
        let response =
            serde_json::json!({"completion": "x".repeat(size - empty.len())}).to_string();
        assert_eq!(response.len(), size);
        let row = serde_json::json!({
            "type": "assay.trace", "schema_version": 1, "request_id": "case",
            "prompt": "P", "response": response,
        });
        let events =
            StreamUpgrader::new(Cursor::new(row.to_string())).collect::<Result<Vec<_>, _>>();
        if size > INGEST_STRING_CEILING {
            assert!(events
                .unwrap_err()
                .to_string()
                .contains("would truncate /content"));
            continue;
        }
        let events = events.unwrap();
        let TraceEvent::Step(step) = &events[1] else {
            panic!("expected step")
        };
        assert_eq!(
            step.content_sha256,
            Some(assay_core::trace::truncation::compute_sha256_str(&response))
        );
        assert_eq!(step.meta["gen_ai.completion"], response);
        assert!(step.content.is_none());
        assert!(step.truncations.is_empty());
        let TraceEvent::EpisodeEnd(end) = &events[2] else {
            panic!("expected episode end")
        };
        assert!(end.final_output.is_none());
    }
}

#[tokio::test]
async fn converted_response_preserves_the_recorded_model() -> anyhow::Result<()> {
    use assay_core::providers::{llm::LlmClient, trace::TraceClient};
    use assay_core::trace::ingest::ingest_file;

    let dir = tempfile::tempdir()?;
    let input = dir.path().join("old.jsonl");
    let output = dir.path().join("typed.jsonl");
    let entry = serde_json::json!({
        "type":"assay.trace", "schema_version":1, "request_id":"case",
        "prompt":"P", "response":"answer", "model":"recorded-model",
    });
    std::fs::write(&input, entry.to_string())?;
    assert!(TraceClient::from_path(&input).is_err());
    ingest_file(&input, &output)?;
    let after = TraceClient::from_path(&output)?.complete("P", None).await?;
    assert_eq!(after.model, "recorded-model");
    Ok(())
}

#[test]
fn converted_literal_remains_visible_in_the_episode_graph() -> anyhow::Result<()> {
    use assay_core::storage::Store;

    let response = r#"{"completion":"inside","model":"inside"}"#;
    let row = serde_json::json!({
        "type": "assay.trace", "schema_version": 1, "request_id": "case",
        "prompt": "P", "response": response,
    });
    let mut untagged = row.clone();
    untagged.as_object_mut().unwrap().remove("type");
    for row in [row, untagged] {
        let events =
            StreamUpgrader::new(Cursor::new(row.to_string())).collect::<Result<Vec<_>, _>>()?;
        let store = Store::memory()?;
        store.init_schema()?;
        let run = store.insert_run("suite")?;
        store.insert_batch(&events, Some(run), Some("test"))?;
        let graph = store.get_episode_graph(run, "test")?;
        assert_eq!(graph.steps[0].content.as_deref(), Some(response));
        let mut stale = events.clone();
        let TraceEvent::Step(step) = &mut stale[1] else {
            panic!("expected step")
        };
        step.meta["gen_ai.completion"] = serde_json::json!("changed");
        store.insert_batch(&stale, Some(run), Some("test"))?;
        assert_eq!(store.get_episode_graph(run, "test")?.steps[0].content, None);

        // A native row can carry arbitrary additional metadata, even if it copies
        // the marker and digest. The converter never emits this metadata shape.
        let mut oversized = events.clone();
        let TraceEvent::Step(step) = &mut oversized[1] else {
            panic!("expected step")
        };
        step.meta["extra"] = serde_json::json!(vec!["x".repeat(4096); 16]);
        store.insert_batch(&oversized, Some(run), Some("test"))?;
        assert_eq!(store.get_episode_graph(run, "test")?.steps[0].content, None);

        // Native typed steps may have replay metadata without a content field.
        let mut native = events.clone();
        let TraceEvent::Step(step) = &mut native[1] else {
            panic!("expected step")
        };
        step.meta = serde_json::json!({"gen_ai.completion": response});
        store.insert_batch(&native, Some(run), Some("test"))?;
        assert_eq!(store.get_episode_graph(run, "test")?.steps[0].content, None);
    }
    Ok(())
}

#[test]
fn jsonl_ingest_refuses_to_overwrite_its_source() -> anyhow::Result<()> {
    use assay_core::trace::ingest::ingest_file;
    use std::fs;

    let dir = tempfile::tempdir()?;
    let input = dir.path().join("source.jsonl");
    let alias = dir.path().join("alias.jsonl");
    let original = br#"{"type":"assay.trace","schema_version":1,"request_id":"case","prompt":"P","response":"R"}"#;
    fs::write(&input, original)?;
    fs::hard_link(&input, &alias)?;

    for output in [&alias, &input] {
        let result = ingest_file(&input, output);
        assert_eq!(fs::read(&input)?, original, "source changed via {output:?}");
        assert!(result.is_err(), "accepted an alias of the source");
    }
    Ok(())
}

#[test]
fn sqlite_ingest_preserves_an_empty_trace_aliased_by_the_database() -> anyhow::Result<()> {
    use assay_core::trace::ingest::ingest_file;

    let dir = tempfile::tempdir()?;
    let input = dir.path().join("empty.jsonl");
    let database = dir.path().join("store.db");
    std::fs::write(&input, [])?;
    std::fs::hard_link(&input, &database)?;
    let result = ingest_file(&input, &database);
    assert_eq!(std::fs::metadata(&input)?.len(), 0);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn sqlite_ingest_keeps_existing_data_when_the_empty_trace_is_separate() -> anyhow::Result<()> {
    use assay_core::trace::ingest::ingest_file;
    use rusqlite::Connection;

    let dir = tempfile::tempdir()?;
    let input = dir.path().join("empty.jsonl");
    std::fs::write(&input, [])?;
    for (name, setup) in [
        ("utf16le", "PRAGMA encoding = 'UTF-16le'"),
        ("utf16be", "PRAGMA encoding = 'UTF-16be'"),
        ("shadow", "CREATE TABLE pragma_database_list(report TEXT)"),
    ] {
        let database = dir.path().join(format!("{name}.db"));
        let connection = Connection::open(&database)?;
        connection.execute_batch(setup)?;
        connection.execute_batch("CREATE TABLE kept AS SELECT 42 AS value")?;
        drop(connection);
        assert_eq!(ingest_file(&input, &database)?.event_count, 0);
        let value: i64 =
            Connection::open(database)?
                .query_row("SELECT value FROM kept", [], |row| row.get(0))?;
        assert_eq!(value, 42);
    }
    Ok(())
}

#[test]
fn an_empty_trace_can_be_ingested_into_memory() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("empty.jsonl");
    std::fs::write(&input, [])?;
    let stats = assay_core::trace::ingest::ingest_file(&input, std::path::Path::new(":memory:"))?;
    assert_eq!(stats.event_count, 0);
    Ok(())
}

#[tokio::test]
async fn converted_trace_preserves_missing_and_explicit_metadata() -> anyhow::Result<()> {
    use assay_core::providers::{llm::LlmClient, trace::TraceClient};
    use assay_core::trace::ingest::ingest_file;
    use serde_json::{json, Value};

    let dir = tempfile::tempdir()?;
    let source = dir.path().join("legacy.jsonl");
    let dest = dir.path().join("events.jsonl");
    for meta in [None, Some(Value::Null), Some(json!({"cost": 0}))] {
        let mut row = json!({"type":"assay.trace","schema_version":1,"request_id":"id","prompt":"P","response":"done"});
        if let Some(meta) = &meta {
            row["meta"] = meta.clone();
        }
        std::fs::write(&source, row.to_string())?;
        ingest_file(&source, &dest)?;
        let after = TraceClient::from_path(&dest)?.complete("P", None).await?;
        assert_eq!(after.meta, meta.unwrap_or_else(|| json!({})), "{row}");
    }
    Ok(())
}

#[test]
fn ingest_keeps_unknown_and_recorded_outcomes_distinct_in_sqlite() -> anyhow::Result<()> {
    use assay_core::trace::ingest::ingest_file;
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("mixed.jsonl");
    let db = dir.path().join("events.db");
    std::fs::write(
        &source,
        concat!(
            r#"{"type":"assay.trace","schema_version":1,"request_id":"old","prompt":"P","response":"done"}"#,
            "\n",
            r#"{"type":"episode_start","episode_id":"native","timestamp":0}"#,
            "\n",
            r#"{"type":"episode_end","episode_id":"native","timestamp":1,"outcome":"fail"}"#,
            "\n",
        ),
    )?;
    ingest_file(&source, &db)?;
    let conn = rusqlite::Connection::open(&db)?;
    for (id, expected) in [("old", None), ("native", Some("fail"))] {
        let outcome: Option<String> =
            conn.query_row("SELECT outcome FROM episodes WHERE id = ?1", [id], |row| {
                row.get(0)
            })?;
        assert_eq!(outcome.as_deref(), expected);
    }
    Ok(())
}

#[test]
fn explicit_trace_type_selects_the_converter() {
    use serde_json::json;
    let v1 = json!({"type":"assay.trace","request_id":"old","prompt":"P","response":"done"});
    let convert = |row: &serde_json::Value| {
        StreamUpgrader::new(Cursor::new(row.to_string())).collect::<Result<Vec<_>, _>>()
    };
    let events = convert(&v1).expect("ADR-002 allows an omitted version");
    assert!(
        matches!(&events[..], [TraceEvent::EpisodeStart(s), _, TraceEvent::EpisodeEnd(_)] if s.episode_id=="old")
    );
    for version in [json!(0), json!(2), json!(null), json!("1")] {
        let mut row = v1.clone();
        row["schema_version"] = version;
        assert!(convert(&row).is_err(), "{row}");
    }
    let mut row = v1;
    row["schema_version"] = json!(1);
    row["type"] = json!("foreign");
    assert!(convert(&row).is_err());
    row["type"] = json!("episode_start");
    assert!(
        convert(&row).is_err(),
        "invalid typed event fell back to v1"
    );
    row["episode_id"] = json!("native");
    row["timestamp"] = json!(42);
    let events = convert(&row).unwrap();
    assert!(
        matches!(&events[..], [TraceEvent::EpisodeStart(s)] if s.episode_id=="native" && s.timestamp==42)
    );
}

#[test]
fn positional_v1_retains_metadata() {
    use serde_json::json;
    for (extra, expected) in [
        (None, json!({})),
        (Some(json!(null)), json!(null)),
        (Some(json!({"k":1})), json!({"k":1})),
    ] {
        let mut fields = vec![
            json!(1),
            json!("assay.trace"),
            json!("id"),
            json!("P"),
            json!("answer"),
        ];
        fields.extend(extra);
        let input = serde_json::to_vec(&fields).unwrap();
        let events = StreamUpgrader::new(Cursor::new(input))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            matches!(&events[..], [TraceEvent::EpisodeStart(start), _, _] if start.meta == expected)
        );
    }
}

#[test]
fn native_positional_end_preserves_its_verdict() {
    let bytes = br#"["episode_end","native",42,"fail",null]"#;
    let events = StreamUpgrader::new(Cursor::new(bytes))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        matches!(&events[..], [TraceEvent::EpisodeEnd(end)] if end.episode_id == "native" && end.timestamp == 42 && end.outcome.as_deref() == Some("fail"))
    );
}

#[test]
fn conversion_refuses_duplicate_request_ids_before_replay() -> anyhow::Result<()> {
    use assay_core::{providers::trace::TraceClient, trace::ingest::ingest_file};
    use serde_json::json;
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("source.jsonl");
    for second in ["different", "same"] {
        let rows = [("same","first"), (second,"second")].map(|(id,prompt)|
            json!({"type":"assay.trace","schema_version":1,"request_id":id,"prompt":prompt,"response":"ok"}).to_string());
        std::fs::write(&input, rows.join("\n"))?;
        assert!(TraceClient::from_path(&input).is_err());
        for name in ["events.jsonl", "events.sqlite"] {
            let result = ingest_file(&input, &dir.path().join(name));
            if second == "same" {
                let error = result
                    .err()
                    .expect("conversion accepted duplicate request_id");
                assert!(format!("{error:#}").contains("request_id"), "{error:#}");
            } else {
                assert_eq!(result?.event_count, 6);
            }
        }
    }
    Ok(())
}

#[test]
fn refusing_duplicate_ids_preserves_previous_ingest() -> anyhow::Result<()> {
    use assay_core::trace::ingest::ingest_file;
    use serde_json::json;
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("source.jsonl");
    let row = |prompt: &str| {
        json!({"type":"assay.trace","schema_version":1,"request_id":"same","prompt":prompt,"response":"ok"}).to_string()
    };
    let mut preserved = vec![];
    for name in ["events.jsonl", "events.sqlite"] {
        let output = dir.path().join(name);
        std::fs::write(&input, row("original"))?;
        ingest_file(&input, &output)?;
        let before = std::fs::read(&output)?;
        std::fs::write(&input, [row("different"), row("second")].join("\n"))?;
        assert!(ingest_file(&input, &output).is_err());
        let unchanged = if name.ends_with(".sqlite") {
            let db = rusqlite::Connection::open(&output)?;
            db.query_row(
                "SELECT count(*) FROM steps WHERE episode_id='same'",
                [],
                |r| r.get::<_, i64>(0),
            )? == 1
        } else {
            before == std::fs::read(&output)?
        };
        preserved.push((name, unchanged));
    }
    assert_eq!(preserved, [("events.jsonl", true), ("events.sqlite", true)]);
    Ok(())
}
