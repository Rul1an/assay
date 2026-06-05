pub(in crate::mcp::policy) fn matches_tool_pattern(tool_name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return tool_name == pattern;
    }
    let starts_star = pattern.starts_with('*');
    let ends_star = pattern.ends_with('*');
    match (starts_star, ends_star) {
        (true, true) => {
            let inner = pattern.trim_matches('*');
            if inner.is_empty() {
                true
            } else {
                tool_name.contains(inner)
            }
        }
        (false, true) => {
            let prefix = pattern.trim_end_matches('*');
            !prefix.is_empty() && tool_name.starts_with(prefix)
        }
        (true, false) => {
            let suffix = pattern.trim_start_matches('*');
            !suffix.is_empty() && tool_name.ends_with(suffix)
        }
        (false, false) => tool_name == pattern,
    }
}
