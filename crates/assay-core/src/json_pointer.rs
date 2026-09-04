//! Shared RFC 6901 JSON Pointer segment escaping and path construction.
//!
//! One construction for reference-token escape and pointer join. Callers must
//! not reimplement `~` / `/` escaping or dotted/`[i]` path notation beside this.

/// Escape one JSON Pointer reference token (RFC 6901 §3).
///
/// Replace `~` with `~0`, then `/` with `~1`. Order is normative: reversing it
/// turns a literal `~1` into an unintended `/`.
#[must_use]
pub(crate) fn escape_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// Unescape one JSON Pointer reference token (RFC 6901 §3).
#[must_use]
pub(crate) fn unescape_segment(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

/// Append an unescaped segment to a JSON Pointer.
///
/// `parent` is either empty (build from the root) or an absolute pointer that
/// already starts with `/`. The segment is escaped before joining.
#[must_use]
pub(crate) fn append_segment(parent: &str, segment: &str) -> String {
    let escaped = escape_segment(segment);
    if parent.is_empty() {
        format!("/{escaped}")
    } else {
        debug_assert!(
            parent.starts_with('/'),
            "JSON Pointer parent must be absolute or empty, got {parent:?}"
        );
        format!("{parent}/{escaped}")
    }
}
