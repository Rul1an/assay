use crate::mcp::decision::{DecisionEmitter, DecisionEvent};
use crate::mcp::jsonrpc::JsonRpcRequest;
use crate::mcp::lifecycle::{LifecycleEmitter, LifecycleEvent};
use crate::mcp::policy::{
    FailClosedMode, McpPolicy, RedactArgsContract, RestrictScopeContract, ToolPolicy, ToolRiskClass,
};
use chrono::{Duration, Utc};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub(super) struct CountingEmitter(pub(super) AtomicUsize);

impl DecisionEmitter for CountingEmitter {
    fn emit(&self, _event: &DecisionEvent) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

pub(super) fn make_tool_call_request(tool: &str, args: Value) -> JsonRpcRequest {
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

pub(super) fn approval_required_policy() -> McpPolicy {
    McpPolicy {
        tools: ToolPolicy {
            approval_required: Some(vec!["deploy_*".to_string()]),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub(super) fn restrict_scope_policy_with_contract(contract: RestrictScopeContract) -> McpPolicy {
    McpPolicy {
        tools: ToolPolicy {
            restrict_scope: Some(vec!["deploy_*".to_string()]),
            restrict_scope_contract: Some(contract),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub(super) fn restrict_scope_policy() -> McpPolicy {
    restrict_scope_policy_with_contract(RestrictScopeContract {
        scope_type: "resource".to_string(),
        scope_value: "service/prod".to_string(),
        scope_match_mode: "exact".to_string(),
    })
}

pub(super) fn redact_args_policy_with_contract(contract: RedactArgsContract) -> McpPolicy {
    McpPolicy {
        tools: ToolPolicy {
            redact_args: Some(vec!["deploy_*".to_string()]),
            redact_args_contract: Some(contract),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub(super) fn redact_args_policy() -> McpPolicy {
    redact_args_policy_with_contract(RedactArgsContract {
        redaction_target: "body".to_string(),
        redaction_mode: "mask".to_string(),
        redaction_scope: "request".to_string(),
    })
}

pub(super) fn approval_artifact(
    bound_tool: &str,
    bound_resource: &str,
    expires_in_seconds: i64,
) -> Value {
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

pub(super) fn outcome_for<'a>(
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

pub(super) fn assert_fail_closed_defaults(event: &DecisionEvent) {
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

#[allow(dead_code)] // Prepared for future tests with mandate authorization
pub(super) struct CountingLifecycleEmitter(
    pub(super) AtomicUsize,
    pub(super) Mutex<Vec<LifecycleEvent>>,
);

impl LifecycleEmitter for CountingLifecycleEmitter {
    fn emit(&self, event: &LifecycleEvent) {
        self.0.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut events) = self.1.lock() {
            events.push(event.clone());
        }
    }
}
