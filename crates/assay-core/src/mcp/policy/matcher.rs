pub(in crate::mcp::policy) fn matches_tool_pattern(tool_name: &str, pattern: &str) -> bool {
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
    use crate::runtime::glob_matches_impl;

    /// The five behaviours `docs/reference/config/policies.md` promises, pinned so the doc and the
    /// matcher cannot drift apart silently. Nothing was checking that they agreed.
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

    /// A `*` that is neither the first nor the last character is a literal, and the doc did not
    /// say so. `read_*_file` does not match `read_config_file`; it matches only a tool literally
    /// named `read_*_file`, because the pattern falls through to the exact-match arm with the
    /// asterisk still in it.
    ///
    /// The consequence runs the dangerous way. In `allow`, such a pattern matches nothing and the
    /// tool is refused -- fail-closed, visible. In `deny`, it matches nothing and the tool is
    /// **permitted**, silently, by a line whose author believed it was blocking something.
    ///
    /// It is not only the exact-match arm. A pattern that is a wildcard at both ends carries its
    /// interior stars into the substring it searches for, so `*a*b*` looks for the three
    /// characters `a*b` and finds them only in a tool name that really contains an asterisk.
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
        assert!(
            !matches_tool_pattern("axbc", "*a*b*"),
            "an interior star inside the substring form is literal as well"
        );
        assert!(
            matches_tool_pattern("a*b", "*a*b*"),
            "which is to say the substring searched for is `a*b`, asterisk and all"
        );
    }

    /// Only-stars and the empty pattern, so the corners are stated rather than discovered.
    ///
    /// The first version of this test asserted `!matches_tool_pattern("anything", "")` under the
    /// message "an empty pattern matches nothing". The assertion held and the message was false:
    /// an empty pattern has no star, so it takes the exact-match arm and matches the empty tool
    /// name. A corner stated wrongly is worse than a corner left undocumented, and this test
    /// exists to state corners.
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
            "an empty pattern is an exact match against the empty name, so it matches no real tool"
        );
        assert!(
            matches_tool_pattern("", ""),
            "and it does match the empty name, which is what makes it exact rather than inert"
        );
        assert!(
            matches_tool_pattern("", "*"),
            "`*` matches the empty tool name too"
        );
    }

    /// Two matchers, one character, several meanings — and the differences are deliberate.
    ///
    /// `matches_tool_pattern` serves the MCP policy's `tools.allow` / `tools.deny`, where a name is
    /// flat and `*` is unbounded. `glob_matches_impl` serves mandate `tool_patterns`, where names
    /// are hierarchical and `*` stops at a `.` while `**` crosses it, the way a filesystem glob
    /// treats `/`.
    ///
    /// Neither is uniformly stricter, which is the part worth pinning. At a dot the mandate
    /// matcher refuses what the policy matcher admits; at an interior star or a backslash escape
    /// it admits what the policy matcher refuses, because the policy matcher has no such syntax.
    ///
    /// Pinned because an unstated segment rule is the defect itself, whichever way it goes. AWS's
    /// resource-ARN reference says a wildcard "cannot span segments", meaning the colon-delimited
    /// parts of an ARN. A reader took `segment` in its other common sense — the slash-delimited
    /// parts of a path — checked `arn:aws:s3:::my-bucket/foo/*/bar` in the policy simulator, found
    /// it spanning slashes, and filed it as a security concern with a worked misconfiguration
    /// (`awsdocs/iam-user-guide#175`, 2020). The documentation was correct; he withdrew the same
    /// day. What survives the correction is the hazard: the rule was stated in a vocabulary the
    /// reader did not share, so he inferred one and reasoned about the breadth of real policies
    /// from it. Two matchers that differ by accident converge by accident. This test makes the
    /// difference a decision.
    #[test]
    fn the_two_tool_pattern_languages_differ_deliberately() {
        // Flat name, no interior syntax: the two agree.
        assert!(matches_tool_pattern("read_file", "read_*"));
        assert!(glob_matches_impl("read_*", "read_file"));

        // A dot: the mandate matcher is the stricter one.
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

        // An interior star: the mandate matcher is the more permissive one. Stated in this
        // direction because "they disagree about `.`" was the whole claim before, and it is only
        // half of the disagreement -- the half where the policy matcher is safe by accident.
        assert!(
            glob_matches_impl("read_*_file", "read_config_file"),
            "the mandate matcher expands an interior star"
        );
        assert!(
            !matches_tool_pattern("read_config_file", "read_*_file"),
            "the policy matcher reads it as a literal asterisk"
        );

        // An escape: the mandate matcher has one and the policy matcher has no notion of it, so
        // `\` is just a character to match.
        assert!(
            glob_matches_impl(r"file\*name", "file*name"),
            r"`\*` is a literal asterisk to the mandate matcher"
        );
        assert!(
            !matches_tool_pattern("file*name", r"file\*name"),
            "to the policy matcher the backslash is part of the name it is looking for"
        );

        // `**` means nothing special to the policy matcher: leading and trailing star is the
        // substring form, so it matches for a different reason than a reader might assume.
        assert!(matches_tool_pattern("fs.read_file", "**"));
    }

    /// An unbounded `*` over-blocks in `deny` and over-permits in `allow`. Only the second is
    /// dangerous, and the asymmetry is invisible in a policy file, which is why the reference
    /// states it. Azure RBAC measured the cost of the permissive direction: about half of the
    /// 15,481 catalogued actions reach across Resource Providers under non-obvious wildcards
    /// (arXiv 2506.10755v3), and the recommendation is explicit enumeration rather than a
    /// cleverer pattern language.
    ///
    /// Note what this test cannot show. `*` reaches the same answer through the empty-inner
    /// substring arm, so no branch is uniquely its own -- an earlier version of this file carried
    /// a `pattern == "*"` guard clause above the match, and deleting it changed nothing. A test
    /// named for a pattern is not a test of a branch.
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
