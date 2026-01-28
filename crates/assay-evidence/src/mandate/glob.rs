//! Mandate Scope Glob Matching (SPEC-Mandate-v1 §3.2.3)
//!
//! Normative pattern matching for tool names and resources.
//!
//! # Matching Rules (NORMATIVE)
//!
//! | Rule | Specification |
//! |------|---------------|
//! | **Anchoring** | Pattern MUST match the **full tool name** (not substring) |
//! | **Case sensitivity** | Matching is **case-sensitive** |
//! | **`*` (single glob)** | Matches any sequence of characters **except** `.` (dot) |
//! | **`**` (double glob)** | Matches any sequence of characters **including** `.` (dot) |
//! | **Literal characters** | All non-glob characters match themselves exactly |
//! | **Escaping** | Use `\*` to match literal `*`; use `\\` to match literal `\` |
//!
//! # Security Limits
//!
//! To prevent ReDoS attacks:
//! - Max tool name length: 256 characters
//! - Max pattern length: 256 characters
//! - Max segments per pattern: 32
//!
//! # Examples
//!
//! ```text
//! search_*      → matches: search_products, search_users
//!               → does NOT match: search.products (dot not matched by *)
//! fs.read_*     → matches: fs.read_file, fs.read_dir
//!               → does NOT match: fs.read.file (second dot)
//! fs.**         → matches: fs.read_file, fs.write.nested.path
//! *             → matches: search, list (single-segment names only)
//! **            → matches: any tool name (universal wildcard)
//! ```

use std::fmt;

// Security limits to prevent ReDoS
const MAX_TOOL_NAME_LENGTH: usize = 256;
const MAX_PATTERN_LENGTH: usize = 256;
const MAX_SEGMENTS: usize = 32;

/// Error returned when a glob pattern is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobError {
    pub pattern: String,
    pub message: String,
}

impl fmt::Display for GlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid glob pattern '{}': {}",
            self.pattern, self.message
        )
    }
}

impl std::error::Error for GlobError {}

/// Compiled glob pattern for efficient matching.
#[derive(Debug, Clone)]
pub struct GlobPattern {
    pattern: String,
    segments: Vec<Segment>,
}

#[derive(Debug, Clone)]
enum Segment {
    /// Literal string to match exactly
    Literal(String),
    /// `*` - matches any chars except dot
    SingleGlob,
    /// `**` - matches any chars including dot
    DoubleGlob,
}

impl GlobPattern {
    /// Compile a glob pattern.
    ///
    /// # Errors
    ///
    /// Returns error for invalid patterns (e.g., unclosed escape, too long).
    pub fn new(pattern: &str) -> Result<Self, GlobError> {
        // Security: bound pattern length
        if pattern.len() > MAX_PATTERN_LENGTH {
            return Err(GlobError {
                pattern: pattern.chars().take(50).collect::<String>() + "...",
                message: format!(
                    "pattern length {} exceeds maximum {}",
                    pattern.len(),
                    MAX_PATTERN_LENGTH
                ),
            });
        }

        let segments = parse_pattern(pattern)?;

        // Security: bound segment count
        if segments.len() > MAX_SEGMENTS {
            return Err(GlobError {
                pattern: pattern.to_string(),
                message: format!(
                    "pattern has {} segments, exceeds maximum {}",
                    segments.len(),
                    MAX_SEGMENTS
                ),
            });
        }

        Ok(Self {
            pattern: pattern.to_string(),
            segments,
        })
    }

    /// Check if the pattern matches the given name.
    ///
    /// Matching is:
    /// - Case-sensitive
    /// - Anchored (must match full name)
    ///
    /// Returns `false` for names exceeding the security limit (256 chars).
    pub fn matches(&self, name: &str) -> bool {
        // Security: bound input length to prevent ReDoS
        if name.len() > MAX_TOOL_NAME_LENGTH {
            return false;
        }
        match_segments(&self.segments, name)
    }

    /// Get the original pattern string.
    pub fn as_str(&self) -> &str {
        &self.pattern
    }
}

fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, GlobError> {
    let mut segments = Vec::new();
    let mut current_literal = String::new();
    let mut chars = pattern.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Escape next character
                match chars.next() {
                    Some(escaped) => current_literal.push(escaped),
                    None => {
                        return Err(GlobError {
                            pattern: pattern.to_string(),
                            message: "trailing backslash".to_string(),
                        })
                    }
                }
            }
            '*' => {
                // Flush current literal
                if !current_literal.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut current_literal)));
                }

                // Check for ** (double glob)
                if chars.peek() == Some(&'*') {
                    chars.next(); // consume second *
                    segments.push(Segment::DoubleGlob);
                } else {
                    segments.push(Segment::SingleGlob);
                }
            }
            _ => {
                current_literal.push(c);
            }
        }
    }

    // Flush remaining literal
    if !current_literal.is_empty() {
        segments.push(Segment::Literal(current_literal));
    }

    Ok(segments)
}

fn match_segments(segments: &[Segment], input: &str) -> bool {
    match_recursive(segments, input)
}

fn match_recursive(segments: &[Segment], input: &str) -> bool {
    if segments.is_empty() {
        return input.is_empty();
    }

    match &segments[0] {
        Segment::Literal(lit) => {
            if input.starts_with(lit) {
                match_recursive(&segments[1..], &input[lit.len()..])
            } else {
                false
            }
        }
        Segment::SingleGlob => {
            // * matches any sequence except dot
            // Try matching 0 to N chars (stopping at dot or end)
            for i in 0..=input.len() {
                let (prefix, suffix) = input.split_at(i);

                // Check if we hit a dot in the prefix
                if prefix.contains('.') {
                    break;
                }

                if match_recursive(&segments[1..], suffix) {
                    return true;
                }
            }
            false
        }
        Segment::DoubleGlob => {
            // ** matches any sequence including dots
            for i in 0..=input.len() {
                let suffix = &input[i..];
                if match_recursive(&segments[1..], suffix) {
                    return true;
                }
            }
            false
        }
    }
}

/// Check if a tool name matches any of the given patterns.
///
/// # Example
///
/// ```
/// use assay_evidence::mandate::glob::matches_any;
///
/// let patterns = &["search_*", "list_*"];
/// assert!(matches_any("search_products", patterns).unwrap());
/// assert!(matches_any("list_items", patterns).unwrap());
/// assert!(!matches_any("delete_item", patterns).unwrap());
/// ```
pub fn matches_any(name: &str, patterns: &[impl AsRef<str>]) -> Result<bool, GlobError> {
    for pattern in patterns {
        let glob = GlobPattern::new(pattern.as_ref())?;
        if glob.matches(name) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Pre-compiled pattern set for efficient batch matching.
#[derive(Debug, Clone)]
pub struct GlobSet {
    patterns: Vec<GlobPattern>,
}

impl GlobSet {
    /// Compile a set of patterns.
    pub fn new(patterns: &[impl AsRef<str>]) -> Result<Self, GlobError> {
        let compiled: Result<Vec<_>, _> = patterns
            .iter()
            .map(|p| GlobPattern::new(p.as_ref()))
            .collect();
        Ok(Self {
            patterns: compiled?,
        })
    }

    /// Check if any pattern matches the name.
    pub fn matches(&self, name: &str) -> bool {
        self.patterns.iter().any(|p| p.matches(name))
    }

    /// Check if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_match() {
        let glob = GlobPattern::new("exact_match").unwrap();
        assert!(glob.matches("exact_match"));
        assert!(!glob.matches("exact_match_extra"));
        assert!(!glob.matches("prefix_exact_match"));
        assert!(!glob.matches("EXACT_MATCH")); // case-sensitive
    }

    #[test]
    fn test_single_glob_basic() {
        let glob = GlobPattern::new("search_*").unwrap();
        assert!(glob.matches("search_products"));
        assert!(glob.matches("search_users"));
        assert!(glob.matches("search_")); // empty match
        assert!(!glob.matches("search")); // no underscore
        assert!(glob.matches("search_foo_bar")); // matches (no dot in suffix)
    }

    #[test]
    fn test_single_glob_stops_at_dot() {
        let glob = GlobPattern::new("search_*").unwrap();
        assert!(!glob.matches("search_.dotted")); // dot stops *
        assert!(!glob.matches("search_products.json"));

        let glob = GlobPattern::new("fs.read_*").unwrap();
        assert!(glob.matches("fs.read_file"));
        assert!(glob.matches("fs.read_dir"));
        assert!(!glob.matches("fs.read.file")); // second dot
        assert!(!glob.matches("fs.read_nested.path"));
    }

    #[test]
    fn test_double_glob_matches_dots() {
        let glob = GlobPattern::new("fs.**").unwrap();
        assert!(glob.matches("fs.read_file"));
        assert!(glob.matches("fs.write.nested.path"));
        assert!(glob.matches("fs.")); // empty after dot
        assert!(!glob.matches("fs")); // no dot

        let glob = GlobPattern::new("**").unwrap();
        assert!(glob.matches("")); // empty string
        assert!(glob.matches("anything"));
        assert!(glob.matches("any.thing.at.all"));
    }

    #[test]
    fn test_wildcard_only() {
        let glob = GlobPattern::new("*").unwrap();
        assert!(glob.matches("search"));
        assert!(glob.matches("list"));
        assert!(glob.matches("")); // empty
        assert!(!glob.matches("namespaced.tool")); // dot not matched
    }

    #[test]
    fn test_escape_asterisk() {
        let glob = GlobPattern::new(r"file\*name").unwrap();
        assert!(glob.matches("file*name"));
        assert!(!glob.matches("filename"));
        assert!(!glob.matches("file_name"));
    }

    #[test]
    fn test_escape_backslash() {
        let glob = GlobPattern::new(r"path\\to").unwrap();
        assert!(glob.matches(r"path\to"));
        assert!(!glob.matches("pathto"));
    }

    #[test]
    fn test_trailing_backslash_error() {
        let result = GlobPattern::new(r"test\");
        assert!(result.is_err());
    }

    #[test]
    fn test_complex_patterns() {
        // Multiple globs
        let glob = GlobPattern::new("*_*").unwrap();
        assert!(glob.matches("search_products"));
        assert!(glob.matches("a_b"));
        assert!(!glob.matches("search")); // no underscore

        // Glob in middle
        let glob = GlobPattern::new("get_*_by_id").unwrap();
        assert!(glob.matches("get_user_by_id"));
        assert!(glob.matches("get_product_by_id"));
        assert!(!glob.matches("get_user_by_name"));
    }

    #[test]
    fn test_matches_any() {
        let patterns = &["search_*", "list_*", "get_**"];

        assert!(matches_any("search_products", patterns).unwrap());
        assert!(matches_any("list_items", patterns).unwrap());
        assert!(matches_any("get_user.by_id", patterns).unwrap()); // ** matches dot
        assert!(!matches_any("delete_item", patterns).unwrap());
    }

    #[test]
    fn test_glob_set() {
        let set = GlobSet::new(&["purchase_*", "transfer_*", "order_*"]).unwrap();

        assert!(set.matches("purchase_item"));
        assert!(set.matches("transfer_funds"));
        assert!(set.matches("order_product"));
        assert!(!set.matches("search_products"));
    }

    #[test]
    fn test_case_sensitive() {
        let glob = GlobPattern::new("Search_*").unwrap();
        assert!(glob.matches("Search_Products"));
        assert!(!glob.matches("search_products")); // case mismatch
        assert!(!glob.matches("SEARCH_PRODUCTS"));
    }

    #[test]
    fn test_spec_examples() {
        // From SPEC-Mandate-v1 §3.2.3

        // search_* matches search_products, search_users
        let glob = GlobPattern::new("search_*").unwrap();
        assert!(glob.matches("search_products"));
        assert!(glob.matches("search_users"));

        // search_* does NOT match search.products
        assert!(!glob.matches("search.products"));

        // fs.read_* matches fs.read_file, fs.read_dir
        let glob = GlobPattern::new("fs.read_*").unwrap();
        assert!(glob.matches("fs.read_file"));
        assert!(glob.matches("fs.read_dir"));

        // fs.read_* does NOT match fs.read.file
        assert!(!glob.matches("fs.read.file"));

        // fs.** matches fs.read_file, fs.write.nested.path
        let glob = GlobPattern::new("fs.**").unwrap();
        assert!(glob.matches("fs.read_file"));
        assert!(glob.matches("fs.write.nested.path"));

        // * matches single-segment names only
        let glob = GlobPattern::new("*").unwrap();
        assert!(glob.matches("search"));
        assert!(glob.matches("list"));
        assert!(!glob.matches("namespaced.tool"));

        // ** matches any tool name
        let glob = GlobPattern::new("**").unwrap();
        assert!(glob.matches("anything"));
        assert!(glob.matches("any.thing.at.all"));
    }

    // === Anchoring tests (full string match, not substring) ===

    #[test]
    fn test_anchoring_no_prefix_match() {
        // read_* must NOT match "xread_file" (pattern is anchored at start)
        let glob = GlobPattern::new("read_*").unwrap();
        assert!(glob.matches("read_file"));
        assert!(glob.matches("read_dir"));
        assert!(
            !glob.matches("xread_file"),
            "Pattern must be anchored at start"
        );
        assert!(
            !glob.matches("prefix_read_file"),
            "Pattern must be anchored at start"
        );
    }

    #[test]
    fn test_anchoring_no_suffix_match() {
        // *_file must NOT match "read_file_extra" (pattern is anchored at end)
        let glob = GlobPattern::new("*_file").unwrap();
        assert!(glob.matches("read_file"));
        assert!(glob.matches("write_file"));
        assert!(
            !glob.matches("read_file_extra"),
            "Pattern must be anchored at end"
        );
        assert!(
            !glob.matches("read_file.bak"),
            "Pattern must be anchored at end"
        );
    }

    #[test]
    fn test_anchoring_exact_match_required() {
        // Pattern must match the FULL tool name, not a substring
        let glob = GlobPattern::new("search").unwrap();
        assert!(glob.matches("search"));
        assert!(
            !glob.matches("search_products"),
            "Exact pattern requires exact match"
        );
        assert!(
            !glob.matches("my_search"),
            "Exact pattern requires exact match"
        );
        assert!(
            !glob.matches("searching"),
            "Exact pattern requires exact match"
        );
    }

    // === Literal escaping tests ===

    #[test]
    fn test_literal_double_star() {
        // fs.\*\* should match literal "fs.**" not glob
        let glob = GlobPattern::new(r"fs.\*\*").unwrap();
        assert!(glob.matches("fs.**"), "Escaped ** should match literal");
        assert!(!glob.matches("fs.read"), "Escaped ** should not glob");
        assert!(
            !glob.matches("fs.anything.here"),
            "Escaped ** should not glob"
        );
    }

    #[test]
    fn test_literal_backslash_star() {
        // fs.\\* should match "fs.\x" where x is any char (backslash is literal)
        let glob = GlobPattern::new(r"fs.\\*").unwrap();
        assert!(
            glob.matches(r"fs.\file"),
            "Should match backslash + wildcard"
        );
        assert!(
            glob.matches(r"fs.\dir"),
            "Should match backslash + wildcard"
        );
        assert!(!glob.matches("fs.file"), "Backslash is literal, not escape");
    }
}
