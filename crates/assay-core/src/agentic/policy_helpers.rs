use crate::mcp::policy::McpPolicy;
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) fn get_policy<'a>(
    cache: &'a mut BTreeMap<String, Option<McpPolicy>>,
    path: &str,
) -> Option<&'a McpPolicy> {
    cache
        .entry(path.to_string())
        .or_insert_with(|| {
            let path = Path::new(path);
            let source = std::fs::read_to_string(path).ok()?;
            let policy = McpPolicy::from_slice(source.as_bytes()).ok()?;
            crate::fix::preserves_source_values(path, &source).then_some(policy)
        })
        .as_ref()
}

pub(crate) fn best_candidate(ctx: &serde_json::Value) -> Option<String> {
    // Prefer candidates[0] if present; else none.
    ctx.get("candidates")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub(crate) fn escape_pointer(s: &str) -> String {
    crate::json_pointer::escape_segment(s)
}
