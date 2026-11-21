//! Syntax layer shared by typed documents and formats that consume the authored tree.

/// Decode one YAML value, retaining the parser error and its source location.
// The reader counter's live control: scripts/ci/closed_reader_count.py counts this call although
// the attribute allows it, so a run that honoured the attribute would come up one call short.
#[allow(clippy::disallowed_methods)]
pub fn parse_tree(bytes: &[u8]) -> Result<serde_yaml::Value, serde_yaml::Error> {
    serde_yaml::from_slice(bytes)
}
