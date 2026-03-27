use super::super::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use super::fixtures::{
    make_tool_call_request, restrict_scope_policy, restrict_scope_policy_with_contract,
    CountingEmitter,
};
use crate::mcp::decision::{reason_codes, ObligationOutcomeStatus};
use crate::mcp::policy::{PolicyState, RestrictScopeContract};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
                        && outcome.reason_code.as_deref() == Some("obligation_applied")
                        && outcome.enforcement_stage.as_deref() == Some("handler")
                        && outcome.normalization_version.as_deref() == Some("v1")
                }));
        }
        other => panic!("expected allow result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}
