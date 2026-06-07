//! Regression: the verifier must validate the gzip CRC/ISIZE trailer.
//!
//! The tar reader stops after the expected entries, so without an explicit drain the gzip trailer
//! was never read and a corrupted trailer (or a compressed-stream mutation landing in an unchecked
//! manifest field) could slip through. These tests pin the post-fix behavior.

use assay_evidence::types::EvidenceEvent;
use assay_evidence::{
    verify_bundle_with_limits, BundleWriter, ErrorClass, VerifyError, VerifyLimits,
};
use chrono::{TimeZone, Utc};
use std::io::Cursor;

fn valid_bundle() -> Vec<u8> {
    let mut bundle = Vec::new();
    let mut writer = BundleWriter::new(&mut bundle);
    for seq in 0..3u64 {
        let mut event = EvidenceEvent::new(
            "assay.test",
            "urn:test",
            "run",
            seq,
            serde_json::json!({ "seq": seq }),
        );
        event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
        writer.add_event(event);
    }
    writer.finish().unwrap();
    bundle
}

#[test]
fn valid_bundle_still_verifies() {
    let bundle = valid_bundle();
    verify_bundle_with_limits(Cursor::new(&bundle), VerifyLimits::default())
        .expect("a valid bundle must verify after the trailer drain");
}

#[test]
fn corrupted_gzip_trailer_is_rejected() {
    let mut bundle = valid_bundle();
    let n = bundle.len();
    // gzip trailer = last 8 bytes (CRC32 + ISIZE). Corrupting it leaves the deflate stream intact
    // but invalid at the trailer check; before the drain fix this passed verification.
    for b in &mut bundle[n - 8..] {
        *b ^= 0xFF;
    }
    let err = verify_bundle_with_limits(Cursor::new(&bundle), VerifyLimits::default())
        .expect_err("corrupted gzip trailer must be rejected");
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("expected a typed VerifyError");
    assert_eq!(
        ve.class,
        ErrorClass::Integrity,
        "trailer corruption must classify as Integrity (got {:?})",
        ve.code
    );
}
