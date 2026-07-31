//! Resource-bounded ingest for untrusted MCP transcript bytes.
//!
//! This layer owns the cost of reaching the existing MCP parser and refuses typed conclusion states
//! that cannot support a clean reading. It does not derive a policy decision or change the public
//! [`McpEvent`] shape. When a caller supplies the transcript as an optional companion input, a
//! refusal can veto that import but can never author or alter the producer's evidence.

use super::era::{conclude, conclude_request, RequestAssessment, ResultConclusion};
use super::parser::parse_mcp_transcript_detailed;
use super::{McpEvent, McpInputFormat};
use assay_common::limits::{LimitExceeded, LimitKind, LimitReader};
use serde::de::{DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use std::io::Read;

/// Domain-local ceilings for one MCP transcript.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct McpTranscriptLimits {
    /// Maximum bytes read from the source before UTF-8 materialization.
    pub max_source_bytes: u64,
    /// Maximum bytes in one JSON-RPC JSONL line.
    pub max_line_bytes: u64,
    /// Maximum messages or transport entries in one transcript.
    pub max_events: usize,
    /// Maximum structural JSON nesting depth.
    pub max_json_depth: usize,
}

impl Default for McpTranscriptLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 16 * 1024 * 1024,
            max_line_bytes: 1024 * 1024,
            max_events: 100_000,
            max_json_depth: 64,
        }
    }
}

/// A typed, value-free refusal at the MCP transcript boundary.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpTranscriptIngestError {
    #[error("MCP transcript exceeded source-byte limit of {limit}")]
    SourceBytes { limit: u64 },
    #[error("MCP transcript JSONL line exceeded byte limit of {limit}")]
    LineBytes { limit: u64 },
    #[error("MCP transcript event count exceeded limit of {limit}")]
    Events { limit: usize },
    #[error("MCP transcript JSON nesting exceeded depth limit of {limit}")]
    JsonDepth { limit: usize },
    #[error("MCP transcript is not valid UTF-8")]
    InvalidUtf8,
    #[error("MCP transcript could not be read")]
    ReadFailed,
    #[error("MCP transcript is invalid")]
    InvalidTranscript,
    /// At least one request or result needs evidence this transcript does not provide.
    #[error("MCP transcript conclusion is incomplete")]
    ConclusionIncomplete,
    /// At least one request or result carries a protocol-invalid observation.
    #[error("MCP transcript conclusion is invalid")]
    ConclusionInvalid,
}

/// Read and parse one transcript under explicit limits, refusing unusable conclusion states.
///
/// `Terminal` and `NonTerminal` results are both accepted. `Incomplete` and `Invalid` request or
/// result states are mapped to value-free ingest errors; no policy decision or profile carrier is
/// derived from them.
pub fn parse_mcp_transcript_bounded<R: Read>(
    reader: R,
    format: McpInputFormat,
    limits: McpTranscriptLimits,
) -> Result<Vec<McpEvent>, McpTranscriptIngestError> {
    let mut reader = LimitReader::new(reader, limits.max_source_bytes, LimitKind::SourceBytes);
    let mut bytes = Vec::new();
    if let Err(error) = reader.read_to_end(&mut bytes) {
        if let Some(LimitExceeded { limit, .. }) = LimitExceeded::from_io(&error) {
            return Err(McpTranscriptIngestError::SourceBytes { limit });
        }
        return Err(McpTranscriptIngestError::ReadFailed);
    }
    check_json_depth(&bytes, format, limits.max_json_depth)?;
    if format == McpInputFormat::JsonRpc {
        check_jsonl_lines(&bytes, limits.max_line_bytes)?;
    }
    let text = String::from_utf8(bytes).map_err(|_| McpTranscriptIngestError::InvalidUtf8)?;
    check_event_count(text.as_bytes(), format, limits.max_events)?;
    let parsed = parse_mcp_transcript_detailed(&text, format)
        .map_err(|_| McpTranscriptIngestError::InvalidTranscript)?;
    for entry in &parsed {
        if let Some(metadata) = &entry.context.request_metadata {
            match conclude_request(
                &entry.context.era,
                metadata,
                entry.context.capability_observation.as_ref(),
            ) {
                RequestAssessment::Valid => {}
                RequestAssessment::Incomplete(_) => {
                    return Err(McpTranscriptIngestError::ConclusionIncomplete)
                }
                RequestAssessment::Invalid(_) => {
                    return Err(McpTranscriptIngestError::ConclusionInvalid)
                }
            }
        }
        if let Some(result) = &entry.context.result_observation {
            match conclude(
                &entry.context.era,
                result,
                entry.context.capability_observation.as_ref(),
            ) {
                ResultConclusion::Terminal | ResultConclusion::NonTerminal => {}
                ResultConclusion::Incomplete(_) => {
                    return Err(McpTranscriptIngestError::ConclusionIncomplete)
                }
                ResultConclusion::Invalid(_) => {
                    return Err(McpTranscriptIngestError::ConclusionInvalid)
                }
            }
        }
        if entry.is_error_response {
            return Err(McpTranscriptIngestError::ConclusionIncomplete);
        }
    }
    Ok(parsed.into_iter().map(|entry| entry.event).collect())
}

fn check_jsonl_lines(bytes: &[u8], max_line_bytes: u64) -> Result<(), McpTranscriptIngestError> {
    for line in bytes.split(|byte| *byte == b'\n') {
        if line.len() as u64 > max_line_bytes {
            return Err(McpTranscriptIngestError::LineBytes {
                limit: max_line_bytes,
            });
        }
    }
    Ok(())
}

fn check_json_depth(
    bytes: &[u8],
    format: McpInputFormat,
    max_json_depth: usize,
) -> Result<(), McpTranscriptIngestError> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for &byte in bytes {
        if byte == b'\n' {
            // Raw newlines cannot occur inside JSON strings. Recover here so malformed JSONL on
            // one row cannot hide structural depth on later rows from the resource guard.
            in_string = false;
            escaped = false;
            if format == McpInputFormat::JsonRpc {
                depth = 0;
            }
            continue;
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > max_json_depth {
                    return Err(McpTranscriptIngestError::JsonDepth {
                        limit: max_json_depth,
                    });
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

fn check_event_count(
    bytes: &[u8],
    format: McpInputFormat,
    max_events: usize,
) -> Result<(), McpTranscriptIngestError> {
    let count = match format {
        McpInputFormat::JsonRpc => {
            let count = bytes
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
                .count();
            if count > max_events {
                CountOutcome::OverLimit
            } else {
                CountOutcome::WithinLimit
            }
        }
        McpInputFormat::Inspector => count_container_array(bytes, "events", true, max_events)?,
        McpInputFormat::StreamableHttp | McpInputFormat::HttpSse => {
            count_container_array(bytes, "entries", false, max_events)?
        }
    };
    match count {
        CountOutcome::WithinLimit => Ok(()),
        CountOutcome::OverLimit => Err(McpTranscriptIngestError::Events { limit: max_events }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CountOutcome {
    WithinLimit,
    OverLimit,
}

fn count_container_array(
    bytes: &[u8],
    array_key: &'static str,
    root_array_is_target: bool,
    max_events: usize,
) -> Result<CountOutcome, McpTranscriptIngestError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer
        .deserialize_any(ContainerVisitor {
            array_key,
            root_array_is_target,
            max_events,
        })
        .map_err(|_| McpTranscriptIngestError::InvalidTranscript)
}

struct ContainerVisitor {
    array_key: &'static str,
    root_array_is_target: bool,
    max_events: usize,
}

impl<'de> Visitor<'de> for ContainerVisitor {
    type Value = CountOutcome;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an MCP transcript container")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, sequence: A) -> Result<Self::Value, A::Error> {
        if self.root_array_is_target {
            CountSeed {
                max_events: self.max_events,
            }
            .count(sequence)
        } else {
            drain_sequence(sequence)?;
            Ok(CountOutcome::WithinLimit)
        }
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut outcome = CountOutcome::WithinLimit;
        while let Some(key) = map.next_key::<String>()? {
            if key == self.array_key {
                let observed = map.next_value_seed(CountSeed {
                    max_events: self.max_events,
                })?;
                if observed == CountOutcome::OverLimit {
                    return Ok(observed);
                }
                outcome = observed;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(outcome)
    }

    fn visit_bool<E: serde::de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_i64<E: serde::de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_u64<E: serde::de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_f64<E: serde::de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_str<E: serde::de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_string<E: serde::de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }
}

struct CountSeed {
    max_events: usize,
}

impl CountSeed {
    fn count<'de, A: SeqAccess<'de>>(&self, mut sequence: A) -> Result<CountOutcome, A::Error> {
        let mut count = 0usize;
        while sequence.next_element::<IgnoredAny>()?.is_some() {
            count += 1;
            if count > self.max_events {
                return Ok(CountOutcome::OverLimit);
            }
        }
        Ok(CountOutcome::WithinLimit)
    }
}

impl<'de> DeserializeSeed<'de> for CountSeed {
    type Value = CountOutcome;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(CountValueVisitor {
            max_events: self.max_events,
        })
    }
}

struct CountValueVisitor {
    max_events: usize,
}

impl<'de> Visitor<'de> for CountValueVisitor {
    type Value = CountOutcome;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an event array")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, sequence: A) -> Result<Self::Value, A::Error> {
        CountSeed {
            max_events: self.max_events,
        }
        .count(sequence)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        drain_map(map)?;
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_bool<E: serde::de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_i64<E: serde::de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_u64<E: serde::de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_f64<E: serde::de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_str<E: serde::de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_string<E: serde::de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(CountOutcome::WithinLimit)
    }
}

fn drain_sequence<'de, A: SeqAccess<'de>>(mut sequence: A) -> Result<(), A::Error> {
    while sequence.next_element::<IgnoredAny>()?.is_some() {}
    Ok(())
}

fn drain_map<'de, A: MapAccess<'de>>(mut map: A) -> Result<(), A::Error> {
    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    const NOTIFICATION: &str = r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{}}"#;

    fn modern_transport(result: &str) -> String {
        format!(
            r#"{{"transport":"streamable-http","transport_context":{{"headers":{{"MCP-Protocol-Version":"2026-07-28"}}}},"entries":[{{"request":{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"example.tool","arguments":{{}},"_meta":{{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{{}}}}}}}}}},{{"response":{{"jsonrpc":"2.0","id":1,"result":{result}}}}}]}}"#
        )
    }

    fn modern_transport_error() -> String {
        r#"{"transport":"streamable-http","transport_context":{"headers":{"MCP-Protocol-Version":"2026-07-28"}},"entries":[{"request":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"example.tool","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}},{"response":{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"ATTACKER_SENTINEL"}}}]}"#.to_string()
    }

    fn limits() -> McpTranscriptLimits {
        McpTranscriptLimits {
            max_source_bytes: 4096,
            max_line_bytes: 4096,
            max_events: 8,
            max_json_depth: 16,
        }
    }

    fn assert_source_limit(error: McpTranscriptIngestError, limit: u64) {
        assert!(
            matches!(error, McpTranscriptIngestError::SourceBytes { limit: got } if got == limit),
            "expected source-byte refusal at {limit}, got {error:?}"
        );
    }

    #[test]
    fn source_limit_is_inclusive_and_one_more_byte_is_refused() {
        let bytes = NOTIFICATION.as_bytes();
        let mut exact = limits();
        exact.max_source_bytes = bytes.len() as u64;
        assert_eq!(
            parse_mcp_transcript_bounded(Cursor::new(bytes), McpInputFormat::JsonRpc, exact)
                .unwrap()
                .len(),
            1
        );

        let mut over = exact;
        over.max_source_bytes -= 1;
        let error = parse_mcp_transcript_bounded(Cursor::new(bytes), McpInputFormat::JsonRpc, over)
            .unwrap_err();
        assert_source_limit(error, over.max_source_bytes);
    }

    #[test]
    fn short_non_seekable_reads_cannot_walk_past_the_source_limit() {
        struct OneByteAtATime(Cursor<Vec<u8>>);
        impl Read for OneByteAtATime {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                if output.is_empty() {
                    return Ok(0);
                }
                self.0.read(&mut output[..1])
            }
        }

        let bytes = NOTIFICATION.as_bytes().to_vec();
        let mut exact = limits();
        exact.max_source_bytes = bytes.len() as u64;
        assert!(parse_mcp_transcript_bounded(
            OneByteAtATime(Cursor::new(bytes.clone())),
            McpInputFormat::JsonRpc,
            exact,
        )
        .is_ok());

        let mut over = exact;
        over.max_source_bytes -= 1;
        let error = parse_mcp_transcript_bounded(
            OneByteAtATime(Cursor::new(bytes)),
            McpInputFormat::JsonRpc,
            over,
        )
        .unwrap_err();
        assert_source_limit(error, over.max_source_bytes);
    }

    #[test]
    fn source_limit_fires_before_invalid_utf8_is_materialized() {
        let input = [b'{', b'}', 0xff];
        let mut bounded = limits();
        bounded.max_source_bytes = 2;
        let error =
            parse_mcp_transcript_bounded(Cursor::new(input), McpInputFormat::Inspector, bounded)
                .unwrap_err();
        assert_source_limit(error, 2);
    }

    #[test]
    fn jsonl_line_limit_is_inclusive_and_one_more_byte_is_refused() {
        let mut exact = limits();
        exact.max_line_bytes = NOTIFICATION.len() as u64;
        assert!(parse_mcp_transcript_bounded(
            Cursor::new(NOTIFICATION),
            McpInputFormat::JsonRpc,
            exact,
        )
        .is_ok());

        let mut over = exact;
        over.max_line_bytes -= 1;
        let error =
            parse_mcp_transcript_bounded(Cursor::new(NOTIFICATION), McpInputFormat::JsonRpc, over)
                .unwrap_err();
        assert!(
            matches!(error, McpTranscriptIngestError::LineBytes { limit } if limit == over.max_line_bytes)
        );
    }

    #[test]
    fn jsonl_line_limit_fires_before_invalid_json_is_parsed() {
        let mut bounded = limits();
        bounded.max_line_bytes = 3;
        let error =
            parse_mcp_transcript_bounded(Cursor::new("not-json"), McpInputFormat::JsonRpc, bounded)
                .unwrap_err();
        assert!(matches!(
            error,
            McpTranscriptIngestError::LineBytes { limit: 3 }
        ));
    }

    #[test]
    fn jsonrpc_event_limit_is_applied_before_the_parser_builds_the_event_vector() {
        let transcript = format!("{NOTIFICATION}\n{NOTIFICATION}");
        let mut exact = limits();
        exact.max_events = 2;
        assert_eq!(
            parse_mcp_transcript_bounded(Cursor::new(&transcript), McpInputFormat::JsonRpc, exact,)
                .unwrap()
                .len(),
            2
        );

        let mut over = exact;
        over.max_events = 1;
        let error =
            parse_mcp_transcript_bounded(Cursor::new(&transcript), McpInputFormat::JsonRpc, over)
                .unwrap_err();
        assert!(matches!(
            error,
            McpTranscriptIngestError::Events { limit: 1 }
        ));

        let malformed_second = format!("{NOTIFICATION}\nnot-json");
        let error = parse_mcp_transcript_bounded(
            Cursor::new(malformed_second),
            McpInputFormat::JsonRpc,
            over,
        )
        .unwrap_err();
        assert!(
            matches!(error, McpTranscriptIngestError::Events { limit: 1 }),
            "event ceiling must decide before a later row reaches the semantic parser"
        );
    }

    #[test]
    fn inspector_event_limit_is_applied_to_the_events_array() {
        let transcript = format!(r#"{{"events":[{NOTIFICATION},{NOTIFICATION}]}}"#);
        let mut bounded = limits();
        bounded.max_events = 1;
        let error = parse_mcp_transcript_bounded(
            Cursor::new(transcript),
            McpInputFormat::Inspector,
            bounded,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            McpTranscriptIngestError::Events { limit: 1 }
        ));
    }

    #[test]
    fn transport_event_limit_is_applied_to_the_entries_array() {
        let entry = format!(r#"{{"request":{NOTIFICATION}}}"#);
        let transcript =
            format!(r#"{{"transport":"streamable-http","entries":[{entry},{entry}]}}"#);
        let mut bounded = limits();
        bounded.max_events = 1;
        let error = parse_mcp_transcript_bounded(
            Cursor::new(transcript),
            McpInputFormat::StreamableHttp,
            bounded,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            McpTranscriptIngestError::Events { limit: 1 }
        ));
    }

    #[test]
    fn json_depth_limit_is_inclusive_and_one_more_level_is_refused() {
        let transcript =
            r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{"outer":{"leaf":1}}}"#;
        let mut exact = limits();
        exact.max_json_depth = 3;
        assert!(parse_mcp_transcript_bounded(
            Cursor::new(transcript),
            McpInputFormat::JsonRpc,
            exact,
        )
        .is_ok());

        let mut over = exact;
        over.max_json_depth = 2;
        let error =
            parse_mcp_transcript_bounded(Cursor::new(transcript), McpInputFormat::JsonRpc, over)
                .unwrap_err();
        assert!(matches!(
            error,
            McpTranscriptIngestError::JsonDepth { limit: 2 }
        ));
    }

    #[test]
    fn braces_inside_strings_do_not_consume_json_depth_budget() {
        let transcript =
            r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{"text":"{{[[}}]]"}}"#;
        let mut bounded = limits();
        bounded.max_json_depth = 2;
        assert!(parse_mcp_transcript_bounded(
            Cursor::new(transcript),
            McpInputFormat::JsonRpc,
            bounded,
        )
        .is_ok());
    }

    #[test]
    fn unterminated_jsonl_string_cannot_mask_depth_on_the_next_line() {
        let transcript = "{\"text\":\"unterminated\n[[[]]]";
        let mut bounded = limits();
        bounded.max_json_depth = 2;
        let error =
            parse_mcp_transcript_bounded(Cursor::new(transcript), McpInputFormat::JsonRpc, bounded)
                .unwrap_err();
        assert!(matches!(
            error,
            McpTranscriptIngestError::JsonDepth { limit: 2 }
        ));
    }

    #[test]
    fn unbalanced_jsonl_depth_does_not_carry_into_the_next_line() {
        let transcript = format!("{{\"open\":{{\n{NOTIFICATION}");
        let mut bounded = limits();
        bounded.max_json_depth = 2;
        let error =
            parse_mcp_transcript_bounded(Cursor::new(transcript), McpInputFormat::JsonRpc, bounded)
                .unwrap_err();
        assert!(matches!(error, McpTranscriptIngestError::InvalidTranscript));
    }

    #[test]
    fn diagnostics_do_not_echo_attacker_controlled_input() {
        let input = br#"{"ATTACKER_SENTINEL":"#;
        let error =
            parse_mcp_transcript_bounded(Cursor::new(input), McpInputFormat::Inspector, limits())
                .unwrap_err();
        assert!(matches!(error, McpTranscriptIngestError::InvalidTranscript));
        assert!(!error.to_string().contains("ATTACKER_SENTINEL"));
        assert!(!format!("{error:?}").contains("ATTACKER_SENTINEL"));
    }

    #[test]
    fn unresolved_request_era_is_refused_after_bounded_parsing() {
        let request = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"example.tool","arguments":{}}}"#;
        let error =
            parse_mcp_transcript_bounded(Cursor::new(request), McpInputFormat::JsonRpc, limits())
                .unwrap_err();
        assert_eq!(error.to_string(), "MCP transcript conclusion is incomplete");
    }

    #[test]
    fn modern_result_without_result_type_is_refused_after_bounded_parsing() {
        let transcript = modern_transport(r#"{"content":[]}"#);
        let error = parse_mcp_transcript_bounded(
            Cursor::new(transcript),
            McpInputFormat::StreamableHttp,
            limits(),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "MCP transcript conclusion is invalid");
    }

    #[test]
    fn unrecognized_modern_result_is_not_promoted_to_a_clean_reading() {
        let transcript = modern_transport(r#"{"content":[],"resultType":"future-state"}"#);
        let error = parse_mcp_transcript_bounded(
            Cursor::new(transcript),
            McpInputFormat::StreamableHttp,
            limits(),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "MCP transcript conclusion is incomplete");
        assert!(!format!("{error:?}").contains("future-state"));
    }

    #[test]
    fn modern_error_response_cannot_bypass_the_conclusion_gate() {
        let error = parse_mcp_transcript_bounded(
            Cursor::new(modern_transport_error()),
            McpInputFormat::StreamableHttp,
            limits(),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "MCP transcript conclusion is incomplete");
        assert!(!format!("{error:?}").contains("ATTACKER_SENTINEL"));
    }

    #[test]
    fn valid_modern_input_required_with_continuation_is_accepted() {
        let transcript =
            modern_transport(r#"{"resultType":"input_required","requestState":"opaque"}"#);
        assert!(parse_mcp_transcript_bounded(
            Cursor::new(transcript),
            McpInputFormat::StreamableHttp,
            limits(),
        )
        .is_ok());
    }
}
