//! ADR-043 §1 for the two CLI ingest entrypoints, at the layer only the shipped binary
//! exercises.
//!
//! `assay evidence verify -` used to `read_to_end` stdin and verify a `Cursor` over the
//! result. `assay evidence push` read the file whole and cloned the buffer for the upload,
//! and `--no-verify` skipped even the after-the-fact check. Both defects are wiring, not
//! logic, which is why they need contract tests over the binary rather than unit tests over
//! a helper.
//!
//! The fixture is a real bundle whose sole event contains a payload deliberately larger than
//! the default `max_line_bytes` (1 MiB). Well-formed enough for the tar walker to reach the
//! ceiling; small enough compressed to keep the test fast. A regression that dropped the
//! per-line ceiling on either entrypoint would parse this and either upload it or accept it
//! on stdin; today both paths refuse with a `LimitLineBytes` refusal.
//!
//! `--no-verify` gets its own arm, because the audit specifically called it out as the mode
//! that historically skipped the check.
//!
//! Restricted to `unix` because `Command::write_stdin` requires a piped stdin the invoked
//! process actually reads, matching the existing sandbox integration tests.

#![cfg(unix)]

use assay_evidence::bundle::writer::BundleWriter;
use assay_evidence::types::EvidenceEvent;
use assert_cmd::Command;
use std::io::Write;
use tempfile::NamedTempFile;

/// A real bundle with one event whose payload comfortably exceeds the default per-line
/// ceiling. The payload is a run of `A`s, so it compresses to a small tar.gz that still
/// unpacks to a legit archive the tar walker can traverse until it hits the ceiling on the
/// events file.
fn bundle_with_oversized_line() -> Vec<u8> {
    // Default `max_line_bytes` is 1 MiB; 2 MiB gives a comfortable margin so the test is not
    // sitting on the exact boundary.
    let pad = "A".repeat(2 * 1024 * 1024);
    let mut buffer = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buffer);
        w.add_event(EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_bounded_cli",
            0,
            serde_json::json!({ "pad": pad }),
        ));
        w.finish().expect("write bundle");
    }
    buffer
}

/// The refusal has to name a ceiling, not a parse failure. Otherwise a regression that
/// happens to also fail on some content check would satisfy the assertion. The exact code is
/// `LimitLineBytes`, and it is spelled with that class prefix in the rendered `VerifyError`.
fn assert_named_a_line_ceiling(stderr: &str) {
    let hay = stderr.to_lowercase();
    assert!(
        hay.contains("limitlinebytes"),
        "refusal must be classified as LimitLineBytes; got:\n{stderr}"
    );
}

#[test]
fn stdin_verify_refuses_a_bundle_whose_line_exceeds_the_ceiling() {
    let payload = bundle_with_oversized_line();

    let out = Command::cargo_bin("assay")
        .expect("binary")
        .args(["evidence", "verify", "-"])
        .write_stdin(payload)
        .assert()
        .get_output()
        .clone();

    assert_ne!(
        out.status.code(),
        Some(0),
        "an oversized-line bundle must not verify on stdin"
    );
    assert_named_a_line_ceiling(&String::from_utf8_lossy(&out.stderr));
}

#[test]
fn push_refuses_a_bundle_whose_line_exceeds_the_ceiling_in_verify_mode() {
    let mut f = NamedTempFile::new().expect("temp bundle");
    f.write_all(&bundle_with_oversized_line()).expect("write");
    f.flush().expect("flush");

    let out = Command::cargo_bin("assay")
        .expect("binary")
        .args(["evidence", "push"])
        .arg(f.path())
        // No `--store`: the run must not reach the point of needing one.
        .assert()
        .get_output()
        .clone();

    assert_ne!(
        out.status.code(),
        Some(0),
        "verify-mode push must refuse an oversized-line bundle before upload"
    );
    assert_named_a_line_ceiling(&String::from_utf8_lossy(&out.stderr));
}

/// The audit named `--no-verify` as the mode that historically skipped resource checks. It
/// must still refuse this fixture, on the per-line ceiling that the pre-scan applies before
/// returning `events_content`.
#[test]
fn push_no_verify_still_refuses_a_bundle_whose_line_exceeds_the_ceiling() {
    let mut f = NamedTempFile::new().expect("temp bundle");
    f.write_all(&bundle_with_oversized_line()).expect("write");
    f.flush().expect("flush");

    let out = Command::cargo_bin("assay")
        .expect("binary")
        .args(["evidence", "push", "--no-verify"])
        .arg(f.path())
        .assert()
        .get_output()
        .clone();

    assert_ne!(
        out.status.code(),
        Some(0),
        "--no-verify push must refuse an oversized-line bundle before upload"
    );
    assert_named_a_line_ceiling(&String::from_utf8_lossy(&out.stderr));
}
