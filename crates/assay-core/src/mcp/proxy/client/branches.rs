use super::super::decisions::{emit_decision, handle_allow, map_policy_code};
use super::super::ProxyConfig;
use crate::mcp::audit::{AuditEvent, AuditLog};
use crate::mcp::decision::{reason_codes, Decision, DecisionEmitter};
use crate::mcp::jsonrpc::{CallToolParams, JsonRpcRequest};
use crate::mcp::policy::{make_deny_response, PolicyDecision, PolicyMatchMetadata};
use crate::mcp::tool_definition::ToolDefinitionBinding;
use std::{io, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BranchOutcome {
    Forward,
    Blocked,
}

pub(super) struct PolicyBranchContext<'a> {
    pub(super) req: &'a JsonRpcRequest,
    pub(super) tool_params: Option<&'a CallToolParams>,
    pub(super) tool_name: &'a str,
    pub(super) tool_call_id: &'a str,
    pub(super) metadata: &'a PolicyMatchMetadata,
    pub(super) tool_definition_binding: Option<&'a ToolDefinitionBinding>,
    pub(super) config: &'a ProxyConfig,
    pub(super) emitter: &'a Arc<dyn DecisionEmitter>,
    pub(super) event_source: &'a str,
    pub(super) audit_log: &'a mut AuditLog,
}

pub(super) fn handle_policy_decision<F>(
    decision: PolicyDecision,
    ctx: PolicyBranchContext<'_>,
    write_deny_response: F,
) -> io::Result<BranchOutcome>
where
    F: FnOnce(&str) -> io::Result<()>,
{
    match decision {
        PolicyDecision::Allow => handle_allow_branch(ctx),
        PolicyDecision::AllowWithWarning { tool, code, reason } => {
            handle_allow_with_warning_branch(ctx, tool, code, reason)
        }
        PolicyDecision::Deny {
            tool,
            code,
            reason,
            contract,
        } => handle_deny_branch(ctx, tool, code, reason, contract, write_deny_response),
    }
}

fn handle_allow_branch(ctx: PolicyBranchContext<'_>) -> io::Result<BranchOutcome> {
    handle_allow(ctx.req, ctx.tool_params, ctx.audit_log, ctx.config.verbose);
    if ctx.req.is_tool_call() {
        emit_decision(
            ctx.emitter,
            ctx.event_source,
            ctx.tool_call_id,
            ctx.tool_name,
            Decision::Allow,
            reason_codes::P_POLICY_ALLOW,
            None,
            ctx.req.id.clone(),
            ctx.metadata,
            ctx.tool_definition_binding,
        );
    }
    Ok(BranchOutcome::Forward)
}

fn handle_allow_with_warning_branch(
    ctx: PolicyBranchContext<'_>,
    tool: String,
    code: String,
    reason: String,
) -> io::Result<BranchOutcome> {
    if ctx.config.verbose {
        eprintln!(
            "[assay] WARNING: Allowing tool '{}' with warning (code: {}, reason: {}).",
            tool, code, reason
        );
    }
    ctx.audit_log.log(&AuditEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        decision: "allow_with_warning".to_string(),
        tool: Some(tool.clone()),
        reason: Some(reason.clone()),
        request_id: ctx.req.id.clone(),
        agentic: None,
    });
    emit_decision(
        ctx.emitter,
        ctx.event_source,
        ctx.tool_call_id,
        &tool,
        Decision::Allow,
        &code,
        Some(reason),
        ctx.req.id.clone(),
        ctx.metadata,
        ctx.tool_definition_binding,
    );
    handle_allow(ctx.req, ctx.tool_params, ctx.audit_log, false);
    Ok(BranchOutcome::Forward)
}

fn handle_deny_branch<F>(
    ctx: PolicyBranchContext<'_>,
    tool: String,
    code: String,
    reason: String,
    contract: serde_json::Value,
    write_deny_response: F,
) -> io::Result<BranchOutcome>
where
    F: FnOnce(&str) -> io::Result<()>,
{
    let decision_str = if ctx.config.dry_run {
        "would_deny"
    } else {
        "deny"
    };

    if ctx.config.verbose {
        eprintln!(
            "[assay] {} {} (reason: {})",
            decision_str.to_uppercase(),
            tool,
            reason
        );
    }

    ctx.audit_log.log(&AuditEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        decision: decision_str.to_string(),
        tool: Some(tool.clone()),
        reason: Some(reason.clone()),
        request_id: ctx.req.id.clone(),
        agentic: Some(contract.clone()),
    });

    let reason_code = map_policy_code(&code);
    emit_decision(
        ctx.emitter,
        ctx.event_source,
        ctx.tool_call_id,
        &tool,
        if ctx.config.dry_run {
            Decision::Allow
        } else {
            Decision::Deny
        },
        &reason_code,
        Some(reason),
        ctx.req.id.clone(),
        ctx.metadata,
        ctx.tool_definition_binding,
    );

    if ctx.config.dry_run {
        return Ok(BranchOutcome::Forward);
    }

    let id = ctx.req.id.clone().unwrap_or(serde_json::Value::Null);
    let response_json = make_deny_response(id, "Content blocked by policy", contract);
    write_deny_response(&response_json)?;
    Ok(BranchOutcome::Blocked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::decision::DecisionEvent;
    use serde_json::json;
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

        fn events(&self) -> Vec<DecisionEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl DecisionEmitter for CapturingEmitter {
        fn emit(&self, event: &DecisionEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    fn proxy_contract_request() -> JsonRpcRequest {
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "request-a",
            "method": "tools/call",
            "params": {
                "name": "read_file",
                "arguments": {
                    "_meta": {
                        "tool_call_id": "tc_step7_001"
                    }
                }
            }
        }))
        .expect("test request should deserialize")
    }

    fn proxy_config(dry_run: bool) -> ProxyConfig {
        ProxyConfig {
            dry_run,
            verbose: false,
            audit_log_path: None,
            server_id: "test-server".to_string(),
            decision_log_path: None,
            event_source: Some("assay://test/client-branches".to_string()),
        }
    }

    fn with_branch_context<R>(
        dry_run: bool,
        f: impl FnOnce(PolicyBranchContext<'_>, Arc<CapturingEmitter>, &mut String) -> io::Result<R>,
    ) -> io::Result<R> {
        let request = proxy_contract_request();
        let params = request.tool_params();
        let emitter = Arc::new(CapturingEmitter::new());
        let emitter_dyn: Arc<dyn DecisionEmitter> = emitter.clone();
        let config = proxy_config(dry_run);
        let metadata = PolicyMatchMetadata::default();
        let mut audit_log = AuditLog::new(None);
        let mut deny_response = String::new();
        let context = PolicyBranchContext {
            req: &request,
            tool_params: params.as_ref(),
            tool_name: "read_file",
            tool_call_id: "tc_step7_001",
            metadata: &metadata,
            tool_definition_binding: None,
            config: &config,
            emitter: &emitter_dyn,
            event_source: "assay://test/client-branches",
            audit_log: &mut audit_log,
        };

        f(context, emitter, &mut deny_response)
    }

    #[test]
    fn proxy_contract_client_branch_allow_emits_allow_and_forwards() {
        with_branch_context(false, |context, emitter, deny_response| {
            let outcome = handle_policy_decision(PolicyDecision::Allow, context, |response| {
                deny_response.push_str(response);
                Ok(())
            })?;

            assert_eq!(outcome, BranchOutcome::Forward);
            assert!(deny_response.is_empty());
            let events = emitter.events();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].data.decision, Decision::Allow);
            assert_eq!(events[0].data.tool, "read_file");
            assert_eq!(events[0].data.reason_code, reason_codes::P_POLICY_ALLOW);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn proxy_contract_client_branch_warning_emits_allow_and_forwards() {
        with_branch_context(false, |context, emitter, deny_response| {
            let outcome = handle_policy_decision(
                PolicyDecision::AllowWithWarning {
                    tool: "read_file".to_string(),
                    code: "W_UNCONSTRAINED".to_string(),
                    reason: "missing schema".to_string(),
                },
                context,
                |response| {
                    deny_response.push_str(response);
                    Ok(())
                },
            )?;

            assert_eq!(outcome, BranchOutcome::Forward);
            assert!(deny_response.is_empty());
            let events = emitter.events();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].data.decision, Decision::Allow);
            assert_eq!(events[0].data.reason_code, "W_UNCONSTRAINED");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn proxy_contract_client_branch_deny_emits_deny_and_blocks() {
        with_branch_context(false, |context, emitter, deny_response| {
            let outcome = handle_policy_decision(
                PolicyDecision::Deny {
                    tool: "read_file".to_string(),
                    code: "E_TOOL_DENIED".to_string(),
                    reason: "tool denied".to_string(),
                    contract: json!({"policy": "test"}),
                },
                context,
                |response| {
                    deny_response.push_str(response);
                    Ok(())
                },
            )?;

            assert_eq!(outcome, BranchOutcome::Blocked);
            assert!(deny_response.contains("Content blocked by policy"));
            let events = emitter.events();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].data.decision, Decision::Deny);
            assert_eq!(events[0].data.reason_code, reason_codes::P_TOOL_DENIED);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn proxy_contract_client_branch_dry_run_deny_emits_allow_and_forwards() {
        with_branch_context(true, |context, emitter, deny_response| {
            let outcome = handle_policy_decision(
                PolicyDecision::Deny {
                    tool: "read_file".to_string(),
                    code: "E_TOOL_DENIED".to_string(),
                    reason: "tool denied".to_string(),
                    contract: json!({"policy": "test"}),
                },
                context,
                |response| {
                    deny_response.push_str(response);
                    Ok(())
                },
            )?;

            assert_eq!(outcome, BranchOutcome::Forward);
            assert!(deny_response.is_empty());
            let events = emitter.events();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].data.decision, Decision::Allow);
            assert_eq!(events[0].data.reason_code, reason_codes::P_TOOL_DENIED);
            Ok(())
        })
        .unwrap();
    }
}
