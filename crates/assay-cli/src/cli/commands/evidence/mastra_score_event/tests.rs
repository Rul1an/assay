use super::constants::{EVENT_SOURCE, EVENT_TYPE, SOURCE_SURFACE};
use super::{cmd_mastra_score_event, MastraScoreEventArgs};
use crate::exit_codes;
use anyhow::Result;
use assay_evidence::bundle::BundleReader;
use std::fs::{self, File};

#[test]
fn import_writes_verifiable_score_event_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mastra-score-events.jsonl");
    let output = dir.path().join("mastra-score-events.tar.gz");
    fs::write(
        &input,
        concat!(
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-30T10:31:38.858Z","score_id_ref":"f6605b31-af00-4b17-ae00-ed6262f4f411","scorer_id":"assay-scoreid-proof-scorer","score":0.91,"target_ref":"span:span-proof-001","trace_id_ref":"trace-proof-001","span_id_ref":"span-proof-001","score_trace_id_ref":"score-trace-proof-001","score_source":"live","metadata_ref":"metadata:scoreid-proof"}"#,
            "\n",
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:58:12.297Z","scorer_name":"P14 Live Capture Scorer","score":0.18,"target_ref":"span:c4b7f4a58f2d90e1","trace_id_ref":"9f5bbab9073de1205f4a1de4925ad2b","span_id_ref":"c4b7f4a58f2d90e1","metadata_ref":"metadata:p14-live-capture"}"#,
            "\n"
        ),
    )
    .unwrap();

    let code = cmd_mastra_score_event(MastraScoreEventArgs {
        input: input.clone(),
        bundle_out: output.clone(),
        source_artifact_ref: Some("mastra-score-events.jsonl".to_string()),
        run_id: "mastra_test".to_string(),
        import_time: Some("2026-04-28T12:00:00Z".to_string()),
    })
    .unwrap();
    assert_eq!(code, exit_codes::OK);

    let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
    assert_eq!(reader.manifest().event_count, 2);
    let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
    assert_eq!(events[0].type_, EVENT_TYPE);
    assert_eq!(events[0].source, EVENT_SOURCE);
    assert_eq!(events[0].payload["source_surface"], SOURCE_SURFACE);
    assert_eq!(
        events[0].payload["score_event"]["scorer_id"],
        "assay-scoreid-proof-scorer"
    );
    assert_eq!(events[0].payload["score_event"]["score"], 0.91);
    assert_eq!(
        events[0].payload["score_event"]["timestamp"],
        "2026-04-30T10:31:38.858Z"
    );
    assert_eq!(
        events[0].payload["score_event"]["score_id_ref"],
        "f6605b31-af00-4b17-ae00-ed6262f4f411"
    );
    assert_eq!(
        events[0].payload["score_event"]["score_trace_id_ref"],
        "score-trace-proof-001"
    );
    assert_eq!(
        events[0].payload["score_event"]["metadata_ref"],
        "metadata:scoreid-proof"
    );
    assert_eq!(
        events[1].payload["score_event"]["scorer_name"],
        "P14 Live Capture Scorer"
    );
    assert_eq!(
        events[1].payload["score_event"]["metadata_ref"],
        "metadata:p14-live-capture"
    );

    let serialized = serde_json::to_string(&events).unwrap();
    assert!(!serialized.contains("correlationContext"));
    assert!(!serialized.contains("\"metadata\":"));
    assert!(!serialized.contains("exportedSpan"));
    assert!(!serialized.contains("feedback"));
}

#[test]
fn import_rejects_raw_metadata_and_correlation_context() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mastra-score-events.jsonl");
    let output = dir.path().join("mastra-score-events.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:53:12.297Z","scorer_id":"p14-live-capture-scorer","score":0.92,"target_ref":"span:7c4180655970aca2","metadata":{"traceDepth":2},"correlationContext":{"entityType":"agent"}}"#,
    )
    .unwrap();

    let err = cmd_mastra_score_event(MastraScoreEventArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "mastra_test".to_string(),
        import_time: Some("2026-04-28T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("unsupported top-level key \"metadata\""));
}

#[test]
fn import_rejects_raw_callback_score_object() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mastra-score-events.jsonl");
    let output = dir.path().join("mastra-score-events.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:53:12.297Z","scorer_id":"p14-live-capture-scorer","score":{"score":0.92},"target_ref":"span:7c4180655970aca2"}"#,
    )
    .unwrap();

    let err = cmd_mastra_score_event(MastraScoreEventArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "mastra_test".to_string(),
        import_time: Some("2026-04-28T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err.to_string().contains("score must be a number"));
}

#[test]
fn import_rejects_missing_scorer_identity() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mastra-score-events.jsonl");
    let output = dir.path().join("mastra-score-events.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:53:12.297Z","score":0.92,"target_ref":"span:7c4180655970aca2"}"#,
    )
    .unwrap();

    let err = cmd_mastra_score_event(MastraScoreEventArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "mastra_test".to_string(),
        import_time: Some("2026-04-28T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err.to_string().contains("missing scorer identity"));
}

#[test]
fn import_rejects_legacy_underscore_surface() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mastra-score-events.jsonl");
    let output = dir.path().join("mastra-score-events.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability_score_event","timestamp":"2026-04-15T18:53:12.297Z","scorer_id":"p14-live-capture-scorer","score":0.92,"target_ref":"span:7c4180655970aca2"}"#,
    )
    .unwrap();

    let err = cmd_mastra_score_event(MastraScoreEventArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "mastra_test".to_string(),
        import_time: Some("2026-04-28T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("surface must be \"observability.score_event\""));
}
