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
            assert_eq!(end.outcome.as_deref(), Some("pass"));
        }
        _ => panic!("Expected EpisodeEnd"),
    }
}

#[test]
fn test_mixed_stream_passthrough() {
    let input = r#"{"schema_version":1,"type":"assay.trace","request_id":"v1-req","prompt":"p","response":"r"}
{"type":"episode_start","episode_id":"v2-ep","timestamp":100}
{"type":"episode_end","episode_id":"v2-ep","timestamp":200}"#;

    let reader = Cursor::new(input);
    let upgrader = StreamUpgrader::new(reader);
    let events: Vec<TraceEvent> = upgrader.map(|res| res.unwrap()).collect();

    // 3 events from V1 + 2 events from V2 = 5 total
    assert_eq!(events.len(), 5);

    // Check V1 upgrade parts
    assert!(matches!(events[0], TraceEvent::EpisodeStart(_)));
    assert!(matches!(events[1], TraceEvent::Step(_)));
    assert!(matches!(events[2], TraceEvent::EpisodeEnd(_)));

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
    let before = TraceClient::from_path(&input)?.complete("P", None).await?;
    assert_eq!(before.text, response);
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
    assert_eq!(after.text, before.text);
    assert_eq!(after.model, before.model);
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
        let events = StreamUpgrader::new(Cursor::new(row.to_string()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let TraceEvent::Step(step) = &events[1] else {
            panic!("expected step")
        };
        assert_eq!(
            step.content_sha256,
            Some(assay_core::trace::truncation::compute_sha256_str(&response))
        );
        if size == INGEST_STRING_CEILING {
            assert_eq!(step.meta["gen_ai.completion"], response);
            assert!(step.content.is_none());
            assert!(step.truncations.is_empty());
        } else {
            assert_eq!(step.content.as_ref().unwrap().len(), INGEST_STRING_CEILING);
            assert!(step.meta.is_null());
            assert_eq!(step.truncations[0].field, "/content");
        }
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
    let before = TraceClient::from_path(&input)?.complete("P", None).await?;
    assert_eq!(before.model, "recorded-model");
    ingest_file(&input, &output)?;
    let after = TraceClient::from_path(&output)?.complete("P", None).await?;
    assert_eq!(after.model, before.model);
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
    Ok(())
}
