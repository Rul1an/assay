//! NDJSON (Newline Delimited JSON) utilities for evidence events.
//!
//! Provides streaming read/write for evidence event streams without
//! loading entire files into memory.
//!
//! # Format
//!
//! NDJSON is one JSON object per line, separated by `\n`:
//! ```text
//! {"specversion":"1.0","type":"assay.test",...}
//! {"specversion":"1.0","type":"assay.test",...}
//! ```
//!
//! # Security
//!
//! By default, uses strict JSON parsing that rejects:
//! - Duplicate keys at any nesting level (prevents semantic divergence attacks)
//! - Lone surrogates in unicode escapes (prevents verification bypass)
//!
//! Use `NdjsonEventsLax` for legacy compatibility when strict parsing is not needed.
//!
//! # Canonicalization
//!
//! When writing, events are serialized using JCS (RFC 8785) for determinism.
//! When reading, strict JSON parsing is used with duplicate key rejection.

use crate::crypto::jcs;
use crate::json_strict::{validate_json_strict, StrictJsonError};
use crate::types::EvidenceEvent;
use anyhow::{Context, Result};
use std::io::{BufRead, Write};

/// Iterator over NDJSON evidence events.
///
/// Parses events lazily, yielding one `Result<EvidenceEvent>` per line.
/// Empty lines are skipped.
///
/// # Example
///
/// ```no_run
/// use assay_evidence::ndjson::NdjsonEvents;
/// use std::io::BufReader;
/// use std::fs::File;
///
/// let file = File::open("events.ndjson").unwrap();
/// let reader = BufReader::new(file);
///
/// for event in NdjsonEvents::new(reader) {
///     let event = event.unwrap();
///     println!("[{}] {}", event.seq, event.type_);
/// }
/// ```
pub struct NdjsonEvents<R: BufRead> {
    reader: R,
    line_buffer: String,
    line_number: usize,
}

impl<R: BufRead> NdjsonEvents<R> {
    /// Create a new NDJSON event iterator.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            line_buffer: String::new(),
            line_number: 0,
        }
    }

    /// Get current line number (1-indexed, for error messages).
    pub fn line_number(&self) -> usize {
        self.line_number
    }
}

impl<R: BufRead> Iterator for NdjsonEvents<R> {
    type Item = Result<EvidenceEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.line_buffer.clear();

            match self.reader.read_line(&mut self.line_buffer) {
                Ok(0) => return None, // EOF
                Ok(_) => {
                    self.line_number += 1;

                    let line = self.line_buffer.trim();
                    if line.is_empty() {
                        continue; // Skip empty lines
                    }

                    // Phase 1: Strict validation (duplicate keys, lone surrogates)
                    // Maps to ErrorClass::Contract / ErrorCode::ContractInvalidJson
                    if let Err(e) = validate_json_strict(line) {
                        let reason = match &e {
                            StrictJsonError::DuplicateKey { key, path } => {
                                format!("duplicate key '{}' at path '{}'", key, path)
                            }
                            StrictJsonError::LoneSurrogate {
                                position,
                                codepoint,
                            } => {
                                format!(
                                    "invalid unicode (lone surrogate) at position {}: {}",
                                    position, codepoint
                                )
                            }
                            _ => e.to_string(),
                        };
                        return Some(Err(anyhow::anyhow!(
                            "Invalid JSON at line {} (strict validation): {}",
                            self.line_number,
                            reason
                        )));
                    }

                    // Phase 2: Deserialize (now safe from semantic attacks)
                    let result = serde_json::from_str::<EvidenceEvent>(line).with_context(|| {
                        format!(
                            "Invalid JSON at line {}: {}",
                            self.line_number,
                            truncate_line(line, 50)
                        )
                    });

                    return Some(result);
                }
                Err(e) => {
                    return Some(Err(anyhow::Error::new(e)
                        .context(format!("IO error reading line {}", self.line_number + 1))));
                }
            }
        }
    }
}

/// Write events to NDJSON format (canonical JSON per line).
///
/// Events are serialized using JCS (RFC 8785) for deterministic output.
/// Each event is followed by a newline `\n`.
///
/// # Example
///
/// ```no_run
/// use assay_evidence::ndjson::write_events;
/// use assay_evidence::types::EvidenceEvent;
/// use std::fs::File;
///
/// let events: Vec<EvidenceEvent> = vec![/* ... */];
/// let mut file = File::create("events.ndjson").unwrap();
/// write_events(&mut file, &events).unwrap();
/// ```
pub fn write_events<W: Write>(mut writer: W, events: &[EvidenceEvent]) -> Result<()> {
    for (i, event) in events.iter().enumerate() {
        let canonical =
            jcs::to_vec(event).with_context(|| format!("Failed to serialize event {}", i))?;

        writer.write_all(&canonical)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

/// Read all events from NDJSON into a Vec.
///
/// Convenience function when you need all events in memory.
/// For streaming, use `NdjsonEvents` directly.
///
/// # Example
///
/// ```no_run
/// use assay_evidence::ndjson::read_events;
/// use std::io::BufReader;
/// use std::fs::File;
///
/// let file = File::open("events.ndjson").unwrap();
/// let reader = BufReader::new(file);
/// let events = read_events(reader).unwrap();
/// ```
pub fn read_events<R: BufRead>(reader: R) -> Result<Vec<EvidenceEvent>> {
    NdjsonEvents::new(reader).collect()
}

/// Read events from bytes.
pub fn read_events_from_bytes(bytes: &[u8]) -> Result<Vec<EvidenceEvent>> {
    use std::io::{BufReader, Cursor};
    read_events(BufReader::new(Cursor::new(bytes)))
}

/// Write events to bytes (canonical NDJSON).
pub fn write_events_to_bytes(events: &[EvidenceEvent]) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    write_events(&mut buffer, events)?;
    Ok(buffer)
}

/// Truncate line for error messages.
fn truncate_line(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EvidenceEvent;
    use chrono::{TimeZone, Utc};
    use std::io::{BufReader, Cursor};

    #[test]
    fn test_roundtrip() {
        let events = vec![create_event(0), create_event(1), create_event(2)];

        let bytes = write_events_to_bytes(&events).unwrap();
        let loaded = read_events_from_bytes(&bytes).unwrap();

        assert_eq!(events.len(), loaded.len());
        for (orig, loaded) in events.iter().zip(loaded.iter()) {
            assert_eq!(orig.seq, loaded.seq);
            assert_eq!(orig.type_, loaded.type_);
        }
    }

    #[test]
    fn test_ndjson_format() {
        let events = vec![create_event(0)];
        let bytes = write_events_to_bytes(&events).unwrap();
        let content = String::from_utf8(bytes).unwrap();

        // Should be single line + newline
        assert!(content.ends_with('\n'));
        assert_eq!(content.lines().count(), 1);

        // Should be valid JSON
        let _: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    }

    #[test]
    fn test_empty_lines_skipped() {
        let ndjson = r#"
{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:0","time":"2023-11-14T22:13:20Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{}}

{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:1","time":"2023-11-14T22:13:20Z","datacontenttype":"application/json","assayrunid":"run","assayseq":1,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{}}
"#;

        let cursor = Cursor::new(ndjson);
        let reader = BufReader::new(cursor);
        let events: Vec<_> = NdjsonEvents::new(reader)
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_invalid_json_error() {
        let ndjson = "not valid json\n";
        let cursor = Cursor::new(ndjson);
        let reader = BufReader::new(cursor);
        let mut iter = NdjsonEvents::new(reader);

        let result = iter.next().unwrap();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("line 1"));
    }

    #[test]
    fn test_determinism() {
        let events = vec![create_event(0), create_event(1)];

        let bytes1 = write_events_to_bytes(&events).unwrap();
        let bytes2 = write_events_to_bytes(&events).unwrap();

        assert_eq!(bytes1, bytes2, "NDJSON output must be deterministic");
    }

    fn create_event(seq: u64) -> EvidenceEvent {
        let mut event = EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_test",
            seq,
            serde_json::json!({"seq": seq}),
        );
        event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
        event
    }

    // === Strict JSON Security Tests ===

    #[test]
    fn test_rejects_duplicate_keys_in_event() {
        // Attack: duplicate mandate_id to confuse verification
        let ndjson = r#"{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:0","time":"2023-11-14T22:13:20Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{"key":"a","key":"b"}}"#;

        let cursor = Cursor::new(ndjson);
        let reader = BufReader::new(cursor);
        let mut iter = NdjsonEvents::new(reader);

        let result = iter.next().unwrap();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.to_lowercase().contains("duplicate") || err.contains("strict validation"),
            "Expected duplicate key error, got: {}",
            err
        );
    }

    #[test]
    fn test_rejects_duplicate_keys_in_nested_data() {
        // Attack: duplicate in nested structure
        let ndjson = r#"{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:0","time":"2023-11-14T22:13:20Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{"nested":{"a":1,"a":2}}}"#;

        let cursor = Cursor::new(ndjson);
        let reader = BufReader::new(cursor);
        let mut iter = NdjsonEvents::new(reader);

        let result = iter.next().unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn test_rejects_lone_surrogate() {
        // Attack: lone surrogate could cause verification/display mismatch
        let ndjson = r#"{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:0","time":"2023-11-14T22:13:20Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{"value":"\uD800"}}"#;

        let cursor = Cursor::new(ndjson);
        let reader = BufReader::new(cursor);
        let mut iter = NdjsonEvents::new(reader);

        let result = iter.next().unwrap();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("surrogate") || err.contains("Security violation"),
            "Expected surrogate error, got: {}",
            err
        );
    }
}
