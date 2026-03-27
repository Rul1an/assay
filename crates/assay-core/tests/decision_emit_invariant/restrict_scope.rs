use crate::fixtures::{
    make_tool_request_with_args, restrict_scope_policy, restrict_scope_policy_with_contract,
    TestEmitter,
};
use assay_core::mcp::decision::{reason_codes, ObligationOutcomeStatus};
use assay_core::mcp::policy::{PolicyState, RestrictScopeContract};
use assay_core::mcp::tool_call_handler::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use std::sync::Arc;

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
