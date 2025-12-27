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
