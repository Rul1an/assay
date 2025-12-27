use super::{ToolContext, ToolError};
use anyhow::{Context, Result};
use serde_json::Value;

pub async fn check_sequence(ctx: &ToolContext, args: &Value) -> Result<Value> {
    // 1. Unpack args & Check Limits
    let history_val = args.get("history").context("Missing 'history' argument")?;
    let history: Vec<String> =
        serde_json::from_value(history_val.clone()).context("Invalid 'history' format")?;

    let next_tool = args
        .get("next_tool")
        .and_then(|v| v.as_str())
        .context("Missing 'next_tool' argument")?;
    let policy_rel_path = args
        .get("policy")
        .and_then(|v| v.as_str())
        .context("Missing 'policy' argument")?;

    if history.len() > ctx.cfg.max_tool_calls {
        return ToolError::new("E_LIMIT_EXCEEDED", "history too long").result();
    }
    if next_tool.len() > ctx.cfg.max_field_bytes {
        return ToolError::new("E_LIMIT_EXCEEDED", "next_tool too long").result();
    }
    if policy_rel_path.len() > ctx.cfg.max_field_bytes {
        return ToolError::new("E_LIMIT_EXCEEDED", "policy path too long").result();
    }

    // 2. Load Policy
    let policy_path = match ctx.resolve_policy_path(policy_rel_path).await {
        Ok(p) => p,
        Err(e) => return e.result(),
    };

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

    let policy = if let Some(p) = ctx.caches.sequence.get(&cache_key) {
        tracing::debug!(event="cache_hit", key=%cache_key, cache="sequence");
        p
    } else {
        tracing::debug!(event="cache_miss", key=%cache_key, cache="sequence");
        // Compile (Parse)
        // Try parsing as list of strings (legacy)
        let policy_item = if let Ok(seq) = serde_yaml::from_slice::<Vec<String>>(&policy_bytes) {
            crate::cache::SequencePolicy::Legacy(seq)
        } else if let Ok(rules) =
            serde_yaml::from_slice::<Vec<assay_core::model::SequenceRule>>(&policy_bytes)
        {
            crate::cache::SequencePolicy::Rules(rules)
        } else {
            return ToolError::new("E_POLICY_PARSE", "Invalid sequence policy format").result();
        };

        let arc = std::sync::Arc::new(policy_item);
        ctx.caches.sequence.insert(cache_key, arc.clone());
        arc
    };

    // 3. Synthesize Trace for Validation
    // history + next_tool
    let mut actual_names = history.clone();
    actual_names.push(next_tool.to_string());

    // 4. Validate
    match &*policy {
        crate::cache::SequencePolicy::Legacy(expected_seq) => {
            if actual_names == *expected_seq {
                Ok(serde_json::json!({ "allowed": true, "violations": [], "suggested_fix": null }))
            } else {
                Ok(serde_json::json!({
                    "allowed": false,
                    "violations": [{
                        "constraint": "sequence_exact_match",
                        "suggestion": format!("Expected {:?}, found {:?}", expected_seq, actual_names)
                    }],
                    "suggested_fix": null
                }))
            }
        }
        crate::cache::SequencePolicy::Rules(rules) => {
            let mut violations = Vec::new();

            for rule in rules {
                match rule {
                    assay_core::model::SequenceRule::Require { tool } => {
                        if !actual_names.contains(tool) {
                            violations.push(serde_json::json!({
                                "constraint": "sequence_rule",
                                "message": format!("required tool '{}' not found", tool)
                            }));
                        }
                    }
                    assay_core::model::SequenceRule::Before { first, then } => {
                        let first_idx = actual_names.iter().position(|n| n == first);
                        let then_idx = actual_names.iter().position(|n| n == then);

                        // Strict dependency: if 'then' exists, 'first' must precede it.
                        if let Some(t_idx) = then_idx {
                            if let Some(f_idx) = first_idx {
                                if f_idx > t_idx {
                                    violations.push(serde_json::json!({
                                        "constraint": "sequence_rule",
                                        "message": format!("tool '{}' appeared at {} but required before '{}' at {}", first, f_idx, then, t_idx)
                                    }));
                                }
                            } else {
                                violations.push(serde_json::json!({
                                    "constraint": "sequence_rule",
                                    "message": format!("tool '{}' required before '{}' but missing", first, then)
                                }));
                            }
                        }
                    }
                    assay_core::model::SequenceRule::Blocklist { pattern } => {
                        for name in &actual_names {
                            if name.contains(pattern) {
                                violations.push(serde_json::json!({
                                    "constraint": "sequence_rule",
                                    "message": format!("tool '{}' matches blocklist pattern '{}'", name, pattern)
                                }));
                            }
                        }
                    }
                }
            }

            if violations.is_empty() {
                Ok(serde_json::json!({ "allowed": true, "violations": [], "suggested_fix": null }))
            } else {
                Ok(
                    serde_json::json!({ "allowed": false, "violations": violations, "suggested_fix": null }),
                )
            }
        }
    }
}
