use crate::fixtures::{
    approval_artifact, approval_required_policy, make_tool_request_with_args, TestEmitter,
};
use assay_core::mcp::decision::reason_codes;
use assay_core::mcp::policy::{ApprovalFreshness, PolicyState};
use assay_core::mcp::tool_call_handler::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use std::sync::Arc;

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
