//! The tool-pattern matcher this module used to define now lives in `assay_common`.
//!
//! It was a byte-identical copy of `assay-core`'s, with no tests of its own, serving both
//! `policy.deny` and `policy.allow` in `evaluator.rs`. The two had already begun to drift: when
//! `assay-core` dropped a dead `pattern == "*"` guard clause in #2117, this copy kept it. That is
//! the harmless direction of drift, and the point is that nothing would have reported the other
//! one — the interior-star rule (`read_*_file` is a literal, so a `deny` entry shaped like a glob
//! blocks nothing) held here too, unstated and unpinned.
//!
//! Per `CLAUDE.md`, one rule gets one function; a parity test is the fallback for when that is
//! impossible, and here it was not impossible.

pub(super) use assay_common::tool_pattern::matches_tool_pattern;
