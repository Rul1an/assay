use super::super::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use super::fixtures::{
    make_tool_call_request, redact_args_policy, redact_args_policy_with_contract, CountingEmitter,
};
use crate::mcp::decision::{reason_codes, ObligationOutcomeStatus};
use crate::mcp::policy::{PolicyState, RedactArgsContract};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

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
        HandleResult::Allow {
            effective_arguments,
            decision_event,
            ..
        } => {
            let redacted_args = effective_arguments.expect("redacted effective_arguments");
            assert_eq!(redacted_args["body"], serde_json::json!("[REDACTED]"));
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
                        && outcome.reason_code.as_deref() == Some("validated_in_handler")
                        && outcome.enforcement_stage.as_deref() == Some("handler")
                        && outcome.normalization_version.as_deref() == Some("v1")
                }));
        }
        other => panic!("expected allow result, got {:?}", other),
    }
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}
