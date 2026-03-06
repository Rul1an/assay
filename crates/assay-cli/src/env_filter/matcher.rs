/// Match a key against glob patterns (PREFIX_*, *_SUFFIX, *CONTAINS*, EXACT)
pub fn matches_any_pattern(key: &str, patterns: &[&str]) -> bool {
    for pattern in patterns {
        if matches_pattern(key, pattern) {
            return true;
        }
    }
    false
}

pub(super) fn matches_pattern(key: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let has_prefix_wildcard = pattern.starts_with('*');
    let has_suffix_wildcard = pattern.ends_with('*');

    match (has_prefix_wildcard, has_suffix_wildcard) {
        (true, true) => {
            // *CONTAINS*
            let inner = &pattern[1..pattern.len() - 1];
            key.contains(inner)
        }
        (true, false) => {
            // *_SUFFIX
            let suffix = &pattern[1..];
            key.ends_with(suffix)
        }
        (false, true) => {
            // PREFIX_*
            let prefix = &pattern[..pattern.len() - 1];
            key.starts_with(prefix)
        }
        (false, false) => {
            // EXACT
            key == pattern
        }
    }
}
