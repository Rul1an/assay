use std::io::BufRead;
use verdict_core::storage::Store;
use verdict_core::trace::otel_ingest::{convert_spans_to_episodes, OtelSpan};

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
    let events = convert_spans_to_episodes(spans);
    assert_eq!(
        events.len(),
        4,
        "Expected EpisodeStart + Step(Model) + Step(Tool) + ToolCall"
    );
    // EpisodeStart (1), Step1 (Chat), Step2 (Tool), ToolCall (Tool) = 4 events?
    // Let's check logic:
    // loop spans:
    //   span1 (chat) -> Step
    //   span2 (tool) -> Step + ToolCall
    // + EpisodeStart at beginning.
    // Total 1 + 1 + 2 = 4. Correct.

    // 3. Store
    let store = Store::memory()?;
    store.init_schema()?;

    // We need parent run for FK
    // We need parent run for FK
    let run_id = store.insert_run("test-suite")?;

    store.insert_batch(&events, Some(run_id), Some("test-agent"))?;

    // 4. Verify via Graph
    let graph = store.get_episode_graph(1, "test-agent")?; // test_id irrelevant as we just query by run_id/test_id if linked.
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
