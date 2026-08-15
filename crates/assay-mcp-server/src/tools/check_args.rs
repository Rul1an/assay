use super::{PolicyParseFailure, ToolContext, ToolError};
use anyhow::{Context, Result};
use assay_core::mcp::policy::{McpPolicy, McpPolicyError, McpPolicyErrorKind, PolicyDecision};
use serde_json::Value;

pub async fn check_args(ctx: &ToolContext, args: &Value) -> Result<Value> {
    // 1. Unpack args & Check Limits
    let tool_name = args
        .get("tool")
        .and_then(|v| v.as_str())
        .context("Missing 'tool' argument")?;
    let tool_args = args
        .get("arguments")
        .context("Missing 'arguments' argument")?;
    let policy_rel_path = args
        .get("policy")
        .and_then(|v| v.as_str())
        .context("Missing 'policy' argument")?;

    if tool_name.len() > ctx.cfg.max_field_bytes {
        return ToolError::new("E_LIMIT_EXCEEDED", "tool name too long").result();
    }
    if policy_rel_path.len() > ctx.cfg.max_field_bytes {
        return ToolError::new("E_LIMIT_EXCEEDED", "policy path too long").result();
    }
    // Check args size approximately
    if serde_json::to_vec(tool_args)?.len() > ctx.cfg.max_field_bytes {
        return ToolError::new("E_LIMIT_EXCEEDED", "arguments too large").result();
    }

    // Slow hook for timeout testing (Strict preservation for tests)
    #[cfg(debug_assertions)]
    if policy_rel_path.contains("slow") {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Note: In Phase 3, we reconstruct state per request for simplicity in this stateless tool.
    // For a stateful server, you would pass `&mut state` from the caller.
    // Since 'check_args' is often used as a stateless validation endpoint, local state is acceptable
    // provided rate limits aren't critical for this specific tool endpoint.
    // Ideally, the Server struct would hold persistent PolicyState.
    let mut state = assay_core::mcp::policy::PolicyState::default();

    let policy_bytes = match ctx.read_policy_bounded(policy_rel_path).await {
        Ok(b) => b,
        Err(e) => return e.result(),
    };

    let policy = match McpPolicy::from_slice(&policy_bytes) {
        Ok(p) => p,
        Err(e) => {
            if let Some(typed) = e.downcast_ref::<McpPolicyError>() {
                let (class, location) = match &typed.kind {
                    McpPolicyErrorKind::Syntax { line, column } => {
                        (PolicyParseFailure::YamlSyntax, line.zip(*column))
                    }
                    McpPolicyErrorKind::RootNotMapping => {
                        (PolicyParseFailure::RootNotMapping, None)
                    }
                    McpPolicyErrorKind::Structure | McpPolicyErrorKind::Validation => {
                        (PolicyParseFailure::Structure, None)
                    }
                    _ => (PolicyParseFailure::Structure, None),
                };
                return ToolError::policy_parse(class, location).result();
            }
            return ToolError::policy_parse(PolicyParseFailure::Structure, None).result();
        }
    };

    // 3. Evaluate
    let decision = policy.evaluate(tool_name, tool_args, &mut state, None);

    // 4. Transform Decision to Output
    match decision {
        PolicyDecision::Allow => Ok(serde_json::json!({
            "allowed": true,
            "violations": [],
            "suggested_fix": null
        })),
        PolicyDecision::AllowWithWarning { code, reason, .. } => Ok(serde_json::json!({
            "allowed": true,
            "warning": { "code": code, "reason": reason },
            "violations": [],
            "suggested_fix": null
        })),
        PolicyDecision::Deny {
            code,
            reason,
            contract,
            ..
        } => {
            // Map unified contract back to expected format if needed, or pass through.
            // Current CLI expects "violations" in a specific way for schema errors.
            // If contract contains violations, use them.
            let violations = contract
                .get("violations")
                .cloned()
                .unwrap_or(serde_json::json!([]));
            Ok(serde_json::json!({
                "allowed": false,
                "code": code,
                "reason": reason,
                "violations": violations,
                "suggested_fix": null,
                "contract": contract
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::PolicyCaches;
    use crate::config::ServerConfig;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_check_args_unified_enforcement() {
        let tmp = TempDir::new().unwrap();
        let policy_root = tokio::fs::canonicalize(tmp.path()).await.unwrap();
        let policy_path = policy_root.join("unified.yaml");

        // V2 Policy with Schema
        let yaml = r#"
version: "2.0"
name: "unified-test"
tools:
  allow: ["read_file"]
schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/tmp/.*"
    required: ["path"]
"#;
        tokio::fs::write(&policy_path, yaml)
            .await
            .expect("write failed");

        let cfg = ServerConfig::default();
        let caches = PolicyCaches::new(100); // Unused but required by struct
        let ctx = ToolContext {
            policy_root: policy_root.clone(),
            policy_root_canon: policy_root.clone(),
            cfg,
            caches,
        };

        // Case 1: Schema Violation
        let args = serde_json::json!({
            "tool": "read_file",
            "policy": "unified.yaml",
            "arguments": { "path": "/etc/passwd" }
        });

        let res = check_args(&ctx, &args).await.unwrap();
        eprintln!("DEBUG: res = {}", res);
        assert_eq!(res["allowed"], false);

        let code = if let Some(c) = res.get("code").and_then(|v| v.as_str()) {
            c.to_string()
        } else if let Some(e) = res
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
        {
            e.to_string()
        } else {
            panic!("No code found in response: {}", res);
        };

        assert!(
            code == "E_ARG_SCHEMA" || code == "MCP_ARG_CONSTRAINT" || code == "E_POLICY_NOT_FOUND"
        );

        // Case 2: Allowed
        let args_ok = serde_json::json!({
            "tool": "read_file",
            "policy": "unified.yaml",
            "arguments": { "path": "/tmp/safe.txt" }
        });
        let res_ok = check_args(&ctx, &args_ok).await.unwrap();
        assert_eq!(res_ok["allowed"], true);
    }
}
