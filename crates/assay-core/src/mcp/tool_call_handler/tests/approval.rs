use super::super::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use super::fixtures::{
    approval_artifact, approval_required_policy, assert_fail_closed_defaults,
    make_tool_call_request, outcome_for, CountingEmitter,
};
use crate::mcp::decision::{reason_codes, FulfillmentDecisionPath, ObligationOutcomeStatus};
use crate::mcp::policy::{ApprovalFreshness, PolicyState};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

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
                decision_event.data.fulfillment_decision_path,
                Some(FulfillmentDecisionPath::PolicyDeny)
            );
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}
