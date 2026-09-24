//! One decision, one instant: every time-dependent verdict in a tool-call decision is judged
//! against the instant passed to `handle_tool_call_at`, and that instant is the `time` of both
//! the emitted decision event and the event returned in `HandleResult`.
//!
//! The instants sit in 2030 so that a verdict taken against the wall clock instead diverges
//! from the expected one rather than coinciding with it.

use super::super::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use super::fixtures::{
    approval_artifact_at, approval_required_policy, make_tool_call_request, CapturingEmitter,
};
use crate::mcp::decision::{reason_codes, DecisionEvent};
use crate::mcp::policy::{ApprovalFreshness, McpPolicy, PolicyState};
use crate::runtime::{
    Authorizer, AuthzConfig, MandateData, MandateKind, MandateStore, OperationClass,
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use std::sync::Arc;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2030, 1, 1, 12, 0, 0).unwrap()
}

fn result_event(result: &HandleResult) -> &DecisionEvent {
    match result {
        HandleResult::Allow { decision_event, .. }
        | HandleResult::Deny { decision_event, .. }
        | HandleResult::Error { decision_event, .. } => decision_event,
    }
}

fn parse(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("RFC 3339 timestamp")
        .with_timezone(&Utc)
}

/// A reviewer's reading of the approval rule, from the event's own fields only.
fn recompute_freshness(event: &DecisionEvent) -> ApprovalFreshness {
    let at = parse(&event.time);
    let issued_at = parse(event.data.issued_at.as_deref().expect("issued_at retained"));
    let expires_at = parse(
        event
            .data
            .expires_at
            .as_deref()
            .expect("expires_at retained"),
    );
    if at > expires_at {
        ApprovalFreshness::Expired
    } else if at < issued_at {
        ApprovalFreshness::Stale
    } else {
        ApprovalFreshness::Fresh
    }
}

fn run_approval_at(now: DateTime<Utc>) -> (HandleResult, Vec<DecisionEvent>) {
    let emitter = Arc::new(CapturingEmitter::new());
    let handler = ToolCallHandler::new(
        approval_required_policy(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );
    let request = make_tool_call_request(
        "deploy_service",
        approval_artifact_at(
            "deploy_service",
            "service/prod",
            t0() - Duration::minutes(5),
            t0(),
        ),
    );
    let mut state = PolicyState::default();
    let result = handler.handle_tool_call_at(now, &request, &mut state, None, None, None, None);
    let emitted = emitter.0.lock().unwrap().clone();
    (result, emitted)
}

#[test]
fn approval_expiry_boundary_is_judged_at_the_decision_instant() {
    let (at_expiry, _) = run_approval_at(t0());
    assert!(
        matches!(at_expiry, HandleResult::Allow { .. }),
        "an approval is fresh at its own expires_at, got {at_expiry:?}"
    );

    let (after_expiry, _) = run_approval_at(t0() + Duration::seconds(1));
    match after_expiry {
        HandleResult::Deny {
            reason_code,
            decision_event,
            ..
        } => {
            assert_eq!(reason_code, reason_codes::P_APPROVAL_REQUIRED);
            assert_eq!(
                decision_event.data.approval_freshness,
                Some(ApprovalFreshness::Expired)
            );
        }
        other => panic!("expected deny one second after expiry, got {other:?}"),
    }
}

#[test]
fn decision_event_time_is_enough_to_recompute_the_freshness_verdict() {
    for now in [
        t0() - Duration::minutes(10),
        t0() - Duration::seconds(1),
        t0(),
        t0() + Duration::seconds(1),
    ] {
        let (result, emitted) = run_approval_at(now);
        assert_eq!(emitted.len(), 1, "exactly one emitted decision");

        for event in [result_event(&result), &emitted[0]] {
            assert_eq!(
                parse(&event.time),
                now,
                "event time is the decision instant"
            );
            assert_eq!(
                Some(recompute_freshness(event)),
                event.data.approval_freshness,
                "the recorded verdict recomputes from the record at {now}"
            );
        }
    }
}

#[test]
fn non_tool_call_error_records_the_decision_instant() {
    let emitter = Arc::new(CapturingEmitter::new());
    let handler = ToolCallHandler::new(
        McpPolicy::default(),
        None,
        emitter.clone(),
        ToolCallHandlerConfig::default(),
    );
    let mut request = make_tool_call_request("unused", serde_json::json!({}));
    request.method = "tools/list".to_string();
    let mut state = PolicyState::default();

    let result = handler.handle_tool_call_at(t0(), &request, &mut state, None, None, None, None);
    assert!(
        matches!(result, HandleResult::Error { .. }),
        "expected error for a non-tool-call request, got {result:?}"
    );
    let emitted = emitter.0.lock().unwrap().clone();
    assert_eq!(emitted.len(), 1, "exactly one emitted decision");
    for event in [result_event(&result), &emitted[0]] {
        assert_eq!(
            parse(&event.time),
            t0(),
            "event time is the decision instant"
        );
    }
}

fn mandate_expiring_at(expires_at: DateTime<Utc>) -> MandateData {
    MandateData {
        mandate_id: "sha256:mandate-instant".to_string(),
        mandate_kind: MandateKind::Intent,
        audience: "org/app".to_string(),
        issuer: "auth.org.com".to_string(),
        tool_patterns: vec!["search_*".to_string()],
        operation_class: Some(OperationClass::Read),
        transaction_ref: None,
        not_before: None,
        expires_at: Some(expires_at),
        single_use: false,
        max_uses: None,
        nonce: None,
        canonical_digest: "sha256:digest-instant".to_string(),
        key_id: "sha256:key-instant".to_string(),
    }
}

fn run_mandate_at(now: DateTime<Utc>) -> HandleResult {
    let authorizer = Authorizer::new(
        MandateStore::memory().expect("in-memory mandate store"),
        AuthzConfig {
            clock_skew_seconds: 30,
            expected_audience: "org/app".to_string(),
            trusted_issuers: vec!["auth.org.com".to_string()],
        },
    );
    let handler = ToolCallHandler::new(
        McpPolicy::default(),
        Some(authorizer),
        Arc::new(CapturingEmitter::new()),
        ToolCallHandlerConfig::default(),
    );
    let request = make_tool_call_request("search_products", serde_json::json!({}));
    let mut state = PolicyState::default();
    let mandate = mandate_expiring_at(t0());
    handler.handle_tool_call_at(now, &request, &mut state, None, None, Some(&mandate), None)
}

#[test]
fn mandate_validity_is_judged_at_the_decision_instant() {
    let before = t0() - Duration::seconds(1);
    let allowed = run_mandate_at(before);
    assert!(
        matches!(allowed, HandleResult::Allow { .. }),
        "mandate valid before expiry, got {allowed:?}"
    );
    assert_eq!(parse(&result_event(&allowed).time), before);

    let past_skew = t0() + Duration::seconds(31);
    match run_mandate_at(past_skew) {
        HandleResult::Deny {
            reason_code,
            decision_event,
            ..
        } => {
            assert_eq!(reason_code, reason_codes::M_EXPIRED);
            assert_eq!(parse(&decision_event.time), past_skew);
        }
        other => panic!("expected mandate expiry past the skew window, got {other:?}"),
    }
}
