use assay_core::mcp::decision::{DecisionEmitter, DecisionEvent};
use assay_core::mcp::jsonrpc::JsonRpcRequest;
use assay_core::mcp::policy::{McpPolicy, RedactArgsContract, RestrictScopeContract, ToolPolicy};
use chrono::{Duration, Utc};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Test emitter that counts and stores events.
pub(crate) struct TestEmitter {
    count: AtomicUsize,
    events: Mutex<Vec<DecisionEvent>>,
}

impl TestEmitter {
    pub(crate) fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            events: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn event_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    pub(crate) fn last_event(&self) -> Option<DecisionEvent> {
        self.events.lock().unwrap().last().cloned()
    }
}

impl DecisionEmitter for TestEmitter {
    fn emit(&self, event: &DecisionEvent) {
        self.count.fetch_add(1, Ordering::SeqCst);
        self.events.lock().unwrap().push(event.clone());
    }
}

pub(crate) fn make_tool_request(tool: &str) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: "tools/call".to_string(),
        params: serde_json::json!({
            "name": tool,
            "arguments": {}
        }),
    }
}

pub(crate) fn make_tool_request_with_args(tool: &str, args: Value) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: "tools/call".to_string(),
        params: serde_json::json!({
            "name": tool,
            "arguments": args
        }),
    }
}

#[allow(clippy::field_reassign_with_default)]
pub(crate) fn approval_required_policy() -> McpPolicy {
    let mut policy = McpPolicy::default();
    policy.tools = ToolPolicy {
        approval_required: Some(vec!["deploy_*".to_string()]),
        ..Default::default()
    };
    policy
}

pub(crate) fn restrict_scope_policy_with_contract(contract: RestrictScopeContract) -> McpPolicy {
    let mut policy = McpPolicy::default();
    policy.tools = ToolPolicy {
        restrict_scope: Some(vec!["deploy_*".to_string()]),
        restrict_scope_contract: Some(contract),
        ..Default::default()
    };
    policy
}

pub(crate) fn restrict_scope_policy() -> McpPolicy {
    restrict_scope_policy_with_contract(RestrictScopeContract {
        scope_type: "resource".to_string(),
        scope_value: "service/prod".to_string(),
        scope_match_mode: "exact".to_string(),
    })
}

pub(crate) fn redact_args_policy_with_contract(contract: RedactArgsContract) -> McpPolicy {
    let mut policy = McpPolicy::default();
    policy.tools = ToolPolicy {
        redact_args: Some(vec!["deploy_*".to_string()]),
        redact_args_contract: Some(contract),
        ..Default::default()
    };
    policy
}

pub(crate) fn redact_args_policy() -> McpPolicy {
    redact_args_policy_with_contract(RedactArgsContract {
        redaction_target: "body".to_string(),
        redaction_mode: "mask".to_string(),
        redaction_scope: "request".to_string(),
    })
}

pub(crate) fn approval_artifact(
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
                "approval_id": "apr_it_001",
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
