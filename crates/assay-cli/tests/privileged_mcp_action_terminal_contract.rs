//! `assay evidence verify-privileged-mcp-action` prints findings in its table, and two of them name
//! an event by its id. The id is the producer's `run_id:seq`: it sits outside the per-event content
//! hash, and the verifier refuses only a colon in a `run_id`, so a bundle whose `run_id` carries
//! escape sequences verifies once its manifest and events are rewritten together. The table must
//! not hand that id to the terminal as written. `--format json` is data and keeps it byte for byte.

use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use assert_cmd::Command;
use chrono::{TimeZone, Utc};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use tempfile::tempdir;

const CLEAN_RUN_ID: &str = "run_privileged_terminal";
/// Colour, a clipboard write (OSC 52), a bell, an 8-bit CSI, a right-to-left override, and a newline
/// followed by a line the table prints itself. No colon: the verifier refuses one in a run_id.
const HOSTILE_RUN_ID: &str =
    "run\u{1b}[31mRED\u{1b}]52;c;cHduZWQ=\u{7}\u{9b}31m\u{202e}\nBundle integrity- forged";

/// One event whose type differs from the profile schema its payload declares, so the verifier
/// reports `event_type_schema_mismatch` and names the event by its id.
fn write_bundle(path: &Path) {
    let producer = ProducerMeta::new("producer", "version");
    let mut file = std::fs::File::create(path).expect("create bundle");
    let mut writer = BundleWriter::new(&mut file);
    let event = EvidenceEvent::new(
        "assay.test.not_the_schema",
        "urn:assay:test:privileged-terminal",
        CLEAN_RUN_ID.to_string(),
        0,
        serde_json::json!({ "schema": "assay.enforcement_decision.v0" }),
    )
    .with_producer(&producer)
    .with_time(Utc.timestamp_opt(1_700_000_000, 0).unwrap());
    writer.add_event(event);
    writer.finish().expect("finish bundle");
}

fn members(bundle: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = tar::Archive::new(GzDecoder::new(bundle));
    archive
        .entries()
        .expect("entries")
        .map(|entry| {
            let mut entry = entry.expect("entry");
            let path = entry.path().expect("path").to_string_lossy().into_owned();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).expect("read member");
            (path, content)
        })
        .collect()
}

fn repack(members: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        for (path, content) in members {
            let mut header = tar::Header::new_gnu();
            header.set_path(path).expect("set_path");
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_cksum();
            builder.append(&header, content.as_slice()).expect("append");
        }
        builder.finish().expect("finish tar");
    }
    encoder.finish().expect("finish gzip")
}

/// The consistent rewrite the verifier documents as out of its reach: the run_id in the manifest
/// and in every event, each event id with it, and the manifest's record of the events file.
fn with_run_id(bundle: &[u8], run_id: &str) -> Vec<u8> {
    let mut parts = members(bundle);
    let mut events_out = Vec::new();
    for (path, content) in parts.iter_mut() {
        if path == "events.ndjson" {
            let text = String::from_utf8(content.clone()).expect("events utf-8");
            let mut out = String::new();
            for line in text.lines() {
                let mut event: Value = serde_json::from_str(line).expect("event json");
                let seq = event["assayseq"].as_u64().expect("assayseq");
                event["assayrunid"] = Value::String(run_id.to_string());
                event["id"] = Value::String(format!("{run_id}:{seq}"));
                out.push_str(&serde_json::to_string(&event).expect("event"));
                out.push('\n');
            }
            *content = out.into_bytes();
            events_out = content.clone();
        }
    }
    for (path, content) in parts.iter_mut() {
        if path == "manifest.json" {
            let mut manifest: Value = serde_json::from_slice(content).expect("manifest json");
            manifest["run_id"] = Value::String(run_id.to_string());
            manifest["files"]["events.ndjson"]["sha256"] = Value::String(format!(
                "sha256:{}",
                hex::encode(Sha256::digest(&events_out))
            ));
            manifest["files"]["events.ndjson"]["bytes"] = Value::from(events_out.len());
            *content = serde_json::to_vec(&manifest).expect("manifest");
        }
    }
    repack(&parts)
}

fn verify(bundle: &Path, format: &str) -> std::process::Output {
    Command::cargo_bin("assay")
        .expect("assay binary")
        .args(["evidence", "verify-privileged-mcp-action"])
        .arg(bundle)
        .args(["--format", format])
        .output()
        .expect("run verify-privileged-mcp-action")
}

fn assert_terminal_safe(stream: &[u8], context: &str) {
    let text = String::from_utf8_lossy(stream);
    for (index, &byte) in stream.iter().enumerate() {
        assert!(
            (byte >= 0x20 && byte != 0x7f) || byte == b'\n',
            "{context}: byte {index} is the control byte {byte:#04x}:\n{text}"
        );
    }
    for c in text.chars() {
        assert!(
            !('\u{80}'..='\u{9f}').contains(&c)
                && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'),
            "{context}: U+{:04X} reached the terminal:\n{text}",
            c as u32
        );
    }
    assert!(
        !text
            .lines()
            .any(|line| line.starts_with("Bundle integrity- forged")),
        "{context}: the bundle wrote a line of its own:\n{text}"
    );
}

#[test]
fn the_table_cannot_write_to_the_terminal_through_an_event_id() {
    let dir = tempdir().expect("tempdir");
    let plain = dir.path().join("plain.tar.gz");
    write_bundle(&plain);
    let written = std::fs::read(&plain).expect("read bundle");

    // Control: the same rewrite with a harmless run_id still verifies and reaches the finding that
    // names the event, so the hostile case is known to reach that line.
    let control = dir.path().join("control.tar.gz");
    std::fs::write(&control, with_run_id(&written, "run_renamed_control")).expect("write");
    let out = verify(&control, "table");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        text.contains("Bundle integrity: pass")
            && text.contains("event_type_schema_mismatch")
            && text.contains("run_renamed_control:0"),
        "control: the rewritten bundle did not verify and name the event:\n{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let hostile = dir.path().join("hostile.tar.gz");
    std::fs::write(&hostile, with_run_id(&written, HOSTILE_RUN_ID)).expect("write");
    let out = verify(&hostile, "table");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        text.contains("Bundle integrity: pass") && text.contains("event_type_schema_mismatch"),
        "the hostile bundle did not reach the finding:\n{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_terminal_safe(&out.stdout, "table stdout");
    assert_terminal_safe(&out.stderr, "table stderr");

    // The data form carries the id unchanged.
    let json: Value =
        serde_json::from_slice(&verify(&hostile, "json").stdout).expect("the data form is JSON");
    let details: Vec<&str> = json["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|f| f["detail"].as_str())
        .collect();
    assert!(
        details
            .iter()
            .any(|d| d.contains(&format!("{HOSTILE_RUN_ID}:0"))),
        "the JSON form changed the event id: {details:?}"
    );
}
