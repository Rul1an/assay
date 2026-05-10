use super::constants::{EVENT_SOURCE, EVENT_TYPE, RECEIPT_SCHEMA, SOURCE_SURFACE};
use super::*;
use crate::exit_codes;
use anyhow::Result;
use assay_evidence::bundle::BundleReader;
use std::fs;
use std::fs::File;

#[test]
fn import_writes_verifiable_tool_action_bundle_without_raw_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("livekit-tool-action.json");
    let output = dir.path().join("livekit-tool-action.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","type":"function_tools_executed","event_ref":"turn-42:function_tools_executed:0","created_at":1778320801.5,"function_calls":[{"id":"item_call_lookup_order","call_id":"call_lookup_order_01","name":"lookup_customer_order","arguments":{"order_id":"ord_123","include_items":true},"created_at":1778320801.234,"group_id":null}],"function_call_outputs":[{"id":"item_output_lookup_order","call_id":"call_lookup_order_01","name":"lookup_customer_order","is_error":false,"output":{"status":"shipped","items_count":2},"created_at":1778320801.467}],"has_tool_reply":true,"has_agent_handoff":false}"#,
    )
    .unwrap();

    let code = cmd_livekit_tool_action(LiveKitToolActionArgs {
        input: input.clone(),
        bundle_out: output.clone(),
        source_artifact_ref: Some("livekit-tool-action.json".to_string()),
        run_id: "livekit_test".to_string(),
        import_time: Some("2026-05-09T10:00:02Z".to_string()),
    })
    .unwrap();
    assert_eq!(code, exit_codes::OK);

    let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
    assert_eq!(reader.manifest().event_count, 1);
    let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
    assert_eq!(events[0].type_, EVENT_TYPE);
    assert_eq!(events[0].source, EVENT_SOURCE);
    assert_eq!(events[0].payload["schema"], RECEIPT_SCHEMA);
    assert_eq!(events[0].payload["source_surface"], SOURCE_SURFACE);
    assert_eq!(
        events[0].payload["function"]["name"],
        "lookup_customer_order"
    );
    assert_eq!(
        events[0].payload["function"]["call_id"],
        "call_lookup_order_01"
    );
    assert_eq!(
        events[0].payload["function"]["created_at"],
        "2026-05-09T10:00:01.234Z"
    );
    assert_eq!(events[0].payload["outcome"]["completed"], true);
    assert_eq!(events[0].payload["outcome"]["is_error"], false);
    assert_eq!(events[0].payload["event_context"]["has_tool_reply"], true);

    let serialized = serde_json::to_string(&events[0].payload).unwrap();
    assert!(!serialized.contains("ord_123"));
    assert!(!serialized.contains("shipped"));
    assert!(!serialized.contains("session_id"));
    assert!(serialized.contains("arguments_hash"));
    assert!(serialized.contains("output_hash"));
}

#[test]
fn import_pairs_by_list_order_and_rejects_call_id_mismatch_when_complete() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("livekit-tool-action.json");
    let output = dir.path().join("livekit-tool-action.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-45:function_tools_executed:0","created_at":"2026-05-09T10:05:00Z","function_calls":[{"call_id":"call_a","name":"lookup_a","arguments_ref":"arg:a"},{"call_id":"call_b","name":"lookup_b","arguments_ref":"arg:b"}],"function_call_outputs":[{"call_id":"call_b","name":"lookup_b","is_error":false,"output_ref":"out:b"},{"call_id":"call_a","name":"lookup_a","is_error":false,"output_ref":"out:a"}]}"#,
    )
    .unwrap();

    let err = cmd_livekit_tool_action(LiveKitToolActionArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "livekit_pairing_test".to_string(),
        import_time: Some("2026-05-09T10:05:02Z".to_string()),
    })
    .unwrap_err();
    assert!(err.to_string().contains("call_id mismatch"));
}

#[test]
fn import_accepts_partial_call_ids_using_list_order() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("livekit-tool-action.json");
    let output = dir.path().join("livekit-tool-action.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-45:function_tools_executed:0","created_at":"2026-05-09T10:05:00Z","function_calls":[{"call_id":"call_a","name":"lookup_a","arguments_ref":"arg:a"},{"name":"lookup_b","arguments_ref":"arg:b"}],"function_call_outputs":[{"name":"lookup_a","is_error":false,"output_ref":"out:a"},{"call_id":"call_b","name":"lookup_b","is_error":false,"output_ref":"out:b"}]}"#,
    )
    .unwrap();

    cmd_livekit_tool_action(LiveKitToolActionArgs {
        input,
        bundle_out: output.clone(),
        source_artifact_ref: None,
        run_id: "livekit_partial_id_test".to_string(),
        import_time: Some("2026-05-09T10:05:02Z".to_string()),
    })
    .unwrap();

    let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
    let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].payload["function"]["call_id"], "call_a");
    assert_eq!(events[0].payload["outcome"]["output_ref"], "out:a");
    assert!(events[1].payload["function"].get("call_id").is_none());
    assert_eq!(events[1].payload["outcome"]["output_ref"], "out:b");
}

#[test]
fn import_accepts_multi_row_jsonl_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("livekit-tool-actions.jsonl");
    let output = dir.path().join("livekit-tool-actions.tar.gz");
    fs::write(
        &input,
        concat!(
            r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-46:function_tools_executed:0","created_at":"2026-05-09T10:06:00Z","function_calls":[{"name":"lookup_a","arguments_ref":"arg:a"}],"function_call_outputs":[{"name":"lookup_a","is_error":false,"output_ref":"out:a"}]}"#,
            "\n",
            r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-47:function_tools_executed:0","created_at":"2026-05-09T10:07:00Z","function_calls":[{"name":"lookup_b","arguments_ref":"arg:b"}],"function_call_outputs":[{"name":"lookup_b","is_error":false,"output_ref":"out:b"}]}"#,
            "\n"
        ),
    )
    .unwrap();

    cmd_livekit_tool_action(LiveKitToolActionArgs {
        input,
        bundle_out: output.clone(),
        source_artifact_ref: None,
        run_id: "livekit_jsonl_test".to_string(),
        import_time: Some("2026-05-09T10:07:02Z".to_string()),
    })
    .unwrap();

    let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
    assert_eq!(reader.manifest().event_count, 2);
}

#[test]
fn import_preserves_missing_output_none_without_inferring_error() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("livekit-tool-action.json");
    let output = dir.path().join("livekit-tool-action.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-44:function_tools_executed:0","created_at":1778320921.5,"function_calls":[{"call_id":"call_missing_output_01","name":"lookup_customer_order","arguments":{"order_id":"ord_404"}}],"function_call_outputs":[null]}"#,
    )
    .unwrap();

    cmd_livekit_tool_action(LiveKitToolActionArgs {
        input,
        bundle_out: output.clone(),
        source_artifact_ref: None,
        run_id: "livekit_missing_output_test".to_string(),
        import_time: Some("2026-05-09T10:02:02Z".to_string()),
    })
    .unwrap();

    let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
    let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload["outcome"]["completed"], false);
    assert!(events[0].payload["outcome"].get("is_error").is_none());
    assert!(events[0].payload["outcome"].get("output_hash").is_none());
}

#[test]
fn import_rejects_capture_context_and_session_identity() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("livekit-tool-action.json");
    let output = dir.path().join("livekit-tool-action.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-42:function_tools_executed:0","created_at":1778320801.5,"capture_context":{"session_id":"session-secret"},"function_calls":[{"call_id":"call_a","name":"lookup_a","arguments_ref":"arg:a"}],"function_call_outputs":[{"call_id":"call_a","name":"lookup_a","is_error":false,"output_ref":"out:a"}]}"#,
    )
    .unwrap();

    let err = cmd_livekit_tool_action(LiveKitToolActionArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "livekit_context_test".to_string(),
        import_time: Some("2026-05-09T10:02:02Z".to_string()),
    })
    .unwrap_err();
    assert!(err.to_string().contains("capture context"));
}

#[test]
fn import_rejects_non_integer_raw_float_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("livekit-tool-action.json");
    let output = dir.path().join("livekit-tool-action.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-42:function_tools_executed:0","created_at":1778320801.5,"function_calls":[{"call_id":"call_a","name":"lookup_a","arguments":{"confidence":0.25}}],"function_call_outputs":[{"call_id":"call_a","name":"lookup_a","is_error":false,"output_ref":"out:a"}]}"#,
    )
    .unwrap();

    let err = cmd_livekit_tool_action(LiveKitToolActionArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "livekit_float_test".to_string(),
        import_time: Some("2026-05-09T10:02:02Z".to_string()),
    })
    .unwrap_err();
    assert!(err.to_string().contains("non-integer floats"));
}
