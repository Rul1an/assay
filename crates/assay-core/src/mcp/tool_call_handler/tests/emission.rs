use super::super::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use super::fixtures::{assert_fail_closed_defaults, make_tool_call_request, CountingEmitter};
use crate::mcp::decision::{reason_codes, FulfillmentDecisionPath, ObligationOutcomeStatus};
use crate::mcp::identity::ToolIdentity;
use crate::mcp::policy::{McpPolicy, PolicyState, ToolPolicy, TypedPolicyDecision};
use crate::mcp::tool_definition::{
    binding_from_tools_list_tool, TOOL_DEFINITION_CANONICALIZATION_JCS_MCP_TOOL_DEFINITION_V1,
    TOOL_DEFINITION_DIGEST_ALG_SHA256, TOOL_DEFINITION_SCHEMA_V1,
    TOOL_DEFINITION_SOURCE_MCP_TOOLS_LIST,
};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

#[test]
fn test_handler_emits_decision_on_policy_deny() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let policy = McpPolicy {
        tools: ToolPolicy {
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn test_allow_with_warning_emits_log_obligation_outcome() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let policy = McpPolicy::default();
    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request("unconstrained_tool", serde_json::json!({}));
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Allow { decision_event, .. } => {
            assert_eq!(
                decision_event.data.typed_decision,
                Some(TypedPolicyDecision::AllowWithObligations)
            );
            assert_fail_closed_defaults(&decision_event);
            assert_eq!(decision_event.data.obligation_outcomes.len(), 1);
            let outcome = &decision_event.data.obligation_outcomes[0];
            assert_eq!(outcome.obligation_type, "log");
            assert_eq!(outcome.status, ObligationOutcomeStatus::Applied);
            assert_eq!(
                outcome.reason.as_deref(),
                Some("mapped from legacy_warning")
            );
            assert_eq!(
                outcome.reason_code.as_deref(),
                Some("legacy_warning_mapped")
            );
            assert_eq!(outcome.enforcement_stage.as_deref(), Some("executor"));
            assert_eq!(outcome.normalization_version.as_deref(), Some("v1"));
            assert_eq!(
                decision_event.data.fulfillment_decision_path,
                Some(FulfillmentDecisionPath::PolicyAllow)
            );
            assert_eq!(decision_event.data.obligation_applied_present, Some(true));
            assert_eq!(decision_event.data.obligation_skipped_present, Some(false));
            assert_eq!(decision_event.data.obligation_error_present, Some(false));
        }
        other => panic!("expected allow result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn test_tool_drift_deny_emits_alert_obligation_outcome() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let mut policy = McpPolicy::default();
    let pinned = ToolIdentity::new(
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

    let runtime_identity = ToolIdentity::new(
        "server-a",
        "drift_tool",
        &Some(serde_json::json!({"shape": "runtime"})),
        &Some("Runtime version".to_string()),
    );

    let request = make_tool_call_request("drift_tool", serde_json::json!({}));
    let mut state = PolicyState::default();
    let result =
        handler.handle_tool_call(&request, &mut state, Some(&runtime_identity), None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            decision_event,
            ..
        } => {
            assert_eq!(reason_code, reason_codes::P_TOOL_DRIFT);
            assert_fail_closed_defaults(&decision_event);
            assert_eq!(
                decision_event.data.typed_decision,
                Some(TypedPolicyDecision::DenyWithAlert)
            );
            assert_eq!(decision_event.data.obligations.len(), 1);
            assert_eq!(decision_event.data.obligations[0].obligation_type, "alert");
            assert_eq!(decision_event.data.obligation_outcomes.len(), 1);
            let outcome = &decision_event.data.obligation_outcomes[0];
            assert_eq!(outcome.obligation_type, "alert");
            assert_eq!(outcome.status, ObligationOutcomeStatus::Applied);
            assert!(outcome.reason.is_none());
            assert_eq!(outcome.reason_code.as_deref(), Some("obligation_applied"));
            assert_eq!(outcome.enforcement_stage.as_deref(), Some("executor"));
            assert_eq!(outcome.normalization_version.as_deref(), Some("v1"));
            assert_eq!(
                decision_event.data.fulfillment_decision_path,
                Some(FulfillmentDecisionPath::PolicyDeny)
            );
            assert_eq!(decision_event.data.obligation_applied_present, Some(true));
            assert_eq!(decision_event.data.obligation_skipped_present, Some(false));
            assert_eq!(decision_event.data.obligation_error_present, Some(false));
        }
        other => panic!("expected deny result, got {:?}", other),
    }

    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn test_alert_obligation_outcome_emitted() {
    test_tool_drift_deny_emits_alert_obligation_outcome();
}

#[test]
fn test_handler_projects_tool_definition_binding_when_supplied() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let policy = McpPolicy::default();
    let handler = ToolCallHandler::new(
        policy,
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );
    let binding = binding_from_tools_list_tool(
        &serde_json::json!({
            "name": "safe_tool",
            "description": " Safe read ",
            "inputSchema": {"type": "object"}
        }),
        Some("server-a"),
    )
    .unwrap()
    .unwrap();

    let request = make_tool_call_request("safe_tool", serde_json::json!({}));
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call_with_tool_definition_binding(
        &request,
        &mut state,
        None,
        Some(&binding),
        None,
        None,
    );

    match result {
        HandleResult::Allow { decision_event, .. } => {
            assert!(decision_event.data.tool_definition_digest.is_some());
            assert_eq!(
                decision_event.data.tool_definition_digest_alg.as_deref(),
                Some(TOOL_DEFINITION_DIGEST_ALG_SHA256)
            );
            assert_eq!(
                decision_event
                    .data
                    .tool_definition_canonicalization
                    .as_deref(),
                Some(TOOL_DEFINITION_CANONICALIZATION_JCS_MCP_TOOL_DEFINITION_V1)
            );
            assert_eq!(
                decision_event.data.tool_definition_schema.as_deref(),
                Some(TOOL_DEFINITION_SCHEMA_V1)
            );
            assert_eq!(
                decision_event.data.tool_definition_source.as_deref(),
                Some(TOOL_DEFINITION_SOURCE_MCP_TOOLS_LIST)
            );
        }
        other => panic!("expected allow result, got {:?}", other),
    }
}
