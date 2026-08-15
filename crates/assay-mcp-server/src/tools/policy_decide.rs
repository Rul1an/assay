use super::{yaml_mapping_stage, PolicyParseFailure, ToolContext, ToolError};
use anyhow::{Context, Result};
use serde_json::Value;

#[derive(serde::Deserialize)]
struct PolicyDecisionDocument {
    #[serde(default)]
    blocklist: Vec<String>,
}

fn parse_policy_decision_document(bytes: &[u8]) -> Result<Vec<String>, ToolError> {
    let super::MappingStage(mapping) = yaml_mapping_stage(bytes)?;

    // policy_decide's private dialect check: reject allow/deny/tools keys.
    for key in &["allow", "deny", "tools"] {
        if mapping.contains_key(serde_yaml::Value::String(key.to_string())) {
            return Err(ToolError::policy_parse(PolicyParseFailure::Structure, None));
        }
    }

    // Preserve the existing JSON-compatible private dialect after the shared
    // YAML syntax/root stage. This value projection is not a string reparse.
    let root = serde_json::to_value(serde_yaml::Value::Mapping(mapping))
        .map_err(|_| ToolError::policy_parse(PolicyParseFailure::Structure, None))?;
    serde_json::from_value::<PolicyDecisionDocument>(root)
        .map(|document| document.blocklist)
        .map_err(|_| ToolError::policy_parse(PolicyParseFailure::Structure, None))
}

pub async fn policy_decide(ctx: &ToolContext, args: &Value) -> Result<Value> {
    // 1. Unpack args & Checks
    let tool_name = args
        .get("tool")
        .and_then(|v| v.as_str())
        .context("Missing 'tool' argument")?;
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

    // 2. Load Policy
    let policy_path = match ctx.resolve_policy_path(policy_rel_path).await {
        Ok(p) => p,
        Err(e) => return e.result(),
    };

    // Read logic
    let policy_bytes = match tokio::fs::read(&policy_path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ToolError::new(
                "E_POLICY_NOT_FOUND",
                &format!("Policy not found: {}", policy_rel_path),
            )
            .result();
        }
        Err(e) => return ToolError::new("E_POLICY_READ", &e.to_string()).result(),
    };

    let sha = crate::cache::sha256_hex(&policy_bytes);
    let cache_key = crate::cache::key(policy_path.to_str().unwrap_or(""), &sha);

    let blocked_tools = if let Some(list) = ctx.caches.blocklist.get(&cache_key) {
        tracing::debug!(event="cache_hit", key=%cache_key, cache="blocklist");
        list
    } else {
        tracing::debug!(event="cache_miss", key=%cache_key, cache="blocklist");
        let list = match parse_policy_decision_document(&policy_bytes) {
            Ok(list) => list,
            Err(error) => return error.result(),
        };

        let arc = std::sync::Arc::new(list);
        ctx.caches.blocklist.insert(cache_key, arc.clone());
        arc
    };

    // 3. Evaluate
    if blocked_tools.contains(&tool_name.to_string()) {
        Ok(serde_json::json!({
            "allowed": false,
            "matches": [format!("Tool '{}' is blocked by policy", tool_name)]
        }))
    } else {
        Ok(serde_json::json!({
            "allowed": true,
            "reason": "Allowed by policy"
        }))
    }
}
