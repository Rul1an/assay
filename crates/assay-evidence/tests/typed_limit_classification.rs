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

/// `max_json_depth` was configurable and had no effect: the validator's depth guard was pinned to
/// a constant, so the field never reached it. And when the constant did trip, the failure was
/// reported as `ContractInvalidJson`, which says the document is malformed when it means the
/// document is deeper than we agreed to parse.
#[test]
fn a_nesting_refusal_classifies_as_limit_json_depth() {
    let mut buf = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buf);
        // Nested well past a small configured ceiling, far under the built-in constant.
        let mut payload = serde_json::json!({"leaf": 1});
        for _ in 0..12 {
            payload = serde_json::json!({ "n": payload });
        }
        w.add_event(EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_depth",
            0,
            payload,
        ));
        w.finish().expect("write bundle");
    }

    let limits = VerifyLimits {
        max_json_depth: 4,
        ..VerifyLimits::default()
    };
    assert_eq!(
        classify(buf.clone(), limits),
        (ErrorClass::Limits, ErrorCode::LimitJsonDepth)
    );

    // Acceptance twin: the same bundle under a ceiling that accommodates it. Without this, a
    // validator that rejected all nesting would satisfy the assertion above.
    verify_bundle_with_limits(Cursor::new(buf), VerifyLimits::default())
        .expect("the same bundle must verify under the default depth");
}

/// Object and array nesting are separate code paths in the validator, and only the object one had
/// been switched to the caller's ceiling. An array-only document therefore kept using the module
/// constant and ignored a configured depth entirely.
#[test]
fn the_ceiling_applies_to_object_array_and_mixed_nesting() {
    fn bundle_with(payload: serde_json::Value) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = BundleWriter::new(&mut buf);
            w.add_event(EvidenceEvent::new(
                "assay.test",
                "urn:assay:test",
                "run_depth_shape",
                0,
                payload,
            ));
            w.finish().expect("write bundle");
        }
        buf
    }

    let mut objects = serde_json::json!({"leaf": 1});
    let mut arrays = serde_json::json!([1]);
    let mut mixed = serde_json::json!({"leaf": 1});
    for i in 0..12 {
        objects = serde_json::json!({ "n": objects });
        arrays = serde_json::json!([arrays]);
        mixed = if i % 2 == 0 {
            serde_json::json!([mixed])
        } else {
            serde_json::json!({ "n": mixed })
        };
    }

    let tight = VerifyLimits {
        max_json_depth: 4,
        ..VerifyLimits::default()
    };
    for (shape, payload) in [("objects", objects), ("arrays", arrays), ("mixed", mixed)] {
        let bytes = bundle_with(payload);
        assert_eq!(
            classify(bytes.clone(), tight),
            (ErrorClass::Limits, ErrorCode::LimitJsonDepth),
            "{shape} nesting must respect the configured ceiling"
        );
        verify_bundle_with_limits(Cursor::new(bytes), VerifyLimits::default())
            .unwrap_or_else(|e| panic!("{shape} must verify under the default depth: {e:#}"));
    }
}

/// The manifest is a document like any other, and it was the one the ceiling never reached. The
/// verifier handed it straight to `serde_json` with no strict pass at all, and peek validated it
/// against the module constant, so an unverified read applied a different budget than a verified
/// one on the same bytes.
#[test]
fn the_manifest_respects_the_configured_ceiling_on_both_paths() {
    use assay_evidence::bundle::BundleReader;

    // A manifest is written by us, so it is not deeply nested. A ceiling of 1 is below even a
    // flat object, which is what makes this observable without hand-forging a bundle.
    let mut buf = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buf);
        w.add_event(EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_manifest_depth",
            0,
            serde_json::json!({"flat": 1}),
        ));
        w.finish().expect("write bundle");
    }

    let tight = VerifyLimits {
        max_json_depth: 1,
        ..VerifyLimits::default()
    };

    assert_eq!(
        classify(buf.clone(), tight),
        (ErrorClass::Limits, ErrorCode::LimitJsonDepth),
        "the verifier must apply the ceiling to manifest.json"
    );

    let err = match BundleReader::open_unverified_with_limits(Cursor::new(buf.clone()), tight) {
        Ok(_) => panic!("the unverified path must apply the ceiling to manifest.json too"),
        Err(e) => e,
    };
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("a manifest depth refusal must be typed on the unverified path as well");
    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitJsonDepth);

    // Acceptance twin on both paths, so a ceiling that refused every manifest would not pass.
    verify_bundle_with_limits(Cursor::new(buf.clone()), VerifyLimits::default())
        .expect("the manifest must verify under the default depth");
    BundleReader::open_unverified_with_limits(Cursor::new(buf), VerifyLimits::default())
        .expect("and must open unverified under the default depth");
}

/// The classification was right while the sentence was wrong. `NestingTooDeep` renders its own
/// Display, which names the module constant as the maximum and echoes the depth the input chose,
/// so a refusal under a configured ceiling of four told the operator it had exceeded 64. One
/// shared classifier now writes the message from the ceiling that actually applied, and does not
/// repeat an attacker-supplied number back at the reader.
#[test]
fn the_message_names_the_configured_ceiling_and_not_the_constant() {
    let mut buf = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buf);
        let mut payload = serde_json::json!({"leaf": 1});
        for _ in 0..12 {
            payload = serde_json::json!({ "n": payload });
        }
        w.add_event(EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_depth_message",
            0,
            payload,
        ));
        w.finish().expect("write bundle");
    }

    let limits = VerifyLimits {
        max_json_depth: 4,
        ..VerifyLimits::default()
    };
    let err = verify_bundle_with_limits(Cursor::new(buf), limits).expect_err("must be refused");
    let ve = err.downcast_ref::<VerifyError>().expect("typed");

    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitJsonDepth);
    assert!(
        ve.message.contains("maximum depth of 4"),
        "the message must name the ceiling that applied: {}",
        ve.message
    );
    assert!(
        !ve.message.contains("64"),
        "the module constant must not be presented as the maximum: {}",
        ve.message
    );
    assert!(
        !ve.message.contains("13"),
        "the observed depth is attacker-chosen and must not be echoed: {}",
        ve.message
    );
    assert!(
        !ve.message.contains("Security"),
        "a budget refusal must not be framed as a security finding: {}",
        ve.message
    );
}

/// `max_path_len`, `max_line_bytes` and `max_events` were enforced on the verifying path and
/// nowhere else, so an unverified read applied a different contract to the same bytes. Skipping
/// verification means skipping the integrity check, not the resource budget.
#[test]
fn the_unverified_path_enforces_line_and_event_ceilings() {
    use assay_evidence::bundle::BundleReader;

    let mut buf = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buf);
        for seq in 0..6 {
            w.add_event(EvidenceEvent::new(
                "assay.test",
                "urn:assay:test",
                "run_shape",
                seq,
                serde_json::json!({ "pad": "A".repeat(256) }),
            ));
        }
        w.finish().expect("write bundle");
    }

    fn refuse_unverified(bytes: &[u8], limits: VerifyLimits) -> (ErrorClass, ErrorCode) {
        let err =
            match BundleReader::open_unverified_with_limits(Cursor::new(bytes.to_vec()), limits) {
                Ok(_) => panic!("the unverified path must apply this ceiling"),
                Err(e) => e,
            };
        let ve = err
            .downcast_ref::<VerifyError>()
            .expect("an unverified ceiling refusal must be typed");
        (ve.class, ve.code)
    }

    assert_eq!(
        refuse_unverified(
            &buf,
            VerifyLimits {
                max_line_bytes: 64,
                ..VerifyLimits::default()
            }
        ),
        (ErrorClass::Limits, ErrorCode::LimitLineBytes)
    );

    assert_eq!(
        refuse_unverified(
            &buf,
            VerifyLimits {
                max_events: 2,
                ..VerifyLimits::default()
            }
        ),
        (ErrorClass::Limits, ErrorCode::LimitTotalEvents)
    );

    assert_eq!(
        refuse_unverified(
            &buf,
            VerifyLimits {
                max_path_len: 4,
                ..VerifyLimits::default()
            }
        ),
        (ErrorClass::Limits, ErrorCode::LimitPathLength)
    );

    // Acceptance twin: the same bundle under the defaults, so a reader that refused everything
    // would not pass.
    BundleReader::open_unverified_with_limits(Cursor::new(buf), VerifyLimits::default())
        .expect("the same bundle must open unverified under the default limits");
}

/// The verified and unverified paths shared a name for `max_line_bytes` and disagreed on what it
/// meant. `read_line_bounded` counted the trailing `\n` inside the ceiling, so a payload of
/// exactly the limit consumed limit + 1 bytes and was refused; `check_events_shape` split on `\n`
/// first, so the same payload passed. One budget, two answers. Pinning one interpretation and
/// requiring the same verdict on both paths is the only way `max_line_bytes` can be a contract.
#[test]
fn max_line_bytes_has_one_interpretation_on_both_paths() {
    use assay_evidence::bundle::BundleReader;

    fn bundle_with_padded_event(pad: usize) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = BundleWriter::new(&mut buf);
            w.add_event(EvidenceEvent::new(
                "assay.test",
                "urn:assay:test",
                "run_line_bytes",
                0,
                serde_json::json!({ "pad": "A".repeat(pad) }),
            ));
            w.finish().expect("write bundle");
        }
        buf
    }

    // A padded event serializes to a JSON line whose length grows with `pad`. Pick a small
    // headroom over the padding so we can lock a `max_line_bytes` right at the line length.
    let pad = 200;
    let bytes = bundle_with_padded_event(pad);
    let line_len = {
        // Peek at the emitted line length through the default verifier, which does not refuse
        // this bundle under the defaults. We only need the true length to build the ±1 tests.
        let default_open = BundleReader::open(Cursor::new(bytes.clone()))
            .expect("default open must accept the fixture");
        default_open
            .events_raw()
            .split(|b| *b == b'\n')
            .next()
            .map(<[u8]>::len)
            .unwrap_or(0)
    };
    assert!(
        line_len > 0,
        "the fixture must emit at least one non-empty line"
    );

    let exact = VerifyLimits {
        max_line_bytes: line_len,
        ..VerifyLimits::default()
    };
    let too_tight = VerifyLimits {
        max_line_bytes: line_len - 1,
        ..VerifyLimits::default()
    };

    // Exact-limit passes on both paths.
    verify_bundle_with_limits(Cursor::new(bytes.clone()), exact)
        .expect("verified path must accept a line of exactly max_line_bytes");
    BundleReader::open_unverified_with_limits(Cursor::new(bytes.clone()), exact)
        .expect("unverified path must accept the same line");

    // One byte tighter refuses on both, with the same class and code.
    assert_eq!(
        classify(bytes.clone(), too_tight),
        (ErrorClass::Limits, ErrorCode::LimitLineBytes),
        "verified path must classify the overflow as LimitLineBytes"
    );
    let err = match BundleReader::open_unverified_with_limits(Cursor::new(bytes), too_tight) {
        Ok(_) => panic!("unverified path must refuse the same line under the tighter limit"),
        Err(e) => e,
    };
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("the unverified overflow must surface as a typed VerifyError");
    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitLineBytes);
}
