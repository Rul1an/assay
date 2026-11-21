use super::{policy_mapping, PolicyParseFailure, ToolContext, ToolError};
use anyhow::{Context, Result};
use serde_json::Value;

assay_common::closed! {
    /// The blocklist dialect. Version and name are labels with arbitrary JSON values.
    root struct PolicyDecisionDocument {
        #[default]
        blocklist: Vec<String>,
        version: Option<assay_common::doc::Opaque<serde::de::IgnoredAny>>,
        name: Option<assay_common::doc::Opaque<serde::de::IgnoredAny>>,
    }
}

fn parse_policy_decision_document(bytes: &[u8]) -> Result<Vec<String>, ToolError> {
    let input = policy_mapping(bytes)?;
    let PolicyDecisionDocument {
        blocklist,
        version: _,
        name: _,
    } = input
        .read_json_projection::<PolicyDecisionDocument>()
        .map_err(|_| ToolError::policy_parse(PolicyParseFailure::Structure, None))?;
    Ok(blocklist)
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
    let policy_bytes = match ctx.read_policy_bounded(policy_rel_path).await {
        Ok(b) => b,
        Err(e) => return e.result(),
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
