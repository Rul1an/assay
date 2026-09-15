//! Per-occurrence truncation readings for `trace verify`, one ASCII line each.
//!
//! A row is a locator plus a reading. The locator is the ordinal of the yielded
//! event, its kind and the identifiers it carries; it is not a physical JSONL
//! line, not unique, and not authenticated provenance. The reading comes from
//! [`read_observed`], the one shared reader: this module adds no loss or trust
//! rule of its own. Absence of an upgrader observation is printed as such, never
//! as clean.

use super::observation::{read_observed, ObservedTraceEvent, TruncationReading};
use super::schema::TraceEvent;
use super::upgrader::declared_field_roots;
use std::fmt::Write as _;
use std::io::{self, Write};

/// Write one row per declared field root of `observed`, or one locator row with
/// `fields=none` for an event kind that declares no field.
pub fn write_event_readings(
    out: &mut dyn Write,
    ordinal: u64,
    observed: &ObservedTraceEvent,
    trusted_stages: &[&str],
) -> io::Result<()> {
    let locator = locator(ordinal, observed.event());
    let roots = declared_field_roots(observed.event());
    if roots.is_empty() {
        return writeln!(out, "truncation {locator} fields=none");
    }
    for pointer in roots {
        let reading = read_observed(observed, pointer, trusted_stages);
        writeln!(
            out,
            "truncation {locator} pointer={pointer} {}",
            render_reading(&reading)
        )?;
    }
    Ok(())
}

/// Announce that rows stop at `after_ordinal` because reading the stream failed.
/// Rows already written describe those events only; there is no whole-trace result.
pub fn write_incomplete(out: &mut dyn Write, after_ordinal: u64, error: &str) -> io::Result<()> {
    writeln!(
        out,
        "truncation incomplete after_ordinal={after_ordinal} error={}",
        quote_ascii(error)
    )
}

fn locator(ordinal: u64, event: &TraceEvent) -> String {
    let mut s = format!("ordinal={ordinal}");
    match event {
        TraceEvent::EpisodeStart(e) => {
            let _ = write!(
                s,
                " kind=episode_start episode_id={}",
                quote_ascii(&e.episode_id)
            );
        }
        TraceEvent::Step(e) => {
            let _ = write!(
                s,
                " kind=step episode_id={} step_id={}",
                quote_ascii(&e.episode_id),
                quote_ascii(&e.step_id)
            );
        }
        TraceEvent::ToolCall(e) => {
            let _ = write!(
                s,
                " kind=tool_call episode_id={} step_id={}",
                quote_ascii(&e.episode_id),
                quote_ascii(&e.step_id)
            );
            // Optional on the wire; printed only when present, never as a fabricated 0.
            if let Some(index) = e.call_index {
                let _ = write!(s, " call_index={index}");
            }
        }
        TraceEvent::EpisodeEnd(e) => {
            let _ = write!(
                s,
                " kind=episode_end episode_id={}",
                quote_ascii(&e.episode_id)
            );
        }
    }
    s
}

fn render_reading(reading: &TruncationReading) -> String {
    match reading {
        TruncationReading::Lossy => "reading=lossy".to_string(),
        TruncationReading::Unmeasured => "reading=unmeasured".to_string(),
        TruncationReading::MeasuredClean { stage, ceiling } => format!(
            "reading=measured_clean stage={} ceiling={ceiling}",
            quote_ascii(stage)
        ),
    }
}

/// Quote `s` as a JSON string literal restricted to printable ASCII.
///
/// Identifiers and stage names come from the trace, so they are hostile input:
/// every control character, every non-ASCII scalar (including U+2028/U+2029 and
/// bidirectional controls) becomes a `\uXXXX` escape, with surrogate pairs above
/// U+FFFF. A row therefore stays one line and reads the same in any terminal, and
/// a consumer can recover the original text with any JSON string parser.
pub(crate) fn quote_ascii(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::quote_ascii;

    #[test]
    fn quote_ascii_escapes_everything_outside_printable_ascii_and_round_trips() {
        let cases = [
            "plain",
            "with space",
            "quote\" backslash\\ nl\n cr\r tab\t",
            "\u{7f}\u{1b}[31m",
            "\u{2028}\u{2029}\u{202e}",
            "é🦀",
        ];
        for case in cases {
            let quoted = quote_ascii(case);
            assert!(quoted.is_ascii(), "{quoted}");
            assert!(
                quoted.chars().all(|c| (' '..='~').contains(&c)),
                "no control characters may survive: {quoted:?}"
            );
            let decoded: String = serde_json::from_str(&quoted).expect("JSON string syntax");
            assert_eq!(decoded, case);
        }
        assert_eq!(quote_ascii("é🦀"), "\"\\u00e9\\ud83e\\udd80\"");
    }
}
