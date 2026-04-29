use crate::fixtures::{make_tool_request, TestEmitter};
use assay_core::mcp::decision::{
    reason_codes, ConsumerPayloadState, ConsumerReadPath, ContextPayloadState, Decision,
    DecisionOrigin, DecisionOutcomeKind, DenyClassificationSource, FulfillmentDecisionPath,
    ObligationOutcomeStatus, OutcomeCompatState, ReplayClassificationSource,
    DECISION_BASIS_VERSION_V1, DECISION_CONSUMER_CONTRACT_VERSION_V1,
    DECISION_CONTEXT_CONTRACT_VERSION_V1, DENY_PRECEDENCE_VERSION_V1,
    POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY, POLICY_SNAPSHOT_DIGEST_ALG_SHA256,
    POLICY_SNAPSHOT_SCHEMA_V1,
};
use assay_core::mcp::policy::{McpPolicy, PolicyState, ToolPolicy, TypedPolicyDecision};
use assay_core::mcp::tool_call_handler::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use std::sync::Arc;

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

#[test]
fn test_commit_tool_no_mandate_emits_deny() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let config = ToolCallHandlerConfig {
        event_source: "assay://test".to_string(),
        require_mandate_for_commit: true,
        commit_tools: vec!["purchase_*".to_string()],
        write_tools: vec![],
        ..Default::default()
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

    let req1 = make_tool_request("tool_a");
    let _ = handler.handle_tool_call(&req1, &mut state, None, None, None);

    let req2 = make_tool_request("tool_b");
    let _ = handler.handle_tool_call(&req2, &mut state, None, None, None);

    let req3 = make_tool_request("tool_c");
    let _ = handler.handle_tool_call(&req3, &mut state, None, None, None);

    assert_eq!(emitter.event_count(), 3, "Should emit 3 events for 3 calls");
}

#[test]
fn test_event_source_from_config() {
    let emitter = Arc::new(TestEmitter::new());
    let policy = McpPolicy::default();
    let config = ToolCallHandlerConfig {
        event_source: "assay://myorg/myapp".to_string(),
        require_mandate_for_commit: false,
        commit_tools: vec![],
        write_tools: vec![],
        ..Default::default()
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

    let request = assay_core::mcp::jsonrpc::JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(99)),
        method: "resources/list".to_string(),
        params: serde_json::json!({}),
    };

    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    assert!(matches!(result, HandleResult::Error { .. }));
    assert_eq!(
        emitter.event_count(),
        1,
        "I1: must emit even for non-tool-call"
    );

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.decision, Decision::Error);
    assert_eq!(event.data.reason_code, reason_codes::S_INTERNAL_ERROR);
    assert_eq!(
        event.data.fulfillment_decision_path,
        Some(FulfillmentDecisionPath::DecisionError)
    );
    assert_eq!(
        event.data.decision_outcome_kind,
        Some(DecisionOutcomeKind::EnforcementDeny)
    );
    assert_eq!(
        event.data.decision_origin,
        Some(DecisionOrigin::RuntimeEnforcement)
    );
    assert_eq!(
        event.data.outcome_compat_state,
        Some(OutcomeCompatState::LegacyFieldsPreserved)
    );
}

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

    assert_eq!(event.specversion, "1.0");
    assert_eq!(event.event_type, "assay.tool.decision");
    assert!(!event.source.is_empty());
    assert!(!event.time.is_empty());
    assert!(!event.data.tool.is_empty());
    assert!(!event.data.tool_call_id.is_empty());
    assert!(!event.data.reason_code.is_empty());
    assert!(event.data.policy_version.is_some());
    let policy_digest = event
        .data
        .policy_digest
        .as_deref()
        .expect("policy digest should be visible");
    assert_eq!(
        event.data.policy_snapshot_digest.as_deref(),
        Some(policy_digest)
    );
    assert_eq!(
        event.data.policy_snapshot_digest_alg.as_deref(),
        Some(POLICY_SNAPSHOT_DIGEST_ALG_SHA256)
    );
    assert_eq!(
        event.data.policy_snapshot_canonicalization.as_deref(),
        Some(POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY)
    );
    assert_eq!(
        event.data.policy_snapshot_schema.as_deref(),
        Some(POLICY_SNAPSHOT_SCHEMA_V1)
    );
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
    assert_eq!(
        event.data.decision_outcome_kind,
        Some(DecisionOutcomeKind::ObligationApplied)
    );
    assert_eq!(
        event.data.decision_origin,
        Some(DecisionOrigin::ObligationExecutor)
    );
    assert_eq!(
        event.data.outcome_compat_state,
        Some(OutcomeCompatState::LegacyFieldsPreserved)
    );
    assert_eq!(
        event.data.decision_basis_version.as_deref(),
        Some(DECISION_BASIS_VERSION_V1)
    );
    assert_eq!(event.data.compat_fallback_applied, Some(false));
    assert_eq!(
        event.data.classification_source,
        Some(ReplayClassificationSource::ConvergedOutcome)
    );
    assert_eq!(
        event.data.replay_diff_reason.as_deref(),
        Some("converged_obligation_applied")
    );
    assert_eq!(event.data.legacy_shape_detected, Some(false));
    assert_eq!(
        event.data.decision_consumer_contract_version.as_deref(),
        Some(DECISION_CONSUMER_CONTRACT_VERSION_V1)
    );
    assert_eq!(
        event.data.consumer_read_path,
        Some(ConsumerReadPath::ConvergedDecision)
    );
    assert_eq!(event.data.consumer_fallback_applied, Some(false));
    assert_eq!(
        event.data.consumer_payload_state,
        Some(ConsumerPayloadState::Converged)
    );
    assert_eq!(
        event.data.required_consumer_fields,
        vec![
            "decision".to_string(),
            "reason_code".to_string(),
            "decision_outcome_kind".to_string(),
            "decision_origin".to_string(),
            "fulfillment_decision_path".to_string(),
            "decision_basis_version".to_string(),
        ]
    );
    assert_eq!(event.data.policy_deny, Some(false));
    assert_eq!(event.data.fail_closed_deny, Some(false));
    assert_eq!(event.data.enforcement_deny, Some(false));
    assert_eq!(
        event.data.deny_precedence_version.as_deref(),
        Some(DENY_PRECEDENCE_VERSION_V1)
    );
    assert_eq!(
        event.data.deny_classification_source,
        Some(DenyClassificationSource::OutcomeKind)
    );
    assert_eq!(event.data.deny_legacy_fallback_applied, Some(false));
    assert_eq!(
        event.data.deny_convergence_reason.as_deref(),
        Some("outcome_not_deny")
    );
    assert_eq!(event.data.obligation_applied_present, Some(true));
    assert_eq!(event.data.obligation_skipped_present, Some(false));
    assert_eq!(event.data.obligation_error_present, Some(false));
    assert_eq!(
        event.data.decision_context_contract_version.as_deref(),
        Some(DECISION_CONTEXT_CONTRACT_VERSION_V1)
    );
    assert_eq!(
        event.data.context_payload_state,
        Some(ContextPayloadState::AbsentEnvelope)
    );
    assert_eq!(
        event.data.required_context_fields,
        vec![
            "lane".to_string(),
            "principal".to_string(),
            "auth_context_summary".to_string(),
            "approval_state".to_string(),
        ]
    );
    assert_eq!(
        event.data.missing_context_fields,
        vec![
            "lane".to_string(),
            "principal".to_string(),
            "auth_context_summary".to_string(),
            "approval_state".to_string(),
        ]
    );
    assert!(event.data.approval_state.is_none());
    assert!(event.data.approval_id.is_none());
    assert!(event.data.approval_freshness.is_none());
}
