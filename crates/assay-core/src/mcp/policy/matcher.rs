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

#[cfg(test)]
mod tests {
    use super::matches_tool_pattern;
    use crate::runtime::authorizer::authorizer_internal::policy::glob_matches_impl;

    /// The five behaviours `docs/reference/config/policies.md` promises, pinned so the doc and the
    /// matcher cannot drift apart silently. The reference has been accurate since it was written;
    /// nothing was checking that it stayed so.
    #[test]
    fn the_documented_wildcard_forms_behave_as_documented() {
        assert!(
            matches_tool_pattern("anything_at_all", "*"),
            "`*` matches all tools"
        );
        assert!(matches_tool_pattern("read_file", "read_*"), "prefix");
        // Anchored, and the negative case has to contain the prefix somewhere other than the
        // start or it proves nothing: `write_file` does not contain `read_` at all, so asserting
        // on it holds even if prefix matching degraded to substring. Found by biting this test.
        assert!(
            !matches_tool_pattern("my_read_file", "read_*"),
            "prefix is anchored at the start, not merely present"
        );
        assert!(matches_tool_pattern("read_file", "*_file"), "suffix");
        assert!(
            !matches_tool_pattern("_file_reader", "*_file"),
            "suffix is anchored at the end, not merely present"
        );
        assert!(
            matches_tool_pattern("web_search_api", "*search*"),
            "substring"
        );
        assert!(
            !matches_tool_pattern("web_fetch", "*search*"),
            "substring is real"
        );
        assert!(matches_tool_pattern("exec", "exec"), "no star is exact");
        assert!(
            !matches_tool_pattern("exec_shell", "exec"),
            "no star is not a prefix"
        );
    }

    /// A `*` anywhere but the first or last position is a literal character, and the doc did not
    /// say so. `read_*_file` does not match `read_config_file`; it matches only a tool literally
    /// named `read_*_file`, because the pattern falls through to the exact-match arm with the
    /// asterisk still in it.
    ///
    /// The consequence runs the dangerous way. In `allow`, such a pattern matches nothing and the
    /// tool is refused -- fail-closed, visible. In `deny`, it matches nothing and the tool is
    /// **permitted**, silently, by a line whose author believed it was blocking something.
    #[test]
    fn a_star_in_the_middle_is_a_literal_not_a_wildcard() {
        assert!(
            !matches_tool_pattern("read_config_file", "read_*_file"),
            "a middle star does not expand"
        );
        assert!(
            matches_tool_pattern("read_*_file", "read_*_file"),
            "it is matched literally, asterisk included"
        );
        assert!(
            !matches_tool_pattern("abc", "a*b*c"),
            "multiple interior stars are literal too"
        );
    }

    /// Only-stars and the empty pattern, so the corners are stated rather than discovered.
    #[test]
    fn degenerate_patterns_have_stated_answers() {
        assert!(
            matches_tool_pattern("anything", "**"),
            "leading+trailing star is the substring form with an empty inner"
        );
        assert!(
            matches_tool_pattern("anything", "***"),
            "same, whatever the count"
        );
        assert!(
            !matches_tool_pattern("anything", ""),
            "an empty pattern matches nothing"
        );
        assert!(
            matches_tool_pattern("", "*"),
            "`*` matches the empty tool name too"
        );
    }

    /// Two matchers, one character, two meanings — and the difference is deliberate.
    ///
    /// `matches_tool_pattern` serves the MCP policy's `tools.allow` / `tools.deny`, where a name is
    /// flat and `*` is unbounded. `glob_matches_impl` serves mandate `tool_patterns`, where names
    /// are hierarchical and `*` stops at a `.` while `**` crosses it, the way a filesystem glob
    /// treats `/`.
    ///
    /// Pinned because an undocumented segment rule is the defect itself, whichever way it goes:
    /// AWS documented that a wildcard could not span ARN segments while the implementation spanned
    /// them, it was reported as a security concern, and the issue was archived unresolved
    /// (`awsdocs/iam-user-guide#175`). Two matchers that differ by accident converge by accident.
    /// This test makes the difference a decision.
    #[test]
    fn the_two_tool_pattern_languages_differ_deliberately_at_a_dot() {
        // Flat name: the two agree.
        assert!(matches_tool_pattern("read_file", "read_*"));
        assert!(glob_matches_impl("read_*", "read_file"));

        // Hierarchical name: they do not, and this is the whole difference.
        assert!(
            matches_tool_pattern("fs.read_file", "*"),
            "the policy matcher's `*` is unbounded"
        );
        assert!(
            !glob_matches_impl("*", "fs.read_file"),
            "the mandate matcher's `*` stops at a dot; `**` is the crossing form"
        );
        assert!(
            glob_matches_impl("**", "fs.read_file"),
            "`**` crosses, which is why it exists"
        );

        // `**` means nothing special to the policy matcher: leading and trailing star is the
        // substring form, so it matches for a different reason than a reader might assume.
        assert!(matches_tool_pattern("fs.read_file", "**"));
    }

    /// An unbounded `*` over-blocks in `deny` and over-permits in `allow`. Only the second is
    /// dangerous, and the asymmetry is invisible in a policy file, which is why the reference
    /// states it. Azure RBAC measured the cost of the permissive direction: roughly 39% of actions
    /// reach across Resource Providers under non-obvious wildcards (arXiv 2506.10755), and its
    /// recommendation is explicit enumeration rather than a cleverer pattern language.
    #[test]
    fn a_star_in_allow_admits_every_tool_including_ones_not_yet_written() {
        for tool in ["read_file", "exec", "shell", "a.tool.added.next.week"] {
            assert!(
                matches_tool_pattern(tool, "*"),
                "`allow: ['*']` admits {tool}"
            );
        }
    }
}
