//! A session-scope finding survives the real write/verify path, and tampering with it is caught.
//!
//! ADR-047 Decision 1 puts `assay.session.finding` in the event stream rather than in a sibling
//! file, on the argument that the event stream is what bundle verification actually covers. This
//! test is that argument, executed.
//!
//! The first version of it was not. It built an `EvidenceEvent` by hand, called
//! `compute_content_hash` and `compute_run_root` directly, and asserted that changing the outcome
//! changed the root. That passed with the event kind replaced by `assay.totally.unrelated.kind`,
//! and it passed with `Payload::SessionFinding` deleted from the crate — because it tested that a
//! hash function depends on its input, which `crypto::id::tests::test_content_hash_changes_with_payload`
//! already tested and which has nothing to do with this ADR. An adversarial review caught it.
//!
//! What is different here: the bundle is produced by `BundleWriter` and read by
//! `verify_bundle_with_limits`, so a gate anywhere on either path — a kind allowlist in the
//! writer, a payload filter in the reader — fails this test rather than being invisible to it.

use assay_evidence::bundle::{verify_bundle_with_limits, BundleWriter, ErrorClass, VerifyLimits};
use assay_evidence::types::{EvidenceEvent, PayloadSessionFinding};
use serde_json::json;
use std::io::Cursor;

const RUN: &str = "run_session_finding_0001";

fn finding_payload(outcome: &str) -> serde_json::Value {
    json!({
        "rule_id": "after:read_credentials->http_post",
        "kind": "after",
        "outcome": outcome,
        "spanned": [1, 2],
        "extent": "complete",
        "reason": "credential read at 1 followed by egress at 2"
    })
}

/// A bundle whose second event is a session-scope finding, written the way a run would write one.
fn bundle_with_finding(outcome: &str) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buffer);
        w.add_event(EvidenceEvent::new(
            "assay.tool.decision",
            "urn:assay:session-finding",
            RUN,
            0,
            json!({"tool": "read_file", "decision": "allow"}),
        ));
        w.add_event(EvidenceEvent::new(
            "assay.session.finding",
            "urn:assay:session-finding",
            RUN,
            1,
            finding_payload(outcome),
        ));
        w.finish()
            .expect("the writer emits a bundle carrying a session finding");
    }
    buffer
}

#[test]
fn a_session_finding_round_trips_through_the_writer_and_the_verifier() {
    let bytes = bundle_with_finding("violated");
    let result = verify_bundle_with_limits(bytes.as_slice(), VerifyLimits::default())
        .expect("a bundle carrying a session finding verifies");
    assert_eq!(result.event_count, 2, "both events survived the round trip");

    // The payload is readable as a typed record rather than as loose JSON, which is what
    // `PayloadSessionFinding` adds. It is parsed directly rather than through `Payload`: wiring the
    // variant into that enum is a semver major (see ADR-047), so the struct ships first and the
    // convenience view follows. A consumer reading `EvidenceEvent::payload` -- a raw `Value` --
    // does exactly this.
    let f: PayloadSessionFinding = serde_json::from_value(finding_payload("violated"))
        .expect("the written payload parses as the typed record");
    assert_eq!(f.spanned, vec![1, 2]);
    assert_eq!(f.outcome, "violated");
}

/// Change the finding after the fact and verification refuses the bundle.
///
/// This is the property Decision 1 rests on: a finding in the event stream is covered by the same
/// integrity chain as the calls it spans. The edit below is a plain byte substitution inside
/// `events.ndjson`, exactly what an attacker with the archive would do -- no hashes recomputed, no
/// manifest touched.
#[test]
fn editing_a_session_finding_after_the_fact_fails_verification() {
    let bytes = bundle_with_finding("violated");

    // Rewrite the gzip'd tar with `violated` replaced by `notheld` inside events.ndjson. Same
    // length, so nothing but the finding's own bytes changes.
    let tampered = retar_with_substitution(&bytes, b"\"violated\"", b"\"notheld_\"");
    assert_ne!(
        tampered, bytes,
        "the substitution must actually have applied"
    );

    let err = verify_bundle_with_limits(tampered.as_slice(), VerifyLimits::default())
        .expect_err("an edited finding must not verify");
    let text = format!("{err}");
    assert!(
        text.starts_with(&format!("{:?}", ErrorClass::Integrity)),
        "an edited finding is an integrity failure, not a contract one: {text}"
    );
}

/// Unpack, substitute inside `events.ndjson`, repack. The manifest is deliberately left stale --
/// that is the point of the test.
fn retar_with_substitution(bundle: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    use flate2::read::GzDecoder;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Read;

    assert_eq!(
        from.len(),
        to.len(),
        "same-length substitution keeps sizes honest"
    );

    let mut tar_bytes = Vec::new();
    GzDecoder::new(Cursor::new(bundle))
        .read_to_end(&mut tar_bytes)
        .expect("bundle is gzip");

    let mut out = Vec::new();
    {
        let enc = GzEncoder::new(&mut out, Compression::default());
        let mut builder = tar::Builder::new(enc);
        let mut archive = tar::Archive::new(Cursor::new(&tar_bytes));
        for entry in archive.entries().expect("tar entries") {
            let mut entry = entry.expect("entry");
            let path = entry.path().expect("path").into_owned();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).expect("entry bytes");
            if path.to_string_lossy() == "events.ndjson" {
                data = replace_bytes(&data, from, to);
            }
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, &path, Cursor::new(data))
                .expect("append");
        }
        builder.into_inner().expect("tar").finish().expect("gzip");
    }
    out
}

fn replace_bytes(haystack: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(haystack.len());
    let mut i = 0;
    while i < haystack.len() {
        if haystack[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(haystack[i]);
            i += 1;
        }
    }
    out
}
