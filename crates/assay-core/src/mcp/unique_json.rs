//! Unique-member JSON ingress for live MCP proxy lines.
//!
//! `serde_json::Value` collapses duplicate object members last-value-wins, so a
//! later parse of the same bytes can disagree with the tree a policy decision
//! already used. This boundary refuses duplicates in one pass and applies the
//! transcript line-byte ceiling before building a tree.

use super::era::UniqueValue;
use super::ingest::McpTranscriptLimits;
use serde_json::Value;

/// Why a client line cannot be authorized.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UniqueJsonError {
    DuplicateMember,
    Malformed,
    LineTooLong { limit: u64 },
}

impl std::fmt::Display for UniqueJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateMember => f.write_str("JSON object contains a duplicate member"),
            Self::Malformed => f.write_str("malformed JSON"),
            Self::LineTooLong { limit } => {
                write!(f, "JSON line exceeded byte limit of {limit}")
            }
        }
    }
}

impl std::error::Error for UniqueJsonError {}

/// Parse one client line, refusing duplicate members and oversized lines.
///
/// The line-byte ceiling is [`McpTranscriptLimits::max_line_bytes`] so live
/// proxy ingest and transcript ingest share one number.
pub fn parse_unique_json(line: &str) -> Result<Value, UniqueJsonError> {
    let limit = McpTranscriptLimits::default().max_line_bytes;
    if (line.len() as u64) > limit {
        return Err(UniqueJsonError::LineTooLong { limit });
    }
    match serde_json::from_str::<UniqueValue>(line) {
        Ok(UniqueValue(value)) => Ok(value),
        Err(error) => {
            if error.to_string().contains("duplicate member") {
                Err(UniqueJsonError::DuplicateMember)
            } else {
                Err(UniqueJsonError::Malformed)
            }
        }
    }
}

/// Whether an unparsable line still looks like a JSON-RPC method frame.
///
/// Matches the wrap path's existing method-bearing check so only that class of
/// opaque line changes from "warn and forward" to refuse.
pub fn is_method_bearing_frame(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('{')
        && (trimmed.contains("\"method\"")
            || trimmed.contains("\"params\"")
            || trimmed.contains("\"tool\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unique_members_parse() {
        let v = parse_unique_json(r#"{"name":"echo","arguments":{"x":1}}"#).unwrap();
        assert_eq!(v["name"], "echo");
        assert_eq!(v["arguments"]["x"], 1);
    }

    #[test]
    fn duplicate_members_are_refused() {
        let err = parse_unique_json(r#"{"name":"alpha","name":"bravo"}"#).unwrap_err();
        assert_eq!(err, UniqueJsonError::DuplicateMember);
        let rendered = format!("{err}");
        assert!(!rendered.contains("alpha") && !rendered.contains("bravo"));
    }

    #[test]
    fn malformed_is_refused() {
        let err = parse_unique_json(r#"{"name":"echo""#).unwrap_err();
        assert_eq!(err, UniqueJsonError::Malformed);
    }

    #[test]
    fn oversized_line_is_refused_before_a_second_parse() {
        let limit = McpTranscriptLimits::default().max_line_bytes;
        let line = "x".repeat((limit as usize) + 1);
        assert_eq!(
            parse_unique_json(&line),
            Err(UniqueJsonError::LineTooLong { limit })
        );
    }

    #[test]
    fn method_bearing_detects_the_wrap_heuristic() {
        assert!(is_method_bearing_frame(
            r#"{"jsonrpc":"2.0","method":"tools/call""#
        ));
        assert!(!is_method_bearing_frame("not-json"));
        assert_eq!(json!({"name": "echo"})["name"], "echo");
    }
}
