use super::*;
use crate::mcp::decision::{
    reason_codes, DecisionEvent, NullDecisionEmitter, ObligationOutcomeStatus,
};
use crate::mcp::identity::ToolIdentity;
use crate::mcp::lifecycle::{LifecycleEmitter, LifecycleEvent};
use crate::mcp::policy::{
    ApprovalFreshness, FailClosedMode, RedactArgsContract, RestrictScopeContract, ToolPolicy,
    ToolRiskClass, TypedPolicyDecision,
};
use chrono::{Duration, Utc};
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

fn approval_required_policy() -> McpPolicy {
    McpPolicy {
        tools: ToolPolicy {
            approval_required: Some(vec!["deploy_*".to_string()]),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn restrict_scope_policy_with_contract(contract: RestrictScopeContract) -> McpPolicy {
    McpPolicy {
        tools: ToolPolicy {
            restrict_scope: Some(vec!["deploy_*".to_string()]),
            restrict_scope_contract: Some(contract),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn restrict_scope_policy() -> McpPolicy {
    restrict_scope_policy_with_contract(RestrictScopeContract {
        scope_type: "resource".to_string(),
        scope_value: "service/prod".to_string(),
        scope_match_mode: "exact".to_string(),
    })
}

fn redact_args_policy_with_contract(contract: RedactArgsContract) -> McpPolicy {
    McpPolicy {
        tools: ToolPolicy {
            redact_args: Some(vec!["deploy_*".to_string()]),
            redact_args_contract: Some(contract),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn redact_args_policy() -> McpPolicy {
    redact_args_policy_with_contract(RedactArgsContract {
        redaction_target: "body".to_string(),
        redaction_mode: "mask".to_string(),
        redaction_scope: "request".to_string(),
    })
}

fn approval_artifact(bound_tool: &str, bound_resource: &str, expires_in_seconds: i64) -> Value {
    let issued_at = Utc::now() - Duration::minutes(5);
    let expires_at = Utc::now() + Duration::seconds(expires_in_seconds);
    serde_json::json!({
        "_meta": {
            "resource": "service/prod",
            "approval": {
                "approval_id": "apr_test_001",
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

fn outcome_for<'a>(
    event: &'a DecisionEvent,
    obligation_type: &str,
) -> &'a crate::mcp::decision::ObligationOutcome {
    event
        .data
        .obligation_outcomes
        .iter()
        .find(|outcome| outcome.obligation_type == obligation_type)
        .expect("expected obligation outcome")
}

fn assert_fail_closed_defaults(event: &DecisionEvent) {
    let context = event
        .data
        .fail_closed
        .as_ref()
        .expect("expected fail_closed context");
    assert_eq!(context.tool_risk_class, ToolRiskClass::LowRiskRead);
    assert_eq!(context.fail_closed_mode, FailClosedMode::DegradeReadOnly);
    assert_eq!(context.fail_closed_trigger, None);
    assert!(!context.fail_closed_applied);
    assert!(context.fail_closed_error_code.is_none());
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
        }
        other => panic!("expected allow result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
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
            assert!(outcome.reason_code.is_none());
            assert_eq!(outcome.enforcement_stage.as_deref(), Some("executor"));
            assert_eq!(outcome.normalization_version.as_deref(), Some("v1"));
        }
        other => panic!("expected deny result, got {:?}", other),
    }

    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn test_alert_obligation_outcome_emitted() {
    test_tool_drift_deny_emits_alert_obligation_outcome();
}

#[test]
fn approval_required_missing_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        approval_required_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request(
        "deploy_service",
        serde_json::json!({
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_APPROVAL_REQUIRED);
            assert_eq!(reason, "missing approval");
            assert_fail_closed_defaults(&decision_event);
            assert_eq!(
                decision_event.data.approval_failure_reason.as_deref(),
                Some("missing approval")
            );
            assert_eq!(
                decision_event.data.approval_state.as_deref(),
                Some("denied")
            );
            assert_eq!(decision_event.data.approval_freshness, None);
            let outcome = outcome_for(&decision_event, "approval_required");
            assert_eq!(outcome.status, ObligationOutcomeStatus::Error);
            assert_eq!(outcome.reason.as_deref(), Some("missing approval"));
            assert_eq!(outcome.reason_code.as_deref(), Some("approval_missing"));
            assert_eq!(outcome.enforcement_stage.as_deref(), Some("handler"));
            assert_eq!(outcome.normalization_version.as_deref(), Some("v1"));
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn approval_required_expired_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        approval_required_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request(
        "deploy_service",
        approval_artifact("deploy_service", "service/prod", -30),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_APPROVAL_REQUIRED);
            assert_eq!(reason, "expired approval");
            assert_fail_closed_defaults(&decision_event);
            assert_eq!(
                decision_event.data.approval_failure_reason.as_deref(),
                Some("expired approval")
            );
            assert_eq!(
                decision_event.data.approval_state.as_deref(),
                Some("denied")
            );
            assert_eq!(
                decision_event.data.approval_freshness,
                Some(ApprovalFreshness::Expired)
            );
            let outcome = outcome_for(&decision_event, "approval_required");
            assert_eq!(outcome.status, ObligationOutcomeStatus::Error);
            assert_eq!(outcome.reason.as_deref(), Some("expired approval"));
            assert_eq!(outcome.reason_code.as_deref(), Some("approval_expired"));
            assert_eq!(outcome.enforcement_stage.as_deref(), Some("handler"));
            assert_eq!(outcome.normalization_version.as_deref(), Some("v1"));
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn approval_required_bound_tool_mismatch_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        approval_required_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request(
        "deploy_service",
        approval_artifact("deploy_other", "service/prod", 300),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_APPROVAL_REQUIRED);
            assert_eq!(reason, "bound tool mismatch");
            assert_fail_closed_defaults(&decision_event);
            assert_eq!(
                decision_event.data.approval_failure_reason.as_deref(),
                Some("bound tool mismatch")
            );
            assert_eq!(
                decision_event.data.approval_state.as_deref(),
                Some("denied")
            );
            assert_eq!(
                decision_event.data.approval_freshness,
                Some(ApprovalFreshness::Fresh)
            );
            let outcome = outcome_for(&decision_event, "approval_required");
            assert_eq!(outcome.status, ObligationOutcomeStatus::Error);
            assert_eq!(outcome.reason.as_deref(), Some("bound tool mismatch"));
            assert_eq!(
                outcome.reason_code.as_deref(),
                Some("approval_bound_tool_mismatch")
            );
            assert_eq!(outcome.enforcement_stage.as_deref(), Some("handler"));
            assert_eq!(outcome.normalization_version.as_deref(), Some("v1"));
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn approval_required_bound_resource_mismatch_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        approval_required_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request(
        "deploy_service",
        approval_artifact("deploy_service", "service/staging", 300),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_APPROVAL_REQUIRED);
            assert_eq!(reason, "bound resource mismatch");
            assert_fail_closed_defaults(&decision_event);
            assert_eq!(
                decision_event.data.approval_failure_reason.as_deref(),
                Some("bound resource mismatch")
            );
            assert_eq!(
                decision_event.data.approval_state.as_deref(),
                Some("denied")
            );
            assert_eq!(
                decision_event.data.approval_freshness,
                Some(ApprovalFreshness::Fresh)
            );
            let outcome = outcome_for(&decision_event, "approval_required");
            assert_eq!(outcome.status, ObligationOutcomeStatus::Error);
            assert_eq!(outcome.reason.as_deref(), Some("bound resource mismatch"));
            assert_eq!(
                outcome.reason_code.as_deref(),
                Some("approval_bound_resource_mismatch")
            );
            assert_eq!(outcome.enforcement_stage.as_deref(), Some("handler"));
            assert_eq!(outcome.normalization_version.as_deref(), Some("v1"));
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn restrict_scope_mismatch_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        restrict_scope_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request(
        "deploy_service",
        serde_json::json!({
            "_meta": {
                "resource": "service/staging"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_RESTRICT_SCOPE);
            assert_eq!(reason, "scope target mismatch");
            assert_eq!(decision_event.data.restrict_scope_present, Some(true));
            assert_eq!(decision_event.data.scope_type.as_deref(), Some("resource"));
            assert_eq!(
                decision_event.data.scope_value.as_deref(),
                Some("service/prod")
            );
            assert_eq!(
                decision_event.data.scope_match_mode.as_deref(),
                Some("exact")
            );
            assert_eq!(
                decision_event.data.scope_evaluation_state.as_deref(),
                Some("mismatch")
            );
            assert_eq!(decision_event.data.restrict_scope_match, Some(false));
            assert_eq!(
                decision_event.data.scope_failure_reason.as_deref(),
                Some("scope_target_mismatch")
            );
            assert_eq!(
                decision_event.data.restrict_scope_reason.as_deref(),
                Some("scope_target_mismatch")
            );
            assert!(decision_event
                .data
                .obligation_outcomes
                .iter()
                .any(|outcome| {
                    outcome.obligation_type == "restrict_scope"
                        && outcome.status == ObligationOutcomeStatus::Error
                        && outcome.reason.as_deref() == Some("scope_target_mismatch")
                        && outcome.reason_code.as_deref() == Some("scope_target_mismatch")
                        && outcome.enforcement_stage.as_deref() == Some("handler")
                        && outcome.normalization_version.as_deref() == Some("v1")
                }));
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn restrict_scope_mismatch_does_not_deny() {
    // Compatibility alias for older gate scripts; semantics are covered by the deny test above.
    restrict_scope_mismatch_denies();
}

#[test]
fn restrict_scope_match_sets_additive_fields() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        restrict_scope_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request(
        "deploy_service",
        serde_json::json!({
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Allow { decision_event, .. } => {
            assert_eq!(decision_event.data.restrict_scope_present, Some(true));
            assert_eq!(decision_event.data.restrict_scope_match, Some(true));
            assert_eq!(
                decision_event.data.scope_evaluation_state.as_deref(),
                Some("matched")
            );
            assert!(decision_event.data.restrict_scope_reason.is_none());
            assert!(decision_event
                .data
                .obligation_outcomes
                .iter()
                .any(|outcome| {
                    outcome.obligation_type == "restrict_scope"
                        && outcome.status == ObligationOutcomeStatus::Applied
                        && outcome.reason_code.is_none()
                        && outcome.enforcement_stage.as_deref() == Some("handler")
                        && outcome.normalization_version.as_deref() == Some("v1")
                }));
        }
        other => panic!("expected allow result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn restrict_scope_target_missing_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        restrict_scope_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request("deploy_service", serde_json::json!({}));
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_RESTRICT_SCOPE);
            assert_eq!(reason, "scope target missing");
            assert_eq!(
                decision_event.data.scope_failure_reason.as_deref(),
                Some("scope_target_missing")
            );
            assert_eq!(
                decision_event.data.restrict_scope_reason.as_deref(),
                Some("scope_target_missing")
            );
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn restrict_scope_unsupported_match_mode_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
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

    let request = make_tool_call_request(
        "deploy_service",
        serde_json::json!({
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_RESTRICT_SCOPE);
            assert_eq!(reason, "scope match mode unsupported");
            assert_eq!(
                decision_event.data.scope_evaluation_state.as_deref(),
                Some("not_evaluated")
            );
            assert_eq!(
                decision_event.data.scope_failure_reason.as_deref(),
                Some("scope_match_mode_unsupported")
            );
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn restrict_scope_unsupported_scope_type_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
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

    let request = make_tool_call_request(
        "deploy_service",
        serde_json::json!({
            "_meta": {
                "resource": "service/prod"
            }
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_RESTRICT_SCOPE);
            assert_eq!(reason, "scope type unsupported");
            assert_eq!(
                decision_event.data.scope_evaluation_state.as_deref(),
                Some("not_evaluated")
            );
            assert_eq!(
                decision_event.data.scope_failure_reason.as_deref(),
                Some("scope_type_unsupported")
            );
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn redact_args_contract_sets_additive_fields() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        redact_args_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request(
        "deploy_service",
        serde_json::json!({
            "body": {"secret": "token-123"},
            "_meta": {"resource": "service/prod"}
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Allow { decision_event, .. } => {
            assert_eq!(
                decision_event.data.redaction_target.as_deref(),
                Some("body")
            );
            assert_eq!(decision_event.data.redaction_mode.as_deref(), Some("mask"));
            assert_eq!(
                decision_event.data.redaction_scope.as_deref(),
                Some("request")
            );
            assert_eq!(
                decision_event.data.redaction_applied_state.as_deref(),
                Some("applied")
            );
            assert!(decision_event.data.redaction_reason.is_none());
            assert!(decision_event.data.redaction_failure_reason.is_none());
            assert_eq!(decision_event.data.redact_args_present, Some(true));
            assert_eq!(
                decision_event.data.redact_args_target.as_deref(),
                Some("body")
            );
            assert_eq!(
                decision_event.data.redact_args_mode.as_deref(),
                Some("mask")
            );
            assert_eq!(
                decision_event.data.redact_args_result.as_deref(),
                Some("applied")
            );
            assert!(decision_event.data.redact_args_reason.is_none());
            assert!(decision_event
                .data
                .obligation_outcomes
                .iter()
                .any(|outcome| {
                    outcome.obligation_type == "redact_args"
                        && outcome.status == ObligationOutcomeStatus::Applied
                        && outcome.reason.is_none()
                        && outcome.reason_code.is_none()
                        && outcome.enforcement_stage.as_deref() == Some("handler")
                        && outcome.normalization_version.as_deref() == Some("v1")
                }));
        }
        other => panic!("expected allow result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn redact_args_target_missing_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let handler = ToolCallHandler::new(
        redact_args_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );

    let request = make_tool_call_request(
        "deploy_service",
        serde_json::json!({
            "_meta": {"resource": "service/prod"}
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_REDACT_ARGS);
            assert_eq!(reason, "redaction target missing");
            assert_eq!(
                decision_event.data.redaction_applied_state.as_deref(),
                Some("not_applied")
            );
            assert_eq!(
                decision_event.data.redaction_failure_reason.as_deref(),
                Some("redaction_target_missing")
            );
            assert!(decision_event
                .data
                .obligation_outcomes
                .iter()
                .any(|outcome| {
                    outcome.obligation_type == "redact_args"
                        && outcome.status == ObligationOutcomeStatus::Error
                        && outcome.reason.as_deref() == Some("redaction_target_missing")
                        && outcome.reason_code.as_deref() == Some("redaction_target_missing")
                        && outcome.enforcement_stage.as_deref() == Some("handler")
                        && outcome.normalization_version.as_deref() == Some("v1")
                }));
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn redact_args_mode_unsupported_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
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

    let request = make_tool_call_request(
        "deploy_service",
        serde_json::json!({
            "body": {"secret": "token-123"},
            "_meta": {"resource": "service/prod"}
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_REDACT_ARGS);
            assert_eq!(reason, "redaction mode unsupported");
            assert_eq!(
                decision_event.data.redaction_applied_state.as_deref(),
                Some("not_evaluated")
            );
            assert_eq!(
                decision_event.data.redaction_failure_reason.as_deref(),
                Some("redaction_mode_unsupported")
            );
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn redact_args_scope_unsupported_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
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

    let request = make_tool_call_request(
        "deploy_service",
        serde_json::json!({
            "body": {"secret": "token-123"},
            "_meta": {"resource": "service/prod"}
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_REDACT_ARGS);
            assert_eq!(reason, "redaction scope unsupported");
            assert_eq!(
                decision_event.data.redaction_applied_state.as_deref(),
                Some("not_evaluated")
            );
            assert_eq!(
                decision_event.data.redaction_failure_reason.as_deref(),
                Some("redaction_scope_unsupported")
            );
        }
        other => panic!("expected deny result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn redact_args_apply_failed_denies() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
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

    let request = make_tool_call_request(
        "deploy_service",
        serde_json::json!({
            "body": {"secret": "token-123"},
            "_meta": {"resource": "service/prod"}
        }),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    match result {
        HandleResult::Deny {
            reason_code,
            reason,
            decision_event,
        } => {
            assert_eq!(reason_code, reason_codes::P_REDACT_ARGS);
            assert_eq!(reason, "redaction apply failed");
            assert_eq!(
                decision_event.data.redaction_applied_state.as_deref(),
                Some("not_applied")
            );
            assert_eq!(
                decision_event.data.redaction_failure_reason.as_deref(),
                Some("redaction_apply_failed")
            );
        }
        other => panic!("expected deny result, got {:?}", other),
    }
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
