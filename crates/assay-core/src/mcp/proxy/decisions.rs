use crate::mcp::audit::{AuditEvent, AuditLog};
use crate::mcp::decision::{
    reason_codes, refresh_contract_projections, Decision, DecisionEmitter, DecisionEvent,
};
use crate::mcp::jsonrpc::{CallToolParams, JsonRpcRequest};
use crate::mcp::policy::PolicyMatchMetadata;
use crate::mcp::tool_definition::ToolDefinitionBinding;
use std::sync::Arc;

pub(super) fn handle_allow(
    req: &JsonRpcRequest,
    tool_params: Option<&CallToolParams>,
    audit_log: &mut AuditLog,
    verbose: bool,
) {
    if verbose && req.is_tool_call() {
        let tool = tool_params.map(|p| p.name.as_str()).unwrap_or("unknown");
        eprintln!("[assay] ALLOW {}", tool);
    }

    if req.is_tool_call() {
        let tool = tool_params.map(|p| p.name.clone());
        audit_log.log(&AuditEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            decision: "allow".to_string(),
            tool,
            reason: None,
            request_id: req.id.clone(),
            agentic: None,
        });
    }
}

/// Extract tool_call_id from request (I4: idempotency key).
pub(super) fn extract_tool_call_id(
    request: &JsonRpcRequest,
    tool_params: Option<&CallToolParams>,
) -> String {
    // Try to get from params._meta.tool_call_id (MCP standard)
    if let Some(params) = tool_params {
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

/// Map policy error code to reason code.
pub(super) fn map_policy_code(code: &str) -> String {
    match code {
        "E_TOOL_DENIED" => reason_codes::P_TOOL_DENIED.to_string(),
        "E_TOOL_NOT_ALLOWED" => reason_codes::P_TOOL_NOT_ALLOWED.to_string(),
        "E_ARG_SCHEMA" => reason_codes::P_ARG_SCHEMA.to_string(),
        "E_RATE_LIMIT" => reason_codes::P_RATE_LIMIT.to_string(),
        "E_TOOL_DRIFT" => reason_codes::P_TOOL_DRIFT.to_string(),
        _ => reason_codes::P_POLICY_DENY.to_string(),
    }
}

/// Emit a decision event (I1: always emit).
#[expect(clippy::too_many_arguments)]
pub(super) fn emit_decision(
    emitter: &Arc<dyn DecisionEmitter>,
    source: &str,
    tool_call_id: &str,
    tool: &str,
    decision: Decision,
    reason_code: &str,
    reason: Option<String>,
    request_id: Option<serde_json::Value>,
    metadata: &PolicyMatchMetadata,
    tool_definition_binding: Option<&ToolDefinitionBinding>,
) {
    let mut event = DecisionEvent::new(
        source.to_string(),
        tool_call_id.to_string(),
        tool.to_string(),
    );
    event.data.decision = decision;
    event.data.reason_code = reason_code.to_string();
    event.data.reason = reason;
    event.data.request_id = request_id;
    event.data.tool_classes = metadata.tool_classes.clone();
    event.data.matched_tool_classes = metadata.matched_tool_classes.clone();
    event.data.match_basis = metadata.match_basis.as_str().map(ToString::to_string);
    event.data.matched_rule = metadata.matched_rule.clone();
    event.data.typed_decision = metadata.typed_decision;
    event.data.policy_version = metadata.policy_version.clone();
    event.data.policy_digest = metadata.policy_digest.clone();
    event.data.apply_policy_snapshot_projection();
    event
        .data
        .apply_tool_definition_binding(tool_definition_binding);
    event.data.obligations = metadata.obligations.clone();
    event.data.obligation_outcomes =
        crate::mcp::obligations::execute_log_only(&metadata.obligations, tool);
    event.data.approval_state = metadata.approval_state.clone();
    if let Some(artifact) = &metadata.approval_artifact {
        event.data.approval_id = Some(artifact.approval_id.clone());
        event.data.approver = Some(artifact.approver.clone());
        event.data.issued_at = Some(artifact.issued_at.clone());
        event.data.expires_at = Some(artifact.expires_at.clone());
        event.data.scope = Some(artifact.scope.clone());
        event.data.approval_bound_tool = Some(artifact.bound_tool.clone());
        event.data.approval_bound_resource = Some(artifact.bound_resource.clone());
    }
    event.data.approval_freshness = metadata.approval_freshness;
    event.data.approval_failure_reason = metadata.approval_failure_reason.clone();
    event.data.scope_type = metadata.scope_type.clone();
    event.data.scope_value = metadata.scope_value.clone();
    event.data.scope_match_mode = metadata.scope_match_mode.clone();
    event.data.scope_evaluation_state = metadata.scope_evaluation_state.clone();
    event.data.scope_failure_reason = metadata.scope_failure_reason.clone();
    event.data.restrict_scope_present = metadata.restrict_scope_present;
    event.data.restrict_scope_target = metadata.restrict_scope_target.clone();
    event.data.restrict_scope_match = metadata.restrict_scope_match;
    event.data.restrict_scope_reason = metadata.restrict_scope_reason.clone();
    event.data.redaction_target = metadata.redaction_target.clone();
    event.data.redaction_mode = metadata.redaction_mode.clone();
    event.data.redaction_scope = metadata.redaction_scope.clone();
    event.data.redaction_applied_state = metadata.redaction_applied_state.clone();
    event.data.redaction_reason = metadata.redaction_reason.clone();
    event.data.redaction_failure_reason = metadata.redaction_failure_reason.clone();
    event.data.redact_args_present = metadata.redact_args_present;
    event.data.redact_args_target = metadata.redact_args_target.clone();
    event.data.redact_args_mode = metadata.redact_args_mode.clone();
    event.data.redact_args_result = metadata.redact_args_result.clone();
    event.data.redact_args_reason = metadata.redact_args_reason.clone();
    event.data.fail_closed = metadata.fail_closed.clone();
    event.data.lane = metadata.lane.clone();
    event.data.principal = metadata.principal.clone();
    event.data.auth_context_summary = metadata.auth_context_summary.clone();
    event.data.auth_scheme = metadata.auth_scheme.clone();
    event.data.auth_issuer = metadata.auth_issuer.clone();
    event.data.delegated_from = metadata.delegated_from.clone();
    event.data.delegation_depth = metadata.delegation_depth;
    refresh_contract_projections(&mut event.data);
    emitter.emit(&event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tool_definition::{
        TOOL_DEFINITION_CANONICALIZATION_JCS_MCP_TOOL_DEFINITION_V1,
        TOOL_DEFINITION_DIGEST_ALG_SHA256, TOOL_DEFINITION_SCHEMA_V1,
        TOOL_DEFINITION_SOURCE_MCP_TOOLS_LIST,
    };
    use std::sync::Mutex as StdMutex;

    struct CapturingEmitter {
        events: StdMutex<Vec<DecisionEvent>>,
    }

    impl CapturingEmitter {
        fn new() -> Self {
            Self {
                events: StdMutex::new(Vec::new()),
            }
        }
    }

    impl DecisionEmitter for CapturingEmitter {
        fn emit(&self, event: &DecisionEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    fn proxy_contract_request(value: serde_json::Value) -> JsonRpcRequest {
        serde_json::from_value(value).expect("test JSON-RPC request should deserialize")
    }

    #[test]
    fn proxy_contract_tool_call_id_prefers_meta() {
        let request = proxy_contract_request(serde_json::json!({
            "jsonrpc": "2.0",
            "id": "request-a",
            "method": "tools/call",
            "params": {
                "name": "read_file",
                "arguments": {
                    "_meta": {
                        "tool_call_id": "tc_explicit_001"
                    }
                }
            }
        }));

        assert_eq!(
            extract_tool_call_id(&request, request.tool_params().as_ref()),
            "tc_explicit_001"
        );
    }

    #[test]
    fn proxy_contract_tool_call_id_uses_request_id() {
        let string_request = proxy_contract_request(serde_json::json!({
            "jsonrpc": "2.0",
            "id": "request-a",
            "method": "tools/call",
            "params": {
                "name": "read_file",
                "arguments": {}
            }
        }));
        let numeric_request = proxy_contract_request(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "tools/call",
            "params": {
                "name": "read_file",
                "arguments": {}
            }
        }));

        assert_eq!(
            extract_tool_call_id(&string_request, string_request.tool_params().as_ref()),
            "req_request-a"
        );
        assert_eq!(
            extract_tool_call_id(&numeric_request, numeric_request.tool_params().as_ref()),
            "req_42"
        );
    }

    #[test]
    fn proxy_contract_unknown_tool_call_id_is_generated() {
        let request = proxy_contract_request(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "read_file",
                "arguments": {}
            }
        }));

        let generated = extract_tool_call_id(&request, request.tool_params().as_ref());
        assert!(
            generated.starts_with("gen_"),
            "missing request id should produce generated idempotency key"
        );
    }

    #[test]
    fn proxy_contract_policy_code_mapping_is_stable() {
        assert_eq!(
            map_policy_code("E_TOOL_DENIED"),
            reason_codes::P_TOOL_DENIED
        );
        assert_eq!(
            map_policy_code("E_TOOL_NOT_ALLOWED"),
            reason_codes::P_TOOL_NOT_ALLOWED
        );
        assert_eq!(map_policy_code("E_ARG_SCHEMA"), reason_codes::P_ARG_SCHEMA);
        assert_eq!(map_policy_code("E_RATE_LIMIT"), reason_codes::P_RATE_LIMIT);
        assert_eq!(map_policy_code("E_TOOL_DRIFT"), reason_codes::P_TOOL_DRIFT);
        assert_eq!(
            map_policy_code("E_UNKNOWN_NEW_CODE"),
            reason_codes::P_POLICY_DENY
        );
    }

    #[test]
    fn proxy_contract_emit_decision_preserves_core_fields() {
        let emitter = Arc::new(CapturingEmitter::new());
        let emitter_trait: Arc<dyn DecisionEmitter> = emitter.clone();
        let metadata = PolicyMatchMetadata {
            tool_classes: vec!["fs.read".to_string()],
            matched_tool_classes: vec!["fs.read".to_string()],
            match_basis: crate::mcp::tool_match::MatchBasis::NameAndClass,
            matched_rule: Some("allow-read-file".to_string()),
            policy_version: Some("2026-05".to_string()),
            policy_digest: Some("sha256:policy".to_string()),
            lane: Some("local-dev".to_string()),
            principal: Some("user:alice".to_string()),
            auth_context_summary: Some("local session".to_string()),
            ..PolicyMatchMetadata::default()
        };

        emit_decision(
            &emitter_trait,
            "assay://test",
            "tc_core_fields",
            "read_file",
            Decision::Deny,
            reason_codes::P_TOOL_DENIED,
            Some("blocked by test policy".to_string()),
            Some(serde_json::json!("request-a")),
            &metadata,
            None,
        );

        let events = emitter.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, "assay://test");
        let data = &events[0].data;
        assert_eq!(data.tool, "read_file");
        assert_eq!(data.tool_call_id, "tc_core_fields");
        assert_eq!(data.decision, Decision::Deny);
        assert_eq!(data.reason_code, reason_codes::P_TOOL_DENIED);
        assert_eq!(data.reason.as_deref(), Some("blocked by test policy"));
        assert_eq!(data.request_id, Some(serde_json::json!("request-a")));
        assert_eq!(data.tool_classes, vec!["fs.read".to_string()]);
        assert_eq!(data.matched_tool_classes, vec!["fs.read".to_string()]);
        assert_eq!(data.match_basis.as_deref(), Some("name+class"));
        assert_eq!(data.matched_rule.as_deref(), Some("allow-read-file"));
        assert_eq!(data.policy_version.as_deref(), Some("2026-05"));
        assert_eq!(data.policy_digest.as_deref(), Some("sha256:policy"));
        assert_eq!(
            data.policy_snapshot_digest.as_deref(),
            Some("sha256:policy")
        );
        assert_eq!(data.lane.as_deref(), Some("local-dev"));
        assert_eq!(data.principal.as_deref(), Some("user:alice"));
        assert_eq!(data.auth_context_summary.as_deref(), Some("local session"));
    }

    #[test]
    fn emit_decision_projects_tool_definition_binding_atomically() {
        let mut tool = serde_json::json!({
            "name": "read_file",
            "description": "Read files",
            "inputSchema": {"type": "object"}
        });
        let observation = super::super::tools::observe_tool_definition(&mut tool, "server-a")
            .expect("supported tool definition should be observed");
        let binding = observation.binding.expect("binding should be visible");
        let emitter = Arc::new(CapturingEmitter::new());
        let emitter_trait: Arc<dyn DecisionEmitter> = emitter.clone();

        emit_decision(
            &emitter_trait,
            "assay://test",
            "tc_tool_definition",
            "read_file",
            Decision::Allow,
            reason_codes::P_POLICY_ALLOW,
            None,
            None,
            &PolicyMatchMetadata::default(),
            Some(&binding),
        );

        let events = emitter.events.lock().unwrap();
        let data = &events[0].data;
        assert!(data.tool_definition_digest.is_some());
        assert_eq!(
            data.tool_definition_digest_alg.as_deref(),
            Some(TOOL_DEFINITION_DIGEST_ALG_SHA256)
        );
        assert_eq!(
            data.tool_definition_canonicalization.as_deref(),
            Some(TOOL_DEFINITION_CANONICALIZATION_JCS_MCP_TOOL_DEFINITION_V1)
        );
        assert_eq!(
            data.tool_definition_schema.as_deref(),
            Some(TOOL_DEFINITION_SCHEMA_V1)
        );
        assert_eq!(
            data.tool_definition_source.as_deref(),
            Some(TOOL_DEFINITION_SOURCE_MCP_TOOLS_LIST)
        );
    }
}
