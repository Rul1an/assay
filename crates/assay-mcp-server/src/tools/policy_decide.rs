use super::{ToolContext, ToolError};
use anyhow::{Context, Result};
use serde_json::Value;

#[derive(serde::Deserialize)]
struct PolicyDecisionDocument {
    #[serde(default)]
    blocklist: Vec<String>,
}

fn parse_policy_decision_document(bytes: &[u8]) -> Result<Vec<String>, ToolError> {
    let root: Value = serde_yaml::from_slice(bytes)
        .map_err(|_| ToolError::new("E_POLICY_PARSE", "Policy YAML is invalid"))?;
    let object = root
        .as_object()
        .ok_or_else(|| ToolError::new("E_POLICY_PARSE", "Policy root must be a mapping"))?;

    if ["allow", "deny", "tools"]
        .iter()
        .any(|key| object.contains_key(*key))
    {
        return Err(ToolError::new(
            "E_POLICY_PARSE",
            "Policy structure is invalid",
        ));
    }

    serde_json::from_value::<PolicyDecisionDocument>(root)
        .map(|document| document.blocklist)
        .map_err(|_| ToolError::new("E_POLICY_PARSE", "Policy structure is invalid"))
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
