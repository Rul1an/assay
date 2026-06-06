pub(super) enum PathClass {
    /// Exact path (no wildcards)
    Exact,
    /// Prefix match (ends with /*)
    Prefix(String),
    /// Complex glob (**, ?, etc.)
    Glob,
}

pub(super) fn classify_path_pattern(pattern: &str) -> PathClass {
    // Check for glob characters.
    let has_double_star = pattern.contains("**");
    let has_single_star = pattern.contains('*');
    let has_question = pattern.contains('?');
    let has_bracket = pattern.contains('[');

    if !has_single_star && !has_question && !has_bracket {
        // No wildcards - exact match.
        return PathClass::Exact;
    }

    if has_double_star || has_question || has_bracket {
        // Complex pattern - needs userspace.
        return PathClass::Glob;
    }

    // Single star - might be a simple prefix.
    if pattern.ends_with("/*") && pattern.matches('*').count() == 1 {
        let prefix = &pattern[..pattern.len() - 1];
        return PathClass::Prefix(prefix.to_string());
    }

    // Pattern like "/etc/*.conf" - needs glob matching.
    PathClass::Glob
}

/// FNV-1a hash (same as kernel)
pub(super) fn fnv1a_hash(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0100_0000_01b3;

    let mut hash = FNV_OFFSET;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
