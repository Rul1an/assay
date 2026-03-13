use super::super::decision::{DecisionEmitter, DecisionEvent};
use super::super::lifecycle::LifecycleEmitter;
use super::super::policy::McpPolicy;
use crate::runtime::{Authorizer, AuthzReceipt};
use std::sync::Arc;

/// Result of tool call handling.
#[derive(Debug)]
pub enum HandleResult {
    /// Tool call is allowed, forward to server
    Allow {
        receipt: Option<AuthzReceipt>,
        /// Runtime-redacted arguments when redact_args enforcement changed payload.
        /// `None` means original args are unchanged.
        effective_arguments: Option<serde_json::Value>,
        decision_event: DecisionEvent,
    },
    /// Tool call is denied, return error response
    Deny {
        reason_code: String,
        reason: String,
        decision_event: DecisionEvent,
    },
    /// Internal error during handling
    Error {
        reason_code: String,
        reason: String,
        decision_event: DecisionEvent,
    },
}

/// Configuration for the tool call handler.
#[derive(Clone)]
pub struct ToolCallHandlerConfig {
    /// Event source URI (I3: fixed, configured value)
    pub event_source: String,
    /// Whether commit tools require mandates
    pub require_mandate_for_commit: bool,
    /// Tools classified as commit operations (glob: "prefix*" or exact)
    pub commit_tools: Vec<String>,
    /// Tools classified as write operations (non-commit; glob or exact). Used for mandate operation_class.
    pub write_tools: Vec<String>,
}

impl Default for ToolCallHandlerConfig {
    fn default() -> Self {
        Self {
            event_source: "assay://unknown".to_string(),
            require_mandate_for_commit: true,
            commit_tools: vec![],
            write_tools: vec![],
        }
    }
}

/// Central tool call handler with integrated authorization.
pub struct ToolCallHandler {
    pub(crate) policy: McpPolicy,
    pub(crate) authorizer: Option<Authorizer>,
    pub(crate) emitter: Arc<dyn DecisionEmitter>,
    /// Emitter for mandate lifecycle events (audit log)
    pub(crate) lifecycle_emitter: Option<Arc<dyn LifecycleEmitter>>,
    pub(crate) config: ToolCallHandlerConfig,
}

pub(super) fn new_handler(
    policy: McpPolicy,
    authorizer: Option<Authorizer>,
    emitter: Arc<dyn DecisionEmitter>,
    config: ToolCallHandlerConfig,
) -> ToolCallHandler {
    ToolCallHandler {
        policy,
        authorizer,
        emitter,
        lifecycle_emitter: None,
        config,
    }
}

pub(super) fn with_lifecycle_emitter(
    mut handler: ToolCallHandler,
    emitter: Arc<dyn LifecycleEmitter>,
) -> ToolCallHandler {
    handler.lifecycle_emitter = Some(emitter);
    handler
}
