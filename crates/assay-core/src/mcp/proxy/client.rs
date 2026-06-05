mod branches;

use self::branches::{handle_policy_decision, BranchOutcome, PolicyBranchContext};
use super::decisions::extract_tool_call_id;
use super::ProxyConfig;
use crate::mcp::audit::AuditLog;
use crate::mcp::decision::DecisionEmitter;
use crate::mcp::identity::ToolIdentity;
use crate::mcp::jsonrpc::JsonRpcRequest;
use crate::mcp::policy::{McpPolicy, PolicyState};
use crate::mcp::tool_definition::ToolDefinitionBinding;
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    process::ChildStdin,
    sync::{Arc, Mutex},
};

#[expect(clippy::too_many_arguments)]
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
                let tool_params = req.tool_params();
                let tool_name = tool_params
                    .as_ref()
                    .map(|params| params.name.as_str())
                    .unwrap_or_default();

                // 2. Check Policy with Identity (Phase 9)
                let (runtime_id, tool_definition_binding) = if req.is_tool_call() {
                    let runtime_id = {
                        let cache = identity_cache.lock().unwrap();
                        cache.get(tool_name).cloned()
                    };
                    let tool_definition_binding = {
                        let cache = tool_definition_cache.lock().unwrap();
                        cache.get(tool_name).cloned()
                    };
                    (runtime_id, tool_definition_binding)
                } else {
                    (None, None)
                };

                let tool_call_id = extract_tool_call_id(&req, tool_params.as_ref());
                let null_arguments = serde_json::Value::Null;
                let tool_arguments = tool_params
                    .as_ref()
                    .map(|params| &params.arguments)
                    .unwrap_or(&null_arguments);

                let policy_eval = policy.evaluate_with_metadata(
                    tool_name,
                    tool_arguments,
                    &mut state,
                    runtime_id.as_ref(),
                );

                let branch_outcome = handle_policy_decision(
                    policy_eval.decision,
                    PolicyBranchContext {
                        req: &req,
                        tool_params: tool_params.as_ref(),
                        tool_name,
                        tool_call_id: &tool_call_id,
                        metadata: &policy_eval.metadata,
                        tool_definition_binding: tool_definition_binding.as_ref(),
                        config: &config,
                        emitter: &emitter,
                        event_source: &event_source,
                        audit_log: &mut audit_log,
                    },
                    |response_json| {
                        let mut out = stdout.lock().map_err(|e| io::Error::other(e.to_string()))?;
                        out.write_all(response_json.as_bytes())?;
                        out.flush()
                    },
                )?;

                if branch_outcome == BranchOutcome::Blocked {
                    line.clear();
                    continue;
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
