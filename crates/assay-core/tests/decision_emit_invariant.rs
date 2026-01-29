//! Integration tests for decision emission invariant I1.
//!
//! These tests verify that every tool call attempt results in exactly
//! one decision event being emitted, regardless of outcome.

use assay_core::mcp::decision::{
    reason_codes, Decision, DecisionEmitter, DecisionEmitterGuard, DecisionEvent,
};
use assay_core::mcp::policy::{McpPolicy, PolicyState, ToolPolicy};
use assay_core::mcp::tool_call_handler::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Test emitter that counts and stores events.
struct TestEmitter {
    count: AtomicUsize,
    events: Mutex<Vec<DecisionEvent>>,
}

impl TestEmitter {
    fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            events: Mutex::new(Vec::new()),
        }
    }

    fn event_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    fn last_event(&self) -> Option<DecisionEvent> {
        self.events.lock().unwrap().last().cloned()
    }
}

impl DecisionEmitter for TestEmitter {
    fn emit(&self, event: &DecisionEvent) {
        self.count.fetch_add(1, Ordering::SeqCst);
        self.events.lock().unwrap().push(event.clone());
    }
}

fn make_tool_request(tool: &str) -> assay_core::mcp::jsonrpc::JsonRpcRequest {
    assay_core::mcp::jsonrpc::JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: "tools/call".to_string(),
        params: serde_json::json!({
            "name": tool,
            "arguments": {}
        }),
    }
}

// =============================================================================
// Test: Policy allow emits exactly one decision
// =============================================================================
#[test]
fn test_policy_allow_emits_once() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request("safe_tool");
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    assert!(matches!(result, HandleResult::Allow { .. }));
    assert_eq!(emitter.event_count(), 1, "Expected exactly 1 event");

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.decision, Decision::Allow);
    assert_eq!(event.data.tool, "safe_tool");
}

// =============================================================================
// Test: Policy deny emits exactly one decision
// =============================================================================
#[test]
fn test_policy_deny_emits_once() {
    let emitter = Arc::new(TestEmitter::new());
    let mut policy = McpPolicy::default();
    policy.tools = ToolPolicy {
        allow: None,
        deny: Some(vec!["blocked_*".to_string()]),
    };
    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request("blocked_tool");
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    assert!(matches!(result, HandleResult::Deny { .. }));
    assert_eq!(emitter.event_count(), 1, "Expected exactly 1 event");

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.decision, Decision::Deny);
    assert_eq!(event.data.tool, "blocked_tool");
}

// =============================================================================
// Test: Commit tool without mandate emits deny
// =============================================================================
#[test]
fn test_commit_tool_no_mandate_emits_deny() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let config = ToolCallHandlerConfig {
        event_source: "assay://test".to_string(),
        require_mandate_for_commit: true,
        commit_tools: vec!["purchase_*".to_string()],
    };
    let handler = ToolCallHandler::new(policy, None, emitter.clone(), config);

    let request = make_tool_request("purchase_item");
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    assert!(matches!(result, HandleResult::Deny { .. }));
    assert_eq!(emitter.event_count(), 1, "Expected exactly 1 event");

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.decision, Decision::Deny);
    assert_eq!(event.data.reason_code, reason_codes::P_MANDATE_REQUIRED);
}

// =============================================================================
// Test: Guard dropped without emit still emits
// =============================================================================
#[test]
fn test_guard_drop_emits_on_early_return() {
    let emitter = Arc::new(TestEmitter::new());

    // Simulate early return scenario
    fn simulate_early_return(emitter: Arc<TestEmitter>) {
        let _guard = DecisionEmitterGuard::new(
            emitter,
            "assay://test".to_string(),
            "tc_001".to_string(),
            "test_tool".to_string(),
        );
        // Early return without calling emit_* - guard drops here
    }

    simulate_early_return(emitter.clone());

    assert_eq!(emitter.event_count(), 1, "Guard must emit on drop");
    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.decision, Decision::Error);
}

// =============================================================================
// Test: Guard dropped in panic still emits
// =============================================================================
#[test]
fn test_guard_emits_on_panic() {
    let emitter = Arc::new(TestEmitter::new());
    let emitter_clone = emitter.clone();

    // Catch panic to verify emit happened
    let result = std::panic::catch_unwind(move || {
        let _guard = DecisionEmitterGuard::new(
            emitter_clone,
            "assay://test".to_string(),
            "tc_panic".to_string(),
            "panic_tool".to_string(),
        );
        panic!("Simulated panic");
    });

    assert!(result.is_err(), "Should have panicked");
    assert_eq!(emitter.event_count(), 1, "Guard must emit even on panic");

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.decision, Decision::Error);
    assert_eq!(event.data.tool_call_id, "tc_panic");
}

// =============================================================================
// Test: Multiple tool calls emit multiple events
// =============================================================================
#[test]
fn test_multiple_calls_emit_multiple_events() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let mut state = PolicyState::default();

    // Call 1
    let req1 = make_tool_request("tool_a");
    let _ = handler.handle_tool_call(&req1, &mut state, None, None, None);

    // Call 2
    let req2 = make_tool_request("tool_b");
    let _ = handler.handle_tool_call(&req2, &mut state, None, None, None);

    // Call 3
    let req3 = make_tool_request("tool_c");
    let _ = handler.handle_tool_call(&req3, &mut state, None, None, None);

    assert_eq!(emitter.event_count(), 3, "Should emit 3 events for 3 calls");
}

// =============================================================================
// Test: Event source is from config (I3 invariant)
// =============================================================================
#[test]
fn test_event_source_from_config() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let config = ToolCallHandlerConfig {
        event_source: "assay://myorg/myapp".to_string(),
        require_mandate_for_commit: false,
        commit_tools: vec![],
    };
    let handler = ToolCallHandler::new(policy, None, emitter.clone(), config);

    let request = make_tool_request("any_tool");
    let mut state = PolicyState::default();
    let _ = handler.handle_tool_call(&request, &mut state, None, None, None);

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.source, "assay://myorg/myapp",
        "Source must match config"
    );
}

// =============================================================================
// Test: tool_call_id is propagated (I4 invariant)
// =============================================================================
#[test]
fn test_tool_call_id_propagated() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    // Request with explicit tool_call_id in _meta
    let request = assay_core::mcp::jsonrpc::JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(42)),
        method: "tools/call".to_string(),
        params: serde_json::json!({
            "name": "test_tool",
            "arguments": {
                "_meta": {
                    "tool_call_id": "explicit_tc_123"
                }
            }
        }),
    };

    let mut state = PolicyState::default();
    let _ = handler.handle_tool_call(&request, &mut state, None, None, None);

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.tool_call_id, "explicit_tc_123",
        "tool_call_id must be extracted from _meta"
    );
}

// =============================================================================
// Test: Non-tool-call request still emits decision (I1 edge case)
// =============================================================================
#[test]
fn test_non_tool_call_emits_error() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    // Request that is NOT a tool call (method != tools/call)
    let request = assay_core::mcp::jsonrpc::JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(99)),
        method: "resources/list".to_string(), // NOT tools/call
        params: serde_json::json!({}),
    };

    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    // Must still emit exactly 1 decision (I1 invariant)
    assert!(matches!(result, HandleResult::Error { .. }));
    assert_eq!(
        emitter.event_count(),
        1,
        "I1: must emit even for non-tool-call"
    );

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.decision, Decision::Error);
    assert_eq!(event.data.reason_code, reason_codes::S_INTERNAL_ERROR);
}

// =============================================================================
// Test: Event contains required fields per SPEC
// =============================================================================
#[test]
fn test_event_contains_required_fields() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request("spec_test_tool");
    let mut state = PolicyState::default();
    let _ = handler.handle_tool_call(&request, &mut state, None, None, None);

    let event = emitter.last_event().expect("Should have event");

    // SPEC-Mandate-v1.0.4 required fields
    assert_eq!(event.specversion, "1.0");
    assert_eq!(event.event_type, "assay.tool.decision");
    assert!(!event.source.is_empty());
    assert!(!event.time.is_empty());
    assert!(!event.data.tool.is_empty());
    assert!(!event.data.tool_call_id.is_empty());
    assert!(!event.data.reason_code.is_empty());
}
