use assay_core::storage::Store;
use assay_core::trace::otel_ingest::{convert_spans_to_episodes, OtelSpan};
use std::io::BufRead;

#[test]
fn test_otel_ingest_logic() -> anyhow::Result<()> {
    // 1. Load Fixture
    let path = "tests/fixtures/otel_genai_trace.jsonl";
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    let mut spans = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let span: OtelSpan = serde_json::from_str(&line)?;
        spans.push(span);
    }

    // 2. Convert
    // 2. Convert
    let events = convert_spans_to_episodes(spans);
    assert_eq!(
        events.len(),
        5,
        "Expected EpisodeStart + Step(Model) + Step(Tool) + ToolCall + EpisodeEnd"
    );

    // Robust checks
    let starts = events
        .iter()
        .filter(|e| matches!(e, assay_core::trace::schema::TraceEvent::EpisodeStart(_)))
        .count();
    let ends = events
        .iter()
        .filter(|e| matches!(e, assay_core::trace::schema::TraceEvent::EpisodeEnd(_)))
        .count();
    assert_eq!(starts, 1, "Should have exactly 1 EpisodeStart");
    assert_eq!(ends, 1, "Should have exactly 1 EpisodeEnd");

    // Verify last event is EpisodeEnd
    if let assay_core::trace::schema::TraceEvent::EpisodeEnd(end) = events.last().unwrap() {
        assert!(!end.episode_id.is_empty());
        // Note: fixture currently doesn't have gen_ai.completion in root span, so final_output is None.
        // If we update fixture, we can assert is_some().
    } else {
        panic!("Last event should be EpisodeEnd");
    }

    // Let's check logic:
    // loop spans:
    //   span1 (chat) -> Step
    //   span2 (tool) -> Step + ToolCall
    // + EpisodeStart at beginning.
    // + EpisodeEnd at the end.
    // Total 1 + 1 + 2 + 1 = 5. Correct.

    // 3. Store
    let store = Store::memory()?;
    store.init_schema()?;

    // We need parent run for FK
    // We need parent run for FK
    let run_id = store.insert_run("test-suite")?;

    store.insert_batch(&events, Some(run_id), Some("test-agent"))?;

    // 4. Verify via Graph
    let _graph = store.get_episode_graph(1, "test-agent")?; // test_id irrelevant as we just query by run_id/test_id if linked.
                                                            // Wait, get_episode_graph takes (run_id, test_id).
                                                            // Otel ingest does NOT link to run_id/test_id by default (passed None, None).
                                                            // So `get_episode_graph` might fail to find it IF it relies on `episodes.run_id` match.
                                                            // `get_episode_graph` query:
                                                            // `SELECT id FROM episodes WHERE run_id = ?1 AND test_id = ?2`
                                                            // So YES, it will fail if we don't link it.

    // FIX: Otel ingest (CLI) passes None/None.
    // Tests need to manually link or we use a different query method.
    // Actually, for assertions we need them linked.
    // How does User link OTel traces to Tests?
    // CLI `ingest-otel` implementation passed None/None.
    // So assertions wouldn't work on them immediately out of the box unless we update them?
    // OR we ingest with run/test IDs?
    // User plan didn't specify linking strategy.

    // For this test, I will update the events to have run_id / test_id to simulate what happens
    // if we did link them (e.g. via post-process or arguments).
    // Or I check raw tables.

    // Let's check raw tables for MVP correctness of schema.
    let conn = store.conn.lock().unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM steps", [], |r| r.get(0))?;
    assert_eq!(count, 2, "Expected 2 steps");

    let tools: i64 = conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |r| r.get(0))?;
    assert_eq!(tools, 1, "Expected 1 tool call");

    Ok(())
}

#[test]
fn test_otel_gen_ai_prompt_multibyte_over_4096_nontruncation() {
    // #2786 Slice B: Pin that an over-4096-byte multibyte OTel gen_ai.prompt
    // remains byte-identical in the emitted Step content and meta.
    //
    // Note: OTel ingest does not run Assay's truncator (`truncate_value_with_provenance`
    // or `apply_truncation`). The prompt remains byte-identical in both Step.content and
    // Step.meta. The resulting Step.truncations vector is empty because no truncation was run,
    // which reflects producer pass-through rather than a measured-clean guarantee.
    // This test makes no claim of upstream completeness.
    let chunk = "✨ otel gen_ai prompt with multibyte 日本語 and €uro ✨ ";
    let repeat_count = (5000 / chunk.len()) + 1;
    let long_prompt = chunk.repeat(repeat_count);
    assert!(long_prompt.len() > 4096);
    assert!(!long_prompt.is_ascii());

    let mut attrs = std::collections::HashMap::new();
    attrs.insert(
        "gen_ai.operation.name".to_string(),
        serde_json::json!("chat"),
    );
    attrs.insert(
        "gen_ai.request.model".to_string(),
        serde_json::json!("test-model"),
    );
    attrs.insert("gen_ai.prompt".to_string(), serde_json::json!(long_prompt));
    attrs.insert(
        "gen_ai.completion".to_string(),
        serde_json::json!("completion response"),
    );

    let span = OtelSpan {
        trace_id: "trace-otel-nontrunc-test".to_string(),
        span_id: "span-model-1".to_string(),
        parent_span_id: None,
        name: "chat test-model".to_string(),
        start_time_unix_nano: "1700000000000000000".to_string(),
        end_time_unix_nano: "1700000001000000000".to_string(),
        attributes: Some(attrs),
    };

    let events = convert_spans_to_episodes(vec![span]);

    let model_step = events
        .iter()
        .find_map(|e| match e {
            assay_core::trace::schema::TraceEvent::Step(s) if s.kind == "model" => Some(s),
            _ => None,
        })
        .expect("Should emit model Step");

    // Verify content contains byte-identical prompt
    let content_raw = model_step
        .content
        .as_deref()
        .expect("model step should have content");
    let content_json: serde_json::Value =
        serde_json::from_str(content_raw).expect("content should be valid json");
    assert_eq!(
        content_json["prompt"].as_str(),
        Some(long_prompt.as_str()),
        "Step content prompt must remain byte-identical without truncation"
    );

    // Verify meta contains byte-identical gen_ai.prompt
    assert_eq!(
        model_step
            .meta
            .get("gen_ai.prompt")
            .and_then(|v| v.as_str()),
        Some(long_prompt.as_str()),
        "Step meta gen_ai.prompt must remain byte-identical without truncation"
    );

    // Verify truncations is empty (no truncator was run; absent/empty is producer pass-through)
    assert!(
        model_step.truncations.is_empty(),
        "Producer did not invoke Assay's truncator, so truncations is empty (not measured-clean)"
    );
}
