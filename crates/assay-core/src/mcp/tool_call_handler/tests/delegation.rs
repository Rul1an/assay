use super::super::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use super::fixtures::{make_tool_call_request, CountingEmitter};
use crate::mcp::policy::{McpPolicy, PolicyState};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

#[test]
fn delegated_context_emits_typed_fields_for_supported_flow() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        McpPolicy::default(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request(
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

    match result {
        HandleResult::Allow { decision_event, .. } => {
            assert_eq!(
                decision_event.data.delegated_from.as_deref(),
                Some("agent:planner")
            );
            assert_eq!(decision_event.data.delegation_depth, Some(1));
        }
        other => panic!("expected allow result, got {:?}", other),
    }

    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn direct_authorization_flow_omits_delegation_fields() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        McpPolicy::default(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request("safe_tool", serde_json::json!({}));
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Allow { decision_event, .. } => {
            assert_eq!(decision_event.data.delegated_from, None);
            assert_eq!(decision_event.data.delegation_depth, None);
        }
        other => panic!("expected allow result, got {:?}", other),
    }

    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn unstructured_delegation_hints_do_not_emit_typed_fields() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        McpPolicy::default(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request(
        "safe_tool",
        serde_json::json!({
            "_meta": {
                "delegation_hint": "planner maybe delegated this",
                "delegation": {
                    "note": "human-readable only"
                }
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Allow { decision_event, .. } => {
            assert_eq!(decision_event.data.delegated_from, None);
            assert_eq!(decision_event.data.delegation_depth, None);
        }
        other => panic!("expected allow result, got {:?}", other),
    }

    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}
