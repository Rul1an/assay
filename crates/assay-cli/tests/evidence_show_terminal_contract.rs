//! `assay evidence show` prints strings a bundle's producer chose: the producer name and version,
//! event types and subjects. A bundle verifies whatever those strings hold (the writer checks the
//! shape of a run id, and of nothing else here), so the table view must not hand them to the
//! terminal as written.
//!
//! The property is stated on the bytes the command writes, not on a list of known sequences: in
//! the table view stdout holds no control byte except the newline that ends a line, no C1 control
//! and no bidirectional override, and one event is one line. The second half matters as much as
//! the first: a newline inside a field would let a bundle write its own `Verified:` line.
//!
//! `--format json` is data, not a rendering: `serde_json` escapes control characters, and the
//! strings must come back byte for byte.

use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use assert_cmd::Command;
use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::path::Path;
use tempfile::tempdir;

/// Window title, clipboard write (OSC 52), colour, bare BEL, 8-bit CSI, carriage return, DEL, a
/// right-to-left override, and a newline followed by the line the inspector prints itself.
const HOSTILE: &str = "x\x1b]0;TITLE\x07\x1b]52;c;cHduZWQ=\x07\x1b[31mRED\x1b[0m\x07\u{9b}31m\r\x7f\u{202e}\nVerified:    \u{2705} OK\n\u{2705} Verified Integrity\ty";

fn hostile(label: &str) -> String {
    format!("{label}-{HOSTILE}")
}

fn write_bundle(path: &Path, clean: bool) {
    let text = |label: &str| {
        if clean {
            label.to_string()
        } else {
            hostile(label)
        }
    };
    let producer = ProducerMeta::new(text("producer"), text("version"));
    // The one field the writer constrains; every other string below is the producer's to choose.
    let run_id = "run_show_terminal".to_string();
    let mut file = std::fs::File::create(path).expect("create bundle");
    let mut writer = BundleWriter::new(&mut file);
    for seq in 0..2u64 {
        let event = EvidenceEvent::new(
            text("assay.type"),
            "urn:assay:test:show-terminal",
            run_id.clone(),
            seq,
            serde_json::json!({ "seq": seq }),
        )
        .with_producer(&producer)
        .with_subject(text("subject"))
        .with_time(Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap());
        writer.add_event(event);
    }
    writer.finish().expect("finish bundle");
}

fn show(bundle: &Path, extra: &[&str]) -> std::process::Output {
    Command::cargo_bin("assay")
        .expect("assay binary")
        .args(["evidence", "show", bundle.to_str().expect("utf8 path")])
        .args(extra)
        .output()
        .expect("run assay evidence show")
}

fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{061c}')
}

/// The whole contract for a table rendering, on raw bytes and on decoded text.
fn assert_terminal_safe(stdout: &[u8], context: &str) {
    for (index, &byte) in stdout.iter().enumerate() {
        assert!(
            byte >= 0x20 || byte == b'\n',
            "{context}: stdout byte {index} is the control byte {byte:#04x}"
        );
        assert_ne!(byte, 0x7f, "{context}: stdout byte {index} is DEL");
    }
    let text = std::str::from_utf8(stdout).expect("table output is UTF-8");
    for c in text.chars() {
        assert!(
            !('\u{80}'..='\u{9f}').contains(&c),
            "{context}: stdout holds the C1 control U+{:04X}",
            c as u32
        );
        assert!(
            !is_bidi_control(c),
            "{context}: stdout holds the bidirectional control U+{:04X}",
            c as u32
        );
    }
}

fn lines_starting(text: &str, prefix: &str) -> usize {
    text.lines().filter(|line| line.starts_with(prefix)).count()
}

#[test]
fn table_view_of_a_verified_bundle_writes_no_control_sequence() {
    let dir = tempdir().expect("tempdir");
    let hostile_bundle = dir.path().join("hostile.tar.gz");
    let clean_bundle = dir.path().join("clean.tar.gz");
    write_bundle(&hostile_bundle, false);
    write_bundle(&clean_bundle, true);

    // Control: the same bundle shape without hostile strings, to fix the line count.
    let clean = show(&clean_bundle, &[]);
    assert_eq!(clean.status.code(), Some(0), "clean bundle must show");
    let clean_text = String::from_utf8(clean.stdout).expect("utf8");
    let expected_lines = clean_text.lines().count();

    let output = show(&hostile_bundle, &[]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "the hostile bundle verifies, so show must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_terminal_safe(&output.stdout, "show");
    let text = String::from_utf8(output.stdout).expect("utf8");

    // One event is one line, and the inspector's own status lines appear once each.
    assert_eq!(
        text.lines().count(),
        expected_lines,
        "a field must not add lines to the table:\n{text}"
    );
    assert_eq!(
        lines_starting(&text, "Verified:"),
        1,
        "forged status line:\n{text}"
    );
    assert_eq!(
        lines_starting(&text, "\u{2705} Verified Integrity"),
        1,
        "forged footer line:\n{text}"
    );

    // The rendering still shows what the field held: the readable part, and a visible mark where
    // something was removed.
    assert!(text.contains("producer-x"), "readable prefix lost:\n{text}");
    assert!(text.contains("subject-x"), "readable prefix lost:\n{text}");
    assert!(text.contains('\u{fffd}'), "no replacement mark:\n{text}");
}

#[test]
fn table_view_without_verification_is_held_to_the_same_contract() {
    let dir = tempdir().expect("tempdir");
    let bundle = dir.path().join("hostile.tar.gz");
    write_bundle(&bundle, false);

    let output = show(&bundle, &["--no-verify"]);
    assert_eq!(output.status.code(), Some(0));
    assert_terminal_safe(&output.stdout, "show --no-verify");
    let text = String::from_utf8(output.stdout).expect("utf8");
    assert_eq!(
        lines_starting(&text, "Verified:"),
        1,
        "forged status line:\n{text}"
    );
}

#[test]
fn json_view_keeps_every_string_byte_for_byte() {
    let dir = tempdir().expect("tempdir");
    let bundle = dir.path().join("hostile.tar.gz");
    write_bundle(&bundle, false);

    let output = show(&bundle, &["--format", "json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        !output.stdout.contains(&0x1b) && !output.stdout.contains(&0x07),
        "JSON output must carry control characters escaped, not raw"
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("show --format json is JSON");
    let event = &report["events"][0];
    assert_eq!(event["type"], Value::String(hostile("assay.type")));
    assert_eq!(event["subject"], Value::String(hostile("subject")));
    assert_eq!(
        report["manifest"]["producer"]["name"],
        Value::String(hostile("producer")),
        "the JSON view is data and must not be rewritten"
    );
}
