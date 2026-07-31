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

// --- Admission, which is a different question from integrity -------------------------------------
//
// The reporting above is a note to an operator. `--require-extent` is a caller stating that it will
// not rely on a bundle whose extent is unknown, and that is where the teeth belong: a verifier
// cannot establish coverage from the artifact, because a producer that wants to omit something can
// decline to carry the declaration too. So the refusal is the consumer's, and integrity is reported
// as passing in the same breath so the two are never confused.

fn verify_requiring_extent(path: &std::path::Path) -> (Option<i32>, String) {
    let output = Command::cargo_bin("assay")
        .expect("binary")
        .args([
            "evidence",
            "verify",
            path.to_str().unwrap(),
            "--require-extent",
        ])
        .output()
        .expect("run assay");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn require_extent_admits_a_completed_declaration() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("complete.tar.gz");
    bundle_from(complete_run(), &path);

    let (code, stderr) = verify_requiring_extent(&path);
    assert_eq!(code, Some(0), "a completed run is admissible: {stderr}");
    assert!(stderr.contains("Liveness: complete"), "{stderr}");
    assert!(!stderr.contains("REFUSED"), "{stderr}");
}

/// The case the flag exists for. This is the scope limitation: the bundle makes no completeness
/// claim, so a caller relying on completeness must not be told "OK" and nothing else.
#[test]
fn require_extent_refuses_an_undeclared_bundle_while_integrity_passes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plain.tar.gz");
    let event = EvidenceEvent::new(
        "assay.tool.call",
        "urn:assay:test",
        "cli-liveness-admission",
        0,
        serde_json::json!({"tool": "read"}),
    );
    bundle_from(vec![event], &path);

    let (code, stderr) = verify_requiring_extent(&path);
    assert_eq!(
        code,
        Some(4),
        "undeclared extent must not be admitted: {stderr}"
    );
    assert!(
        stderr.contains("Bundle verified"),
        "integrity must still be reported as passing: {stderr}"
    );
    assert!(stderr.contains("Admission REFUSED"), "{stderr}");
    assert!(
        stderr.contains("declares no extent"),
        "the refusal must say which question failed: {stderr}"
    );
}

#[test]
fn require_extent_refuses_a_run_that_never_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("truncated.tar.gz");
    bundle_from(truncated_run(), &path);

    let (code, stderr) = verify_requiring_extent(&path);
    assert_eq!(code, Some(4), "an open run must not be admitted: {stderr}");
    assert!(stderr.contains("Liveness: OPEN"), "{stderr}");
    assert!(stderr.contains("Admission REFUSED"), "{stderr}");
}

/// Opting out must leave the previous contract exactly as it was, including the silence on an
/// undeclared bundle. If asking the question changed the answer for callers who did not ask, the
/// separation this flag draws would be fictional.
#[test]
fn without_the_flag_an_undeclared_bundle_is_unchanged() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plain-noflag.tar.gz");
    let event = EvidenceEvent::new(
        "assay.tool.call",
        "urn:assay:test",
        "cli-liveness-noflag",
        0,
        serde_json::json!({"tool": "read"}),
    );
    bundle_from(vec![event], &path);

    let (ok, stderr) = verify(&path);
    assert!(ok, "verify should succeed: {stderr}");
    assert!(!stderr.contains("Liveness:"), "{stderr}");
    assert!(!stderr.contains("REFUSED"), "{stderr}");
}

/// The streaming path cannot answer the question at all, because the bounded verifier does not
/// retain the events. That must refuse rather than admit. An unevaluable policy question coming
/// back as a pass is the one outcome that would make the flag worse than not having it.
#[test]
fn require_extent_refuses_a_streamed_bundle_it_cannot_evaluate() {
    use std::io::Write;
    use std::process::Stdio;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("streamed.tar.gz");
    bundle_from(complete_run(), &path);
    let bytes = std::fs::read(&path).expect("read bundle");

    let mut child = Command::cargo_bin("assay")
        .expect("binary")
        .args(["evidence", "verify", "-", "--require-extent"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn assay");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(&bytes)
        .expect("write bundle to stdin");
    let output = child.wait_with_output().expect("wait");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert_eq!(
        output.status.code(),
        Some(4),
        "a bundle whose extent cannot be evaluated must not be admitted, even though this one is \
         in fact complete: {stderr}"
    );
    assert!(stderr.contains("Bundle verified"), "{stderr}");
    assert!(stderr.contains("cannot be evaluated"), "{stderr}");
}
