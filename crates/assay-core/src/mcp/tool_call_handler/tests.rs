use super::*;
use crate::mcp::decision::{reason_codes, DecisionEvent, NullDecisionEmitter};
use crate::mcp::lifecycle::{LifecycleEmitter, LifecycleEvent};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct CountingEmitter(AtomicUsize);

impl DecisionEmitter for CountingEmitter {
    fn emit(&self, _event: &DecisionEvent) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn make_tool_call_request(tool: &str, args: Value) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::Number(1.into())),
        method: "tools/call".to_string(),
        params: serde_json::json!({
            "name": tool,
            "arguments": args
        }),
    }
}

#[test]
fn test_handler_emits_decision_on_policy_deny() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let policy = McpPolicy {
        tools: super::super::policy::ToolPolicy {
            allow: None,
            deny: Some(vec!["dangerous_*".to_string()]),
            ..Default::default()
        },
        ..Default::default()
    };

    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request("dangerous_tool", serde_json::json!({}));
    let mut state = PolicyState::default();

    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    assert!(matches!(result, HandleResult::Deny { .. }));
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn test_handler_emits_decision_on_policy_allow() {
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
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn test_commit_tool_without_mandate_denied() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let policy = McpPolicy::default();

    let config = ToolCallHandlerConfig {
        event_source: "assay://test".to_string(),
        require_mandate_for_commit: true,
        commit_tools: vec!["purchase_*".to_string()],
        write_tools: vec![],
    };

    let handler = ToolCallHandler::new(policy, None, emitter.clone(), config);

    let request = make_tool_call_request("purchase_item", serde_json::json!({}));
    let mut state = PolicyState::default();

    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    assert!(
        matches!(result, HandleResult::Deny { reason_code, .. } if reason_code == reason_codes::P_MANDATE_REQUIRED)
    );
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn test_is_commit_tool_matching() {
    let config = ToolCallHandlerConfig {
        commit_tools: vec!["purchase_*".to_string(), "delete_account".to_string()],
        ..Default::default()
    };

    let handler = ToolCallHandler::new(
        McpPolicy::default(),
        None,
        Arc::new(NullDecisionEmitter),
        config,
    );

    assert!(handler.is_commit_tool("purchase_item"));
    assert!(handler.is_commit_tool("purchase_subscription"));
    assert!(handler.is_commit_tool("delete_account"));
    assert!(!handler.is_commit_tool("search_products"));
    assert!(!handler.is_commit_tool("purchase")); // Doesn't match purchase_*
}

#[test]
fn test_operation_class_for_tool() {
    use crate::runtime::OperationClass;
    let config = ToolCallHandlerConfig {
        commit_tools: vec!["purchase_*".to_string()],
        write_tools: vec!["update_*".to_string(), "create_item".to_string()],
        ..Default::default()
    };
    let handler = ToolCallHandler::new(
        McpPolicy::default(),
        None,
        Arc::new(NullDecisionEmitter),
        config,
    );
    assert_eq!(
        handler.operation_class_for_tool("purchase_item"),
        OperationClass::Commit
    );
    assert_eq!(
        handler.operation_class_for_tool("update_profile"),
        OperationClass::Write
    );
    assert_eq!(
        handler.operation_class_for_tool("create_item"),
        OperationClass::Write
    );
    assert_eq!(
        handler.operation_class_for_tool("read_file"),
        OperationClass::Read
    );
}

// === P0-B: Lifecycle event emission tests ===

#[allow(dead_code)] // Prepared for future tests with mandate authorization
struct CountingLifecycleEmitter(AtomicUsize, std::sync::Mutex<Vec<LifecycleEvent>>);

impl LifecycleEmitter for CountingLifecycleEmitter {
    fn emit(&self, event: &LifecycleEvent) {
        self.0.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut events) = self.1.lock() {
            events.push(event.clone());
        }
    }
}

#[test]
fn test_lifecycle_emitter_not_called_when_none() {
    // When no lifecycle emitter is set, handler should still work
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let policy = McpPolicy::default();

    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );
    // No lifecycle emitter set

    let request = make_tool_call_request("safe_tool", serde_json::json!({}));
    let mut state = PolicyState::default();

    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    assert!(matches!(result, HandleResult::Allow { .. }));
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1); // Decision emitted
}
