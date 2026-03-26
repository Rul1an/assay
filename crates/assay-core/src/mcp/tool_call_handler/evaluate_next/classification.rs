use super::super::super::decision::reason_codes;
use super::super::super::jsonrpc::JsonRpcRequest;
use super::super::types::ToolCallHandler;
use crate::runtime::{AuthorizeError, OperationClass};
use serde_json::Value;

pub(super) fn requested_resource(args: &Value) -> Option<&str> {
    args.get("_meta")
        .and_then(|meta| meta.get("resource"))
        .and_then(Value::as_str)
        .or_else(|| args.get("resource").and_then(Value::as_str))
}

impl ToolCallHandler {
    /// Extract tool_call_id from request (I4: idempotency key).
    pub(in crate::mcp::tool_call_handler) fn extract_tool_call_id(
        &self,
        request: &JsonRpcRequest,
    ) -> String {
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
    pub(in crate::mcp::tool_call_handler) fn is_commit_tool(&self, tool_name: &str) -> bool {
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

    /// Check if a tool is classified as a write operation (non-commit).
    fn is_write_tool(&self, tool_name: &str) -> bool {
        self.config.write_tools.iter().any(|pattern| {
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

    /// Derive operation class from tool classification (commit_tools, write_tools, else Read).
    pub(in crate::mcp::tool_call_handler) fn operation_class_for_tool(
        &self,
        tool_name: &str,
    ) -> OperationClass {
        if self.is_commit_tool(tool_name) {
            OperationClass::Commit
        } else if self.is_write_tool(tool_name) {
            OperationClass::Write
        } else {
            OperationClass::Read
        }
    }

    /// Map policy error code to reason code.
    pub(in crate::mcp::tool_call_handler) fn map_policy_code_to_reason(
        &self,
        code: &str,
    ) -> String {
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
    pub(in crate::mcp::tool_call_handler) fn map_authz_error(
        &self,
        error: &AuthorizeError,
    ) -> (String, String) {
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
                    AuthzError::Revoked { revoked_at } => (
                        reason_codes::M_REVOKED.to_string(),
                        format!("Mandate revoked at {}", revoked_at),
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
