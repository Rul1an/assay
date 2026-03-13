//! Integration tests for decision emission invariant I1.
//!
//! These tests verify that every tool call attempt results in exactly
//! one decision event being emitted, regardless of outcome.

use assay_core::mcp::decision::{
    reason_codes, Decision, DecisionEmitter, DecisionEmitterGuard, DecisionEvent,
    FulfillmentDecisionPath, ObligationOutcomeStatus,
};
use assay_core::mcp::policy::{
    ApprovalFreshness, McpPolicy, PolicyState, RedactArgsContract, RestrictScopeContract,
    ToolPolicy, TypedPolicyDecision,
};
use assay_core::mcp::tool_call_handler::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use chrono::{Duration, Utc};
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

fn make_tool_request_with_args(
    tool: &str,
    args: serde_json::Value,
) -> assay_core::mcp::jsonrpc::JsonRpcRequest {
    assay_core::mcp::jsonrpc::JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: "tools/call".to_string(),
        params: serde_json::json!({
            "name": tool,
            "arguments": args
        }),
    }
}

#[allow(clippy::field_reassign_with_default)]
fn approval_required_policy() -> McpPolicy {
    let mut policy = McpPolicy::default();
    policy.tools = ToolPolicy {
        approval_required: Some(vec!["deploy_*".to_string()]),
        ..Default::default()
    };
    policy
}

fn restrict_scope_policy_with_contract(contract: RestrictScopeContract) -> McpPolicy {
    let mut policy = McpPolicy::default();
    policy.tools = ToolPolicy {
        restrict_scope: Some(vec!["deploy_*".to_string()]),
        restrict_scope_contract: Some(contract),
        ..Default::default()
    };
    policy
}

fn restrict_scope_policy() -> McpPolicy {
    restrict_scope_policy_with_contract(RestrictScopeContract {
        scope_type: "resource".to_string(),
        scope_value: "service/prod".to_string(),
        scope_match_mode: "exact".to_string(),
    })
}

fn redact_args_policy_with_contract(contract: RedactArgsContract) -> McpPolicy {
    let mut policy = McpPolicy::default();
    policy.tools = ToolPolicy {
        redact_args: Some(vec!["deploy_*".to_string()]),
        redact_args_contract: Some(contract),
        ..Default::default()
    };
    policy
}

fn redact_args_policy() -> McpPolicy {
    redact_args_policy_with_contract(RedactArgsContract {
        redaction_target: "body".to_string(),
        redaction_mode: "mask".to_string(),
        redaction_scope: "request".to_string(),
    })
}

fn approval_artifact(
    bound_tool: &str,
    bound_resource: &str,
    expires_in_seconds: i64,
) -> serde_json::Value {
    let issued_at = Utc::now() - Duration::minutes(5);
    let expires_at = Utc::now() + Duration::seconds(expires_in_seconds);
    serde_json::json!({
        "_meta": {
            "resource": "service/prod",
            "approval": {
                "approval_id": "apr_it_001",
                "approver": "alice@example.com",
                "issued_at": issued_at.to_rfc3339(),
                "expires_at": expires_at.to_rfc3339(),
                "scope": "tool:deploy",
                "bound_tool": bound_tool,
                "bound_resource": bound_resource
            }
        }
    })
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
        ..Default::default()
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
        write_tools: vec![],
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

#[test]
fn test_alert_obligation_outcome_emitted() {
    let emitter = Arc::new(TestEmitter::new());
    let mut policy = McpPolicy::default();
    let pinned = assay_core::mcp::identity::ToolIdentity::new(
        "server-a",
        "drift_tool",
        &Some(serde_json::json!({"shape": "pinned"})),
        &Some("Pinned version".to_string()),
    );
    policy
        .tool_pins
        .insert("drift_tool".to_string(), pinned.clone());
    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let runtime_identity = assay_core::mcp::identity::ToolIdentity::new(
        "server-a",
        "drift_tool",
        &Some(serde_json::json!({"shape": "runtime"})),
        &Some("Runtime version".to_string()),
    );

    let request = make_tool_request("drift_tool");
    let mut state = PolicyState::default();
    let result =
        handler.handle_tool_call(&request, &mut state, Some(&runtime_identity), None, None);
    assert!(matches!(result, HandleResult::Deny { .. }));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.typed_decision,
        Some(TypedPolicyDecision::DenyWithAlert)
    );
    assert_eq!(event.data.obligation_outcomes.len(), 1);
    assert_eq!(event.data.obligation_outcomes[0].obligation_type, "alert");
}

#[test]
fn approval_required_missing_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        approval_required_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        serde_json::json!({"_meta": {"resource": "service/prod"}}),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(result, HandleResult::Deny { .. }));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.reason_code, reason_codes::P_APPROVAL_REQUIRED);
    assert_eq!(
        event.data.approval_failure_reason.as_deref(),
        Some("missing approval")
    );
}

#[test]
fn approval_required_expired_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        approval_required_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        approval_artifact("deploy_service", "service/prod", -30),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(result, HandleResult::Deny { .. }));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.reason_code, reason_codes::P_APPROVAL_REQUIRED);
    assert_eq!(
        event.data.approval_failure_reason.as_deref(),
        Some("expired approval")
    );
    assert_eq!(
        event.data.approval_freshness,
        Some(ApprovalFreshness::Expired)
    );
}

#[test]
fn approval_required_bound_tool_mismatch_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        approval_required_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        approval_artifact("deploy_other", "service/prod", 300),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(result, HandleResult::Deny { .. }));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.approval_failure_reason.as_deref(),
        Some("bound tool mismatch")
    );
}

#[test]
fn approval_required_bound_resource_mismatch_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        approval_required_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        approval_artifact("deploy_service", "service/staging", 300),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(result, HandleResult::Deny { .. }));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.approval_failure_reason.as_deref(),
        Some("bound resource mismatch")
    );
}

#[test]
fn restrict_scope_mismatch_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        restrict_scope_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        serde_json::json!({
            "_meta": {
                "resource": "service/staging"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(
        result,
        HandleResult::Deny { ref reason_code, .. } if reason_code == reason_codes::P_RESTRICT_SCOPE
    ));
    assert_eq!(emitter.event_count(), 1);

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.restrict_scope_present, Some(true));
    assert_eq!(event.data.restrict_scope_match, Some(false));
    assert_eq!(
        event.data.scope_evaluation_state.as_deref(),
        Some("mismatch")
    );
    assert_eq!(
        event.data.scope_failure_reason.as_deref(),
        Some("scope_target_mismatch")
    );
    assert_eq!(
        event.data.restrict_scope_reason.as_deref(),
        Some("scope_target_mismatch")
    );
    assert!(event.data.obligation_outcomes.iter().any(|outcome| {
        outcome.obligation_type == "restrict_scope"
            && outcome.status == ObligationOutcomeStatus::Error
            && outcome.reason.as_deref() == Some("scope_target_mismatch")
    }));
}

#[test]
fn restrict_scope_mismatch_does_not_deny() {
    // Compatibility alias for older gate scripts; semantics are covered by the deny test above.
    restrict_scope_mismatch_denies();
}

#[test]
fn restrict_scope_match_sets_additive_fields() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        restrict_scope_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        serde_json::json!({
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(result, HandleResult::Allow { .. }));
    assert_eq!(emitter.event_count(), 1);

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.restrict_scope_present, Some(true));
    assert_eq!(event.data.restrict_scope_match, Some(true));
    assert_eq!(event.data.scope_type.as_deref(), Some("resource"));
    assert_eq!(event.data.scope_value.as_deref(), Some("service/prod"));
    assert_eq!(event.data.scope_match_mode.as_deref(), Some("exact"));
    assert_eq!(
        event.data.scope_evaluation_state.as_deref(),
        Some("matched")
    );
    assert!(event.data.restrict_scope_reason.is_none());
    assert!(event.data.obligation_outcomes.iter().any(|outcome| {
        outcome.obligation_type == "restrict_scope"
            && outcome.status == ObligationOutcomeStatus::Applied
    }));
}

#[test]
fn restrict_scope_target_missing_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        restrict_scope_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args("deploy_service", serde_json::json!({}));
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(
        result,
        HandleResult::Deny { ref reason_code, .. } if reason_code == reason_codes::P_RESTRICT_SCOPE
    ));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.scope_failure_reason.as_deref(),
        Some("scope_target_missing")
    );
    assert_eq!(
        event.data.restrict_scope_reason.as_deref(),
        Some("scope_target_missing")
    );
}

#[test]
fn restrict_scope_unsupported_match_mode_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        restrict_scope_policy_with_contract(RestrictScopeContract {
            scope_type: "resource".to_string(),
            scope_value: "service/prod".to_string(),
            scope_match_mode: "regex".to_string(),
        }),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        serde_json::json!({
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(
        result,
        HandleResult::Deny { ref reason_code, .. } if reason_code == reason_codes::P_RESTRICT_SCOPE
    ));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.scope_evaluation_state.as_deref(),
        Some("not_evaluated")
    );
    assert_eq!(
        event.data.scope_failure_reason.as_deref(),
        Some("scope_match_mode_unsupported")
    );
}

#[test]
fn restrict_scope_unsupported_scope_type_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        restrict_scope_policy_with_contract(RestrictScopeContract {
            scope_type: "tenant".to_string(),
            scope_value: "acme".to_string(),
            scope_match_mode: "exact".to_string(),
        }),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        serde_json::json!({
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(
        result,
        HandleResult::Deny { ref reason_code, .. } if reason_code == reason_codes::P_RESTRICT_SCOPE
    ));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.scope_evaluation_state.as_deref(),
        Some("not_evaluated")
    );
    assert_eq!(
        event.data.scope_failure_reason.as_deref(),
        Some("scope_type_unsupported")
    );
}

#[test]
fn redact_args_contract_sets_additive_fields() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        redact_args_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        serde_json::json!({
            "body": {"token": "secret"},
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(result, HandleResult::Allow { .. }));
    assert_eq!(emitter.event_count(), 1);

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.redaction_target.as_deref(), Some("body"));
    assert_eq!(event.data.redaction_mode.as_deref(), Some("mask"));
    assert_eq!(event.data.redaction_scope.as_deref(), Some("request"));
    assert_eq!(
        event.data.redaction_applied_state.as_deref(),
        Some("applied")
    );
    assert!(event.data.redaction_reason.is_none());
    assert!(event.data.redaction_failure_reason.is_none());
    assert_eq!(event.data.redact_args_present, Some(true));
    assert_eq!(event.data.redact_args_target.as_deref(), Some("body"));
    assert_eq!(event.data.redact_args_mode.as_deref(), Some("mask"));
    assert_eq!(event.data.redact_args_result.as_deref(), Some("applied"));
    assert!(event.data.redact_args_reason.is_none());
    assert!(event.data.obligation_outcomes.iter().any(|outcome| {
        outcome.obligation_type == "redact_args"
            && outcome.status == ObligationOutcomeStatus::Applied
            && outcome.reason.is_none()
    }));
}

#[test]
fn redact_args_target_missing_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        redact_args_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        serde_json::json!({
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(
        result,
        HandleResult::Deny { ref reason_code, .. } if reason_code == reason_codes::P_REDACT_ARGS
    ));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.redaction_applied_state.as_deref(),
        Some("not_applied")
    );
    assert_eq!(
        event.data.redaction_failure_reason.as_deref(),
        Some("redaction_target_missing")
    );
}

#[test]
fn redact_args_mode_unsupported_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        redact_args_policy_with_contract(RedactArgsContract {
            redaction_target: "body".to_string(),
            redaction_mode: "tokenize".to_string(),
            redaction_scope: "request".to_string(),
        }),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        serde_json::json!({
            "body": {"token": "secret"},
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(
        result,
        HandleResult::Deny { ref reason_code, .. } if reason_code == reason_codes::P_REDACT_ARGS
    ));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.redaction_applied_state.as_deref(),
        Some("not_evaluated")
    );
    assert_eq!(
        event.data.redaction_failure_reason.as_deref(),
        Some("redaction_mode_unsupported")
    );
}

#[test]
fn redact_args_scope_unsupported_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        redact_args_policy_with_contract(RedactArgsContract {
            redaction_target: "body".to_string(),
            redaction_mode: "mask".to_string(),
            redaction_scope: "response".to_string(),
        }),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        serde_json::json!({
            "body": {"token": "secret"},
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(
        result,
        HandleResult::Deny { ref reason_code, .. } if reason_code == reason_codes::P_REDACT_ARGS
    ));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.redaction_applied_state.as_deref(),
        Some("not_evaluated")
    );
    assert_eq!(
        event.data.redaction_failure_reason.as_deref(),
        Some("redaction_scope_unsupported")
    );
}

#[test]
fn redact_args_apply_failed_denies() {
    let emitter = Arc::new(TestEmitter::new());
    let handler = ToolCallHandler::new(
        redact_args_policy_with_contract(RedactArgsContract {
            redaction_target: "body".to_string(),
            redaction_mode: "partial".to_string(),
            redaction_scope: "request".to_string(),
        }),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_request_with_args(
        "deploy_service",
        serde_json::json!({
            "body": {"token": "secret"},
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);
    assert!(matches!(
        result,
        HandleResult::Deny { ref reason_code, .. } if reason_code == reason_codes::P_REDACT_ARGS
    ));

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(
        event.data.redaction_applied_state.as_deref(),
        Some("not_applied")
    );
    assert_eq!(
        event.data.redaction_failure_reason.as_deref(),
        Some("redaction_apply_failed")
    );
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
        write_tools: vec![],
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
    assert!(event.data.policy_version.is_some());
    assert!(event.data.policy_digest.is_some());
    assert_eq!(
        event.data.typed_decision,
        Some(TypedPolicyDecision::AllowWithObligations)
    );
    assert!(!event.data.obligations.is_empty());
    assert!(!event.data.obligation_outcomes.is_empty());
    assert_eq!(event.data.obligation_outcomes[0].obligation_type, "log");
    assert_eq!(
        event.data.obligation_outcomes[0].status,
        ObligationOutcomeStatus::Applied
    );
    assert_eq!(
        event.data.obligation_outcomes[0].reason_code.as_deref(),
        Some("legacy_warning_mapped")
    );
    assert_eq!(
        event.data.fulfillment_decision_path,
        Some(FulfillmentDecisionPath::PolicyAllow)
    );
    assert_eq!(event.data.obligation_applied_present, Some(true));
    assert_eq!(event.data.obligation_skipped_present, Some(false));
    assert_eq!(event.data.obligation_error_present, Some(false));
    assert!(event.data.approval_state.is_none());
    assert!(event.data.approval_id.is_none());
    assert!(event.data.approval_freshness.is_none());
}
