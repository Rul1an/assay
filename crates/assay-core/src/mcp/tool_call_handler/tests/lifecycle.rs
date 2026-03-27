use super::super::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use super::fixtures::{make_tool_call_request, CountingEmitter};
use crate::mcp::policy::{McpPolicy, PolicyState};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

#[test]
fn test_lifecycle_emitter_not_called_when_none() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let policy = McpPolicy::default();

    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request("safe_tool", serde_json::json!({}));
    let mut state = PolicyState::default();

    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    assert!(matches!(result, HandleResult::Allow { .. }));
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}
