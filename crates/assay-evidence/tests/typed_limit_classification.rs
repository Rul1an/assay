//! ADR-043: a ceiling failure must be classifiable as data, never by reading the message.
//!
//! The verifier used to recognise its own ceilings with `ve.message.contains("LimitBundleBytes")`.
//! That made the rendered text a contract: rewording a diagnostic would silently reclassify a
//! resource refusal as an integrity fault. The cause now travels as a typed
//! `assay_common::limits::LimitExceeded` through the `io::Error`, and the verifier maps it onto its
//! own `ErrorCode`.
//!
//! Every assertion here goes through `downcast_ref::<VerifyError>()` and checks the exact class
//! and code, so a test cannot pass on a coincidentally similar message.

use assay_evidence::bundle::writer::{
    verify_bundle_with_limits, BundleWriter, ErrorClass, ErrorCode, VerifyError, VerifyLimits,
};
use assay_evidence::types::EvidenceEvent;
use std::io::Cursor;

fn bundle(events: u64, pad: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buf);
        for seq in 0..events {
            w.add_event(EvidenceEvent::new(
                "assay.test",
                "urn:assay:test",
                "run_typed",
                seq,
                serde_json::json!({ "pad": "A".repeat(pad) }),
            ));
        }
        w.finish().expect("write bundle");
    }
    buf
}

fn classify(bytes: Vec<u8>, limits: VerifyLimits) -> (ErrorClass, ErrorCode) {
    let err = verify_bundle_with_limits(Cursor::new(bytes), limits)
        .expect_err("the bundle must be refused under these limits");
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("a ceiling refusal must surface as a typed VerifyError");
    (ve.class, ve.code)
}

#[test]
fn the_compressed_ceiling_classifies_as_limit_bundle_bytes() {
    let bytes = bundle(1, 8);
    let limits = VerifyLimits {
        max_bundle_bytes: bytes.len() as u64 - 1,
        ..VerifyLimits::default()
    };
    assert_eq!(
        classify(bytes, limits),
        (ErrorClass::Limits, ErrorCode::LimitBundleBytes)
    );
}

#[test]
fn the_expansion_ceiling_classifies_as_limit_decode_bytes() {
    let bytes = bundle(400, 512);
    let limits = VerifyLimits {
        max_bundle_bytes: bytes.len() as u64 * 8,
        max_decode_bytes: 4096,
        ..VerifyLimits::default()
    };
    assert_eq!(
        classify(bytes, limits),
        (ErrorClass::Limits, ErrorCode::LimitDecodeBytes)
    );
}

/// The acceptance side. A refusal-classifying verifier that refused everything would satisfy the
/// tests above; this one holds it to accepting a bundle that fits.
#[test]
fn a_bundle_within_every_ceiling_is_not_refused() {
    let bytes = bundle(4, 32);
    verify_bundle_with_limits(Cursor::new(bytes), VerifyLimits::default())
        .expect("a small bundle must verify under the default limits");
}
