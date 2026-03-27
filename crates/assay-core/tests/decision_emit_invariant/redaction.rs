use crate::fixtures::{
    make_tool_request_with_args, redact_args_policy, redact_args_policy_with_contract, TestEmitter,
};
use assay_core::mcp::decision::{reason_codes, ObligationOutcomeStatus};
use assay_core::mcp::policy::{PolicyState, RedactArgsContract};
use assay_core::mcp::tool_call_handler::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use std::sync::Arc;

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
    match result {
        HandleResult::Allow {
            effective_arguments,
            ..
        } => {
            let redacted_args = effective_arguments.expect("redacted effective_arguments");
            assert_eq!(redacted_args["body"], serde_json::json!("[REDACTED]"));
        }
        other => panic!("expected allow result, got {:?}", other),
    }
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
            && outcome.reason_code.as_deref() == Some("validated_in_handler")
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
