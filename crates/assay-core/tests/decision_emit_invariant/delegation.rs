use crate::fixtures::{make_tool_request_with_args, TestEmitter};
use assay_core::mcp::decision::Decision;
use assay_core::mcp::policy::{McpPolicy, PolicyState};
use assay_core::mcp::tool_call_handler::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use std::sync::Arc;

#[test]
fn test_delegation_fields_are_additive_on_emitted_decisions() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "safe_tool",
        serde_json::json!({
            "_meta": {
                "delegation": {
                    "delegated_from": "agent:planner",
                    "delegation_depth": 1
                }
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    assert!(matches!(result, HandleResult::Allow { .. }));
    assert_eq!(emitter.event_count(), 1, "Expected exactly 1 event");

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.decision, Decision::Allow);
    assert_eq!(event.data.delegated_from.as_deref(), Some("agent:planner"));
    assert_eq!(event.data.delegation_depth, Some(1));
}
