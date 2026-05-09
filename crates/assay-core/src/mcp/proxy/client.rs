use super::decisions::{emit_decision, extract_tool_call_id, handle_allow, map_policy_code};
use super::ProxyConfig;
use crate::mcp::audit::{AuditEvent, AuditLog};
use crate::mcp::decision::{reason_codes, Decision, DecisionEmitter};
use crate::mcp::identity::ToolIdentity;
use crate::mcp::jsonrpc::JsonRpcRequest;
use crate::mcp::policy::{make_deny_response, McpPolicy, PolicyDecision, PolicyState};
use crate::mcp::tool_definition::ToolDefinitionBinding;
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    process::ChildStdin,
    sync::{Arc, Mutex},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn run_client_to_server(
    mut child_stdin: ChildStdin,
    stdout: Arc<Mutex<io::Stdout>>,
    policy: McpPolicy,
    config: ProxyConfig,
    emitter: Arc<dyn DecisionEmitter>,
    event_source: String,
    identity_cache: Arc<Mutex<HashMap<String, ToolIdentity>>>,
    tool_definition_cache: Arc<Mutex<HashMap<String, ToolDefinitionBinding>>>,
) -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();

    let mut state = PolicyState::default();
    let mut audit_log = AuditLog::new(config.audit_log_path.as_deref());

    while reader.read_line(&mut line)? > 0 {
        // 1. Try Parse as MCP Request
        match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(req) => {
                // 2. Check Policy with Identity (Phase 9)
                let (runtime_id, tool_definition_binding) = if req.is_tool_call() {
                    let name = req.tool_params().map(|p| p.name).unwrap_or_default();
                    let runtime_id = {
                        let cache = identity_cache.lock().unwrap();
                        cache.get(&name).cloned()
                    };
                    let tool_definition_binding = {
                        let cache = tool_definition_cache.lock().unwrap();
                        cache.get(&name).cloned()
                    };
                    (runtime_id, tool_definition_binding)
                } else {
                    (None, None)
                };

                let tool_name = req.tool_params().map(|p| p.name).unwrap_or_default();
                let tool_call_id = extract_tool_call_id(&req);

                let policy_eval = policy.evaluate_with_metadata(
                    &tool_name,
                    &req.tool_params()
                        .map(|p| p.arguments)
                        .unwrap_or(serde_json::Value::Null),
                    &mut state,
                    runtime_id.as_ref(),
                );

                match policy_eval.decision {
                    PolicyDecision::Allow => {
                        handle_allow(&req, &mut audit_log, config.verbose);
                        // Emit decision event (I1: always emit)
                        if req.is_tool_call() {
                            emit_decision(
                                &emitter,
                                &event_source,
                                &tool_call_id,
                                &tool_name,
                                Decision::Allow,
                                reason_codes::P_POLICY_ALLOW,
                                None,
                                req.id.clone(),
                                &policy_eval.metadata,
                                tool_definition_binding.as_ref(),
                            );
                        }
                    }
                    PolicyDecision::AllowWithWarning { tool, code, reason } => {
                        // Log warning about allowing a tool invocation with issues
                        if config.verbose {
                            eprintln!(
                                "[assay] WARNING: Allowing tool '{}' with warning (code: {}, reason: {}).",
                                tool, code, reason
                            );
                        }
                        audit_log.log(&AuditEvent {
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            decision: "allow_with_warning".to_string(),
                            tool: Some(tool.clone()),
                            reason: Some(reason.clone()),
                            request_id: req.id.clone(),
                            agentic: None,
                        });
                        // Emit decision event (I1: always emit)
                        emit_decision(
                            &emitter,
                            &event_source,
                            &tool_call_id,
                            &tool,
                            Decision::Allow,
                            &code,
                            Some(reason),
                            req.id.clone(),
                            &policy_eval.metadata,
                            tool_definition_binding.as_ref(),
                        );
                        // Then proceed as a normal allow
                        handle_allow(&req, &mut audit_log, false);
                        // false = don't double log ALLOW
                    }
                    PolicyDecision::Deny {
                        tool,
                        code,
                        reason,
                        contract,
                    } => {
                        // Log Decision
                        let decision_str = if config.dry_run { "would_deny" } else { "deny" };

                        if config.verbose {
                            eprintln!(
                                "[assay] {} {} (reason: {})",
                                decision_str.to_uppercase(),
                                tool,
                                reason
                            );
                        }

                        audit_log.log(&AuditEvent {
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            decision: decision_str.to_string(),
                            tool: Some(tool.clone()),
                            reason: Some(reason.clone()),
                            request_id: req.id.clone(),
                            agentic: Some(contract.clone()),
                        });

                        // Emit decision event (I1: always emit)
                        let reason_code = map_policy_code(&code);
                        emit_decision(
                            &emitter,
                            &event_source,
                            &tool_call_id,
                            &tool,
                            if config.dry_run {
                                Decision::Allow
                            } else {
                                Decision::Deny
                            },
                            &reason_code,
                            Some(reason),
                            req.id.clone(),
                            &policy_eval.metadata,
                            tool_definition_binding.as_ref(),
                        );

                        if config.dry_run {
                            // DRY RUN: Forward anyway
                            // Fallthrough to forward logic below
                        } else {
                            // BLOCK: Send error response
                            let id = req.id.unwrap_or(serde_json::Value::Null);
                            let response_json =
                                make_deny_response(id, "Content blocked by policy", contract);

                            let mut out =
                                stdout.lock().map_err(|e| io::Error::other(e.to_string()))?;
                            out.write_all(response_json.as_bytes())?;
                            out.flush()?;

                            line.clear();
                            continue; // Skip forwarding
                        }
                    }
                }
            }
            Err(_) => {
                // Hardening: Suspicious Unparsable JSON
                let trimmed = line.trim();
                if trimmed.starts_with('{')
                    && (trimmed.contains("\"method\"")
                        || trimmed.contains("\"params\"")
                        || trimmed.contains("\"tool\""))
                {
                    eprintln!("[assay] WARNING: Suspicious unparsable JSON, forwarding anyway (potential bypass attempt?): {:.60}...", trimmed);
                }
            }
        }

        // 3. Forward
        child_stdin.write_all(line.as_bytes())?;
        child_stdin.flush()?;
        line.clear();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_contract_client_loop_inputs_remain_private() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<Arc<Mutex<HashMap<String, ToolIdentity>>>>();
        assert_send::<Arc<Mutex<HashMap<String, ToolDefinitionBinding>>>>();
        assert_sync::<Arc<dyn DecisionEmitter>>();
    }
}
