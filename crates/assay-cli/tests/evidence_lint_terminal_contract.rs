//! `assay evidence lint` reads a bundle it may have just received and prints findings about it.
//! Several findings quote the bundle: rule ASSAY-I001 names an event's `source`, and the header
//! names the bundle id. A bundle verifies whatever those strings hold (`source` is outside the
//! content hash and needs only a colon), so the text report must not hand them to the terminal as
//! written. `tests/evidence_show_terminal_contract.rs` pins the same for `evidence show`.
//!
//! The property is stated on the bytes the command writes, not on a list of known sequences: in
//! the text report stdout and stderr hold no control byte except the newline that ends a line, no
//! C1 control and no bidirectional control, and a string from the bundle cannot add a line.
//!
//! `--format json` and `--format sarif` are data, not a rendering: `serde_json` escapes control
//! characters, and the quoted string must come back byte for byte.

use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use assert_cmd::Command;
use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::path::Path;
use tempfile::tempdir;

/// Window title, clipboard write (OSC 52), colour, bare BEL, 8-bit CSI, carriage return, DEL, a
/// right-to-left override, and a newline followed by a line the report prints itself.
const HOSTILE: &str = "\x1b]0;TITLE\x07\x1b]52;c;cHduZWQ=\x07\x1b[31mRED\x1b[0m\x07\u{9b}31m\r\x7f\u{202e}\nSummary: 0 total (0 errors, 0 warnings, 0 info)\ty";

/// A source that breaks the urn:/https:// convention, so ASSAY-I001 fires and quotes it.
fn source(clean: bool) -> String {
    if clean {
        "x:lint-terminal".to_string()
    } else {
        format!("x:lint-terminal-{HOSTILE}")
    }
}

fn write_bundle(path: &Path, clean: bool) {
    let producer = ProducerMeta::new("producer", "version");
    let mut file = std::fs::File::create(path).expect("create bundle");
    let mut writer = BundleWriter::new(&mut file);
    for seq in 0..2u64 {
        let event = EvidenceEvent::new(
            "assay.test.lint_terminal",
            source(clean),
            "run_lint_terminal".to_string(),
            seq,
            serde_json::json!({ "seq": seq }),
        )
        .with_producer(&producer)
        .with_time(Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap());
        writer.add_event(event);
    }
    writer.finish().expect("finish bundle");
}

fn lint(bundle: &Path, extra: &[&str]) -> std::process::Output {
    Command::cargo_bin("assay")
        .expect("assay binary")
        .args(["evidence", "lint", bundle.to_str().expect("utf8 path")])
        .args(extra)
        .output()
        .expect("run assay evidence lint")
}

fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{061c}')
}

fn assert_terminal_safe(stream: &[u8], context: &str) {
    for (index, &byte) in stream.iter().enumerate() {
        assert!(
            byte >= 0x20 || byte == b'\n',
            "{context}: byte {index} is the control byte {byte:#04x}:\n{}",
            String::from_utf8_lossy(stream)
        );
        assert_ne!(byte, 0x7f, "{context}: byte {index} is DEL");
    }
    let text = std::str::from_utf8(stream).expect("the text report is UTF-8");
    for c in text.chars() {
        assert!(
            !('\u{80}'..='\u{9f}').contains(&c),
            "{context}: the C1 control U+{:04X} reached the terminal:\n{text}",
            c as u32
        );
        assert!(
            !is_bidi_control(c),
            "{context}: the bidirectional control U+{:04X} reached the terminal:\n{text}",
            c as u32
        );
    }
}

fn summary_lines(stream: &[u8]) -> usize {
    String::from_utf8_lossy(stream)
        .lines()
        .filter(|line| line.starts_with("Summary:"))
        .count()
}

#[test]
fn the_text_report_cannot_write_to_the_terminal_through_a_quoted_source() {
    let dir = tempdir().expect("tempdir");
    let hostile = dir.path().join("hostile.tar.gz");
    let clean = dir.path().join("clean.tar.gz");
    write_bundle(&hostile, false);
    write_bundle(&clean, true);

    // Control: the clean bundle reaches the same finding, so the hostile case is known to reach
    // the line that quotes the source.
    let control = lint(&clean, &[]);
    let control_err = String::from_utf8_lossy(&control.stderr).into_owned();
    assert!(
        control_err.contains("ASSAY-I001") && control_err.contains("x:lint-terminal"),
        "control: ASSAY-I001 did not quote the source:\n{control_err}"
    );
    assert_eq!(summary_lines(&control.stderr), 1, "control: {control_err}");

    let out = lint(&hostile, &[]);
    assert_eq!(
        out.status.code(),
        control.status.code(),
        "the hostile bundle must lint like the clean one"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("ASSAY-I001"),
        "the hostile bundle did not reach ASSAY-I001"
    );
    assert_terminal_safe(&out.stdout, "lint stdout");
    assert_terminal_safe(&out.stderr, "lint stderr");
    assert_eq!(
        summary_lines(&out.stderr),
        1,
        "the bundle wrote a Summary line of its own:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn json_and_sarif_carry_the_quoted_source_unchanged() {
    let dir = tempdir().expect("tempdir");
    let hostile = dir.path().join("hostile.tar.gz");
    write_bundle(&hostile, false);
    let quoted = source(false);

    for format in ["json", "sarif"] {
        let out = lint(&hostile, &["--format", format]);
        assert!(
            !out.stdout.contains(&0x1b),
            "{format}: a raw escape byte in the data form"
        );
        let doc: Value = serde_json::from_slice(&out.stdout).expect("the data form is JSON");
        let text = doc.to_string();
        let messages: Vec<String> = find_strings(&doc)
            .into_iter()
            .filter(|s| s.contains("does not follow urn:"))
            .collect();
        assert!(
            !messages.is_empty(),
            "{format}: no ASSAY-I001 message in {text}"
        );
        assert!(
            messages.iter().all(|m| m.contains(&quoted)),
            "{format}: the source did not come back byte for byte: {messages:?}"
        );
    }
}

fn find_strings(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items.iter().flat_map(find_strings).collect(),
        Value::Object(map) => map.values().flat_map(find_strings).collect(),
        _ => Vec::new(),
    }
}
