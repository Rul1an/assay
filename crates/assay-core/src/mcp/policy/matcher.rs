//! The MCP policy tool-pattern matcher, and the one test that can only live here.
//!
//! The matcher itself moved to `assay_common::tool_pattern`, together with the tests that
//! describe its behaviour, because `assay-metrics` held a byte-identical second copy of it and a
//! pattern language is exactly the kind of mechanism a second implementation silently redefines.
//!
//! What stays is the divergence test. It compares this matcher against the mandate glob in
//! `assay-evidence`, and `assay-core` is the only crate that depends on both, so it is the only
//! place the comparison can be written at all.

pub(in crate::mcp::policy) use assay_common::tool_pattern::matches_tool_pattern;

#[cfg(test)]
mod tests {
    use super::matches_tool_pattern;
    use crate::runtime::glob_matches_impl;

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
}
