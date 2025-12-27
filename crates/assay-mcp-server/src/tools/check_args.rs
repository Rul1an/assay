use super::{ToolContext, ToolError};
use anyhow::{Context, Result};
// Keep if needed for trace synthesis? Wait, user logic removed trace synthesis requirements for P2.2? The "Pseudo" code showed logic flow. I'll check what I replaced.
// I replaced Trace/Expectation logic with direct validation.
// So I probably don't need ToolCallRecord/LlmResponse/etc.
// Retaining essential imports only.
// For testing cache behavior deterministically
#[cfg(test)]
pub static COMPILE_CT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

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

    // 2. Load Policy (Read -> Hash -> Cache -> Compile)
    // Secure resolve
    let policy_path = match ctx.resolve_policy_path(policy_rel_path).await {
        Ok(p) => p,
        Err(e) => return e.result(),
    };

    // Slow hook for timeout testing (PR1 legacy, strictly kept for tests)
    #[cfg(debug_assertions)]
    if policy_rel_path.contains("slow") {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Let's rewrite read clean.
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
    // Include tool_name in cache key because we extract sub-schema
    let cache_key_str = format!("{}::{}", policy_path.to_str().unwrap_or(""), tool_name);
    let cache_key = crate::cache::key(&cache_key_str, &sha);

    let schema = if let Some(s) = ctx.caches.args_schema.get(&cache_key) {
        tracing::debug!(event="cache_hit", key=%cache_key, cache="args_schema");
        s
    } else {
        tracing::debug!(event="cache_miss", key=%cache_key, cache="args_schema");
        // Compile
        // 1. Parse YAML to Value
        let full_policy: Value = match serde_yaml::from_slice(&policy_bytes) {
            Ok(v) => v,
            Err(e) => return ToolError::new("E_POLICY_PARSE", &e.to_string()).result(),
        };

        // Extract tool specific schema
        let schema_val = if let Some(s) = full_policy.get(tool_name) {
            s.clone()
        } else {
            // If tool not found in policy, implicit allow? Or Error?
            // If policy is mandatory, then Error.
            return ToolError::new(
                "E_POLICY_MISSING_TOOL",
                &format!("Tool '{}' not defined in policy", tool_name),
            )
            .result();
        };

        // 2. Compile JSON Schema (Blocking)
        #[cfg(test)]
        COMPILE_CT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // We need to move schema_val into thread
        let result = tokio::task::spawn_blocking(move || {
            let static_val = Box::leak(Box::new(schema_val));
            jsonschema::JSONSchema::compile(static_val)
        })
        .await?;

        match result {
            Ok(s) => {
                let arc = std::sync::Arc::new(s);
                ctx.caches.args_schema.insert(cache_key, arc.clone());
                arc
            }
            Err(e) => return ToolError::new("E_SCHEMA_COMPILE", &e.to_string()).result(),
        }
    };

    // 3. Validate
    let result = schema.validate(tool_args);
    if let Err(errors) = result {
        let violations: Vec<Value> = errors
            .map(|e| {
                serde_json::json!({
                    "path": e.instance_path.to_string(),
                    "constraint": e.to_string(), // Simplified message
                    "message": e.to_string()
                })
            })
            .collect();

        Ok(serde_json::json!({
            "allowed": false,
            "violations": violations,
            "suggested_fix": null
        }))
    } else {
        Ok(serde_json::json!({
            "allowed": true,
            "violations": [],
            "suggested_fix": null
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::PolicyCaches;
    use crate::config::ServerConfig;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_cache_hit_behavior() {
        // Use timestamp for unique dir
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("assay_test_{}", unique_id));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
        let policy_file = temp_dir.join("test_policy.yaml");
        tokio::fs::write(&policy_file, "ls:\n  type: object")
            .await
            .unwrap();

        let cfg = ServerConfig::default();
        let caches = PolicyCaches::new(100);
        let policy_root_canon = tokio::fs::canonicalize(&temp_dir).await.unwrap();
        let ctx = ToolContext {
            policy_root: temp_dir.clone(),
            policy_root_canon,
            cfg,
            caches,
        };

        // Reset Counter
        COMPILE_CT.store(0, std::sync::atomic::Ordering::Relaxed);

        // 1. First Call (Miss -> compile)
        let args = serde_json::json!({
            "tool": "ls",
            "arguments": { "path": "." },
            "policy": "test_policy.yaml"
        });

        // Write a valid schema so compile succeeds
        // (Moved above)

        let _ = check_args(&ctx, &args).await;

        let ct_after_1 = COMPILE_CT.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(ct_after_1, 1, "Should have compiled once");

        // 2. Second Call (Hit)
        let _ = check_args(&ctx, &args).await;

        let ct_after_2 = COMPILE_CT.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(ct_after_2, 1, "Should hit cache (count unchanged)");

        // 3. Edit File (Cache Miss simulation)
        tokio::fs::write(&policy_file, "ls:\n  type: string")
            .await
            .unwrap();

        // Call (Miss -> compile)
        let _ = check_args(&ctx, &args).await;
        let ct_after_mod = COMPILE_CT.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(ct_after_mod, 2, "Modified file should trigger compile");

        // Cleanup
        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }
}
