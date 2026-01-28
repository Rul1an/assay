//! Central tool call handler with mandate authorization.
//!
//! This module integrates policy evaluation, mandate authorization, and
//! decision emission into a single handler that guarantees the always-emit
//! invariant (I1).

use super::decision::{reason_codes, DecisionEmitter, DecisionEmitterGuard, DecisionEvent};
use super::identity::ToolIdentity;
use super::jsonrpc::JsonRpcRequest;
use super::policy::{McpPolicy, PolicyDecision, PolicyState};
use crate::runtime::{Authorizer, AuthzReceipt, MandateData, OperationClass, ToolCallData};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

/// Result of tool call handling.
#[derive(Debug)]
pub enum HandleResult {
    /// Tool call is allowed, forward to server
    Allow {
        receipt: Option<AuthzReceipt>,
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
    /// Tools classified as commit operations
    pub commit_tools: Vec<String>,
}

impl Default for ToolCallHandlerConfig {
    fn default() -> Self {
        Self {
            event_source: "assay://unknown".to_string(),
            require_mandate_for_commit: true,
            commit_tools: vec![],
        }
    }
}

/// Central tool call handler with integrated authorization.
pub struct ToolCallHandler {
    policy: McpPolicy,
    authorizer: Option<Authorizer>,
    emitter: Arc<dyn DecisionEmitter>,
    config: ToolCallHandlerConfig,
}

impl ToolCallHandler {
    /// Create a new handler.
    pub fn new(
        policy: McpPolicy,
        authorizer: Option<Authorizer>,
        emitter: Arc<dyn DecisionEmitter>,
        config: ToolCallHandlerConfig,
    ) -> Self {
        Self {
            policy,
            authorizer,
            emitter,
            config,
        }
    }

    /// Handle a tool call with full authorization and always-emit guarantee.
    ///
    /// This is the main entry point that enforces invariant I1: exactly one
    /// decision event is emitted for every tool call attempt.
    pub fn handle_tool_call(
        &self,
        request: &JsonRpcRequest,
        state: &mut PolicyState,
        runtime_identity: Option<&ToolIdentity>,
        mandate: Option<&MandateData>,
        transaction_object: Option<&Value>,
    ) -> HandleResult {
        let params = match request.tool_params() {
            Some(p) => p,
            None => {
                // Not a tool call - this shouldn't happen but handle gracefully
                return HandleResult::Error {
                    reason_code: reason_codes::S_INTERNAL_ERROR.to_string(),
                    reason: "Not a tool call".to_string(),
                    decision_event: DecisionEvent::new(
                        self.config.event_source.clone(),
                        "unknown".to_string(),
                        "unknown".to_string(),
                    )
                    .error(
                        reason_codes::S_INTERNAL_ERROR,
                        Some("Not a tool call".to_string()),
                    ),
                };
            }
        };

        let tool_name = params.name.clone();
        let tool_call_id = self.extract_tool_call_id(request);

        // Create guard - ensures decision is ALWAYS emitted
        let mut guard = DecisionEmitterGuard::new(
            self.emitter.clone(),
            self.config.event_source.clone(),
            tool_call_id.clone(),
            tool_name.clone(),
        );
        guard.set_request_id(request.id.clone());

        let start = Instant::now();

        // Step 1: Policy evaluation
        let policy_decision =
            self.policy
                .evaluate(&tool_name, &params.arguments, state, runtime_identity);

        match policy_decision {
            PolicyDecision::Deny {
                tool: _,
                code,
                reason,
                contract: _,
            } => {
                let reason_code = self.map_policy_code_to_reason(&code);
                guard.emit_deny(&reason_code, Some(reason.clone()));

                return HandleResult::Deny {
                    reason_code: reason_code.clone(),
                    reason: reason.clone(),
                    decision_event: DecisionEvent::new(
                        self.config.event_source.clone(),
                        tool_call_id,
                        tool_name,
                    )
                    .deny(&reason_code, Some(reason)),
                };
            }
            PolicyDecision::AllowWithWarning { .. } | PolicyDecision::Allow => {
                // Continue to mandate check
            }
        }

        // Step 2: Check if mandate is required
        let is_commit_tool = self.is_commit_tool(&tool_name);
        if is_commit_tool && self.config.require_mandate_for_commit && mandate.is_none() {
            guard.emit_deny(
                reason_codes::P_MANDATE_REQUIRED,
                Some("Commit tool requires mandate authorization".to_string()),
            );

            return HandleResult::Deny {
                reason_code: reason_codes::P_MANDATE_REQUIRED.to_string(),
                reason: "Commit tool requires mandate authorization".to_string(),
                decision_event: DecisionEvent::new(
                    self.config.event_source.clone(),
                    tool_call_id,
                    tool_name,
                )
                .deny(
                    reason_codes::P_MANDATE_REQUIRED,
                    Some("Commit tool requires mandate authorization".to_string()),
                ),
            };
        }

        // Step 3: Mandate authorization (if mandate present)
        if let (Some(authorizer), Some(mandate_data)) = (&self.authorizer, mandate) {
            let operation_class = if is_commit_tool {
                OperationClass::Commit
            } else {
                OperationClass::Read // TODO: Determine from tool classification
            };

            let tool_call_data = ToolCallData {
                tool_name: tool_name.clone(),
                tool_call_id: tool_call_id.clone(),
                operation_class,
                transaction_object: transaction_object.cloned(),
                source_run_id: None,
            };

            let authz_start = Instant::now();
            match authorizer.authorize_and_consume(mandate_data, &tool_call_data) {
                Ok(receipt) => {
                    let authz_ms = authz_start.elapsed().as_millis() as u64;
                    guard.set_mandate_info(
                        Some(mandate_data.mandate_id.clone()),
                        Some(receipt.use_id.clone()),
                        Some(receipt.use_count),
                    );
                    guard.set_mandate_matches(
                        Some(true),
                        Some(true),
                        transaction_object.map(|_| true),
                    );
                    guard.set_latencies(Some(authz_ms), None);
                    guard.emit_allow(reason_codes::P_MANDATE_VALID);

                    return HandleResult::Allow {
                        receipt: Some(receipt),
                        decision_event: DecisionEvent::new(
                            self.config.event_source.clone(),
                            tool_call_id,
                            tool_name,
                        )
                        .allow(reason_codes::P_MANDATE_VALID),
                    };
                }
                Err(e) => {
                    let (reason_code, reason) = self.map_authz_error(&e);
                    guard.set_mandate_info(Some(mandate_data.mandate_id.clone()), None, None);
                    guard.emit_deny(&reason_code, Some(reason.clone()));

                    return HandleResult::Deny {
                        reason_code,
                        reason,
                        decision_event: DecisionEvent::new(
                            self.config.event_source.clone(),
                            tool_call_id,
                            tool_name,
                        ),
                    };
                }
            }
        }

        // Step 4: No mandate required, policy allows
        let elapsed_ms = start.elapsed().as_millis() as u64;
        guard.set_latencies(Some(elapsed_ms), None);
        guard.emit_allow(reason_codes::P_POLICY_DENY); // Actually P_POLICY_ALLOW but we use P_POLICY_DENY as catch-all

        HandleResult::Allow {
            receipt: None,
            decision_event: DecisionEvent::new(
                self.config.event_source.clone(),
                tool_call_id,
                tool_name,
            )
            .allow(reason_codes::P_POLICY_DENY),
        }
    }

    /// Extract tool_call_id from request (I4: idempotency key).
    fn extract_tool_call_id(&self, request: &JsonRpcRequest) -> String {
        // Try to get from params._meta.tool_call_id (MCP standard)
        if let Some(params) = request.tool_params() {
            if let Some(meta) = params.arguments.get("_meta") {
                if let Some(id) = meta.get("tool_call_id").and_then(|v| v.as_str()) {
                    return id.to_string();
                }
            }
        }

        // Fall back to request.id if present
        if let Some(id) = &request.id {
            if let Some(s) = id.as_str() {
                return format!("req_{}", s);
            }
            if let Some(n) = id.as_i64() {
                return format!("req_{}", n);
            }
        }

        // Generate one if none found
        format!("gen_{}", uuid::Uuid::new_v4())
    }

    /// Check if a tool is classified as a commit operation.
    fn is_commit_tool(&self, tool_name: &str) -> bool {
        self.config.commit_tools.iter().any(|pattern| {
            if pattern == "*" {
                return true;
            }
            if pattern.ends_with('*') {
                let prefix = pattern.trim_end_matches('*');
                tool_name.starts_with(prefix)
            } else {
                tool_name == pattern
            }
        })
    }

    /// Map policy error code to reason code.
    fn map_policy_code_to_reason(&self, code: &str) -> String {
        match code {
            "E_TOOL_DENIED" => reason_codes::P_TOOL_DENIED.to_string(),
            "E_TOOL_NOT_ALLOWED" => reason_codes::P_TOOL_NOT_ALLOWED.to_string(),
            "E_ARG_SCHEMA" => reason_codes::P_ARG_SCHEMA.to_string(),
            "E_RATE_LIMIT" => reason_codes::P_RATE_LIMIT.to_string(),
            "E_TOOL_DRIFT" => reason_codes::P_TOOL_DRIFT.to_string(),
            _ => reason_codes::P_POLICY_DENY.to_string(),
        }
    }

    /// Map authorization error to reason code and message.
    fn map_authz_error(&self, error: &crate::runtime::AuthorizeError) -> (String, String) {
        use crate::runtime::AuthorizeError;

        match error {
            AuthorizeError::Policy(pe) => {
                use crate::runtime::PolicyError;
                match pe {
                    PolicyError::Expired { .. } => (
                        reason_codes::M_EXPIRED.to_string(),
                        "Mandate expired".to_string(),
                    ),
                    PolicyError::NotYetValid { .. } => (
                        reason_codes::M_NOT_YET_VALID.to_string(),
                        "Mandate not yet valid".to_string(),
                    ),
                    PolicyError::ToolNotInScope { tool } => (
                        reason_codes::M_TOOL_NOT_IN_SCOPE.to_string(),
                        format!("Tool '{}' not in mandate scope", tool),
                    ),
                    PolicyError::KindMismatch { kind, op_class } => (
                        reason_codes::M_KIND_MISMATCH.to_string(),
                        format!(
                            "Mandate kind '{}' does not allow operation class '{}'",
                            kind, op_class
                        ),
                    ),
                    PolicyError::AudienceMismatch { expected, actual } => (
                        reason_codes::M_AUDIENCE_MISMATCH.to_string(),
                        format!(
                            "Audience mismatch: expected '{}', got '{}'",
                            expected, actual
                        ),
                    ),
                    PolicyError::IssuerNotTrusted { issuer } => (
                        reason_codes::M_ISSUER_NOT_TRUSTED.to_string(),
                        format!("Issuer '{}' not in trusted list", issuer),
                    ),
                    PolicyError::MissingTransactionObject => (
                        reason_codes::M_TRANSACTION_REF_MISMATCH.to_string(),
                        "Transaction object required but not provided".to_string(),
                    ),
                    PolicyError::TransactionRefMismatch { expected, actual } => (
                        reason_codes::M_TRANSACTION_REF_MISMATCH.to_string(),
                        format!(
                            "Transaction ref mismatch: expected '{}', computed '{}'",
                            expected, actual
                        ),
                    ),
                }
            }
            AuthorizeError::Store(se) => {
                use crate::runtime::AuthzError;
                match se {
                    AuthzError::AlreadyUsed => (
                        reason_codes::M_ALREADY_USED.to_string(),
                        "Single-use mandate already consumed".to_string(),
                    ),
                    AuthzError::MaxUsesExceeded { max, current } => (
                        reason_codes::M_MAX_USES_EXCEEDED.to_string(),
                        format!("Max uses exceeded: {} of {} used", current, max),
                    ),
                    AuthzError::NonceReplay { nonce } => (
                        reason_codes::M_NONCE_REPLAY.to_string(),
                        format!("Nonce replay detected: {}", nonce),
                    ),
                    AuthzError::MandateNotFound { mandate_id } => (
                        reason_codes::M_NOT_FOUND.to_string(),
                        format!("Mandate not found: {}", mandate_id),
                    ),
                    AuthzError::MandateConflict { .. }
                    | AuthzError::InvalidConstraints { .. }
                    | AuthzError::Database(_) => (
                        reason_codes::S_DB_ERROR.to_string(),
                        format!("Database error: {}", se),
                    ),
                }
            }
            AuthorizeError::TransactionRef(msg) => (
                reason_codes::M_TRANSACTION_REF_MISMATCH.to_string(),
                format!("Transaction ref error: {}", msg),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::decision::NullDecisionEmitter;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingEmitter(AtomicUsize);

    impl DecisionEmitter for CountingEmitter {
        fn emit(&self, _event: &DecisionEvent) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn make_tool_call_request(tool: &str, args: Value) -> JsonRpcRequest {
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

    #[test]
    fn test_handler_emits_decision_on_policy_deny() {
        let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
        let policy = McpPolicy {
            tools: super::super::policy::ToolPolicy {
                allow: None,
                deny: Some(vec!["dangerous_*".to_string()]),
            },
            ..Default::default()
        };

        let handler = ToolCallHandler::new(
            policy,
            None,
            emitter.clone(),
            ToolCallHandlerConfig::default(),
        );

        let request = make_tool_call_request("dangerous_tool", serde_json::json!({}));
        let mut state = PolicyState::default();

        let result = handler.handle_tool_call(&request, &mut state, None, None, None);

        assert!(matches!(result, HandleResult::Deny { .. }));
        assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_handler_emits_decision_on_policy_allow() {
        let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
        let policy = McpPolicy::default();

        let handler = ToolCallHandler::new(
            policy,
            None,
            emitter.clone(),
            ToolCallHandlerConfig::default(),
        );

        let request = make_tool_call_request("safe_tool", serde_json::json!({}));
        let mut state = PolicyState::default();

        let result = handler.handle_tool_call(&request, &mut state, None, None, None);

        assert!(matches!(result, HandleResult::Allow { .. }));
        assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_commit_tool_without_mandate_denied() {
        let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
        let policy = McpPolicy::default();

        let config = ToolCallHandlerConfig {
            event_source: "assay://test".to_string(),
            require_mandate_for_commit: true,
            commit_tools: vec!["purchase_*".to_string()],
        };

        let handler = ToolCallHandler::new(policy, None, emitter.clone(), config);

        let request = make_tool_call_request("purchase_item", serde_json::json!({}));
        let mut state = PolicyState::default();

        let result = handler.handle_tool_call(&request, &mut state, None, None, None);

        assert!(
            matches!(result, HandleResult::Deny { reason_code, .. } if reason_code == reason_codes::P_MANDATE_REQUIRED)
        );
        assert_eq!(emitter.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_is_commit_tool_matching() {
        let config = ToolCallHandlerConfig {
            commit_tools: vec!["purchase_*".to_string(), "delete_account".to_string()],
            ..Default::default()
        };

        let handler = ToolCallHandler::new(
            McpPolicy::default(),
            None,
            Arc::new(NullDecisionEmitter),
            config,
        );

        assert!(handler.is_commit_tool("purchase_item"));
        assert!(handler.is_commit_tool("purchase_subscription"));
        assert!(handler.is_commit_tool("delete_account"));
        assert!(!handler.is_commit_tool("search_products"));
        assert!(!handler.is_commit_tool("purchase")); // Doesn't match purchase_*
    }
}
