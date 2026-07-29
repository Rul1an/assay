//! `assay evidence verify` reports run extent beside integrity, and never on the exit code.
//!
//! The three cases below are the contract. The middle one is the reason the report exists: a
//! truncated bundle passes every integrity check it has, so "OK" alone has never meant "all of it".

use assay_evidence::bundle::BundleWriter;
use assay_evidence::liveness::{LivenessDeclaration, LivenessWriter};
use assay_evidence::types::EvidenceEvent;
use assert_cmd::prelude::*;
use chrono::{DateTime, Duration, TimeZone, Utc};
use std::process::Command;

fn epoch() -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0).unwrap()
}

fn declaration() -> LivenessDeclaration {
    LivenessDeclaration {
        interval_ms: 60_000,
        tolerance_ms: 5_000,
    }
}

fn bundle_from(events: Vec<EvidenceEvent>, path: &std::path::Path) {
    let out = std::fs::File::create(path).expect("create bundle");
    let mut writer = BundleWriter::new(out);
    for event in events {
        writer.add_event(event);
    }
    writer.finish().expect("write bundle");
}

fn parse(sink: &[u8]) -> Vec<EvidenceEvent> {
    std::str::from_utf8(sink)
        .expect("utf8")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse record"))
        .collect()
}

/// A run the writer opened, beat, and closed.
fn complete_run() -> Vec<EvidenceEvent> {
    let mut w = LivenessWriter::open(
        Vec::new(),
        "cli-liveness-complete",
        "urn:assay:test",
        declaration(),
        epoch(),
    )
    .expect("open");
    w.tick(epoch() + Duration::milliseconds(10_000))
        .expect("tick");
    let sink = w
        .close(epoch() + Duration::milliseconds(20_000))
        .expect("close");
    parse(&sink)
}

/// The same producer, killed before it could close.
fn truncated_run() -> Vec<EvidenceEvent> {
    let mut w = LivenessWriter::open(
        Vec::new(),
        "cli-liveness-truncated",
        "urn:assay:test",
        declaration(),
        epoch(),
    )
    .expect("open");
    w.tick(epoch() + Duration::milliseconds(10_000))
        .expect("tick");
    w.emitted().to_vec()
}

fn verify(path: &std::path::Path) -> (bool, String) {
    let output = Command::cargo_bin("assay")
        .expect("binary")
        .args(["evidence", "verify", path.to_str().unwrap()])
        .output()
        .expect("run assay");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn a_complete_run_reports_complete_and_exits_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("complete.tar.gz");
    bundle_from(complete_run(), &path);

    let (ok, stderr) = verify(&path);
    assert!(ok, "verify should succeed: {stderr}");
    assert!(stderr.contains("Bundle verified"), "{stderr}");
    assert!(stderr.contains("Liveness: complete"), "{stderr}");
}

/// The load-bearing case. Integrity still passes, so the liveness line is the only thing telling a
/// reader the tail may be missing, and the exit code deliberately does not change: this is a
/// finding for an operator, not a verification failure.
#[test]
fn a_truncated_run_still_verifies_but_reports_open() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("truncated.tar.gz");
    bundle_from(truncated_run(), &path);

    let (ok, stderr) = verify(&path);
    assert!(
        ok,
        "a truncated bundle is still integral, so verify must not fail: {stderr}"
    );
    assert!(stderr.contains("Bundle verified"), "{stderr}");
    assert!(stderr.contains("Liveness: OPEN"), "{stderr}");
}

/// Bundles that predate liveness declare nothing, so the report stays silent rather than
/// retroactively annotating every existing artifact.
#[test]
fn a_bundle_without_liveness_records_reports_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plain.tar.gz");
    let event = EvidenceEvent::new(
        "assay.tool.call",
        "urn:assay:test",
        "cli-liveness-plain",
        0,
        serde_json::json!({"tool": "read"}),
    );
    bundle_from(vec![event], &path);

    let (ok, stderr) = verify(&path);
    assert!(ok, "verify should succeed: {stderr}");
    assert!(stderr.contains("Bundle verified"), "{stderr}");
    assert!(
        !stderr.contains("Liveness:"),
        "an undeclared bundle must not be annotated: {stderr}"
    );
}
