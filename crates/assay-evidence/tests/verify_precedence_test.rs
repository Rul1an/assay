//! The verifier stops at the first fault, so a bundle carrying several at once
//! reports exactly one code. Which one is part of the contract: a second
//! implementation that rejects the same bundle under a different code has not
//! agreed with this one, it has only also said no.
//!
//! These tests pin the order documented on `verify_bundle`. A change here is a
//! wire change for every consumer, not a refactor.

use assay_evidence::bundle::writer::{
    verify_bundle, verify_bundle_with_limits, BundleWriter, ErrorClass, ErrorCode, VerifyError,
    VerifyLimits,
};
use assay_evidence::types::EvidenceEvent;
use chrono::{TimeZone, Utc};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Cursor, Write};

/// Build a single-entry `.tar.gz`.
///
/// The name goes straight into the header bytes because `tar::Builder` refuses
/// to serialise `..` or absolute paths, which are exactly the shapes under test.
fn hostile_tar_gz(path: &str, body: &[u8]) -> Vec<u8> {
    let mut tar_buf = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_buf);
        let mut header = tar::Header::new_gnu();
        {
            let gnu = header.as_gnu_mut().expect("gnu header");
            let bytes = path.as_bytes();
            assert!(bytes.len() < gnu.name.len(), "path too long for fixture");
            gnu.name[..bytes.len()].copy_from_slice(bytes);
            for slot in gnu.name[bytes.len()..].iter_mut() {
                *slot = 0;
            }
        }
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, body).expect("append entry");
        builder.finish().expect("finish tar");
    }
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&tar_buf).expect("gzip write");
    gz.finish().expect("gzip finish")
}

fn verdict(bundle: &[u8], limits: VerifyLimits) -> (ErrorClass, ErrorCode) {
    let Err(err) = verify_bundle_with_limits(Cursor::new(bundle), limits) else {
        panic!("bundle must be rejected");
    };
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("rejection must carry a VerifyError");
    (ve.class, ve.code)
}

/// A tightened limit must not hide a security finding it shares an entry with.
///
/// This is the regression that motivated the ordering: the same hostile bundle
/// used to report `LimitFileSize` or `LimitPathLength` purely because the
/// operator had hardened a limit, turning an attack into a size complaint.
#[test]
fn path_safety_outranks_every_resource_limit() {
    let traversing_and_oversized = hostile_tar_gz("../evil.json", &[b'x'; 100]);

    let configs = [
        ("default", VerifyLimits::default()),
        (
            "tight events size",
            VerifyLimits {
                max_events_bytes: 10,
                ..VerifyLimits::default()
            },
        ),
        (
            "tight path length",
            VerifyLimits {
                max_path_len: 5,
                ..VerifyLimits::default()
            },
        ),
        (
            "tight both",
            VerifyLimits {
                max_events_bytes: 10,
                max_path_len: 5,
                ..VerifyLimits::default()
            },
        ),
    ];

    for (name, limits) in configs {
        assert_eq!(
            verdict(&traversing_and_oversized, limits),
            (ErrorClass::Security, ErrorCode::SecurityPathTraversal),
            "limit configuration '{name}' must not mask the traversal"
        );
    }
}

/// Absolute paths get their own code rather than being folded into traversal.
#[test]
fn absolute_path_is_distinguished_from_traversal() {
    assert_eq!(
        verdict(
            &hostile_tar_gz("/etc/passwd", b"{}"),
            VerifyLimits::default()
        ),
        (ErrorClass::Security, ErrorCode::SecurityAbsolutePath),
    );

    assert_eq!(
        verdict(
            &hostile_tar_gz("../evil.json", b"{}"),
            VerifyLimits::default()
        ),
        (ErrorClass::Security, ErrorCode::SecurityPathTraversal),
    );
}

/// Contract faults are ranked below security but above content integrity.
#[test]
fn contract_order_within_an_entry() {
    assert_eq!(
        verdict(
            &hostile_tar_gz("notallowed.txt", b"{}"),
            VerifyLimits::default()
        ),
        (ErrorClass::Contract, ErrorCode::ContractUnexpectedFile),
    );

    // Allowlisted, but the manifest must come first.
    assert_eq!(
        verdict(
            &hostile_tar_gz("events.ndjson", b"{}"),
            VerifyLimits::default()
        ),
        (ErrorClass::Contract, ErrorCode::ContractFileOrder),
    );
}

fn nested_payload(depth: usize) -> serde_json::Value {
    let mut value = serde_json::json!("leaf");
    for _ in 0..depth {
        value = serde_json::json!({ "n": value });
    }
    value
}

fn bundle_with_payload(payload: serde_json::Value) -> Vec<u8> {
    let mut event = EvidenceEvent::new(
        "assay.test.event",
        "urn:assay:test",
        "run_deterministic_test",
        0,
        payload,
    );
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();

    let mut buffer = Vec::new();
    {
        let mut writer = BundleWriter::new(&mut buffer);
        writer.add_event(event);
        writer.finish().expect("write bundle");
    }
    buffer
}

/// `max_json_depth` used to be a knob that did nothing: the validator compared
/// against a hardcoded ceiling and never saw the configured one.
#[test]
fn configured_json_depth_is_enforced() {
    let bundle = bundle_with_payload(nested_payload(20));

    // Comfortably inside the default ceiling.
    verify_bundle(Cursor::new(&bundle)).expect("accepted under default limits");

    // An operator who hardens the ceiling now actually gets it, and gets told
    // it was the depth limit rather than generic malformed JSON.
    assert_eq!(
        verdict(
            &bundle,
            VerifyLimits {
                max_json_depth: 5,
                ..VerifyLimits::default()
            }
        ),
        (ErrorClass::Limits, ErrorCode::LimitJsonDepth),
    );
}

/// The line-length ceiling is signalled by a typed marker carried inside the
/// io::Error. It used to be a substring of the error message, so a rename
/// degraded silently at runtime into a generic io failure.
#[test]
fn line_limit_survives_the_io_boundary_as_a_typed_signal() {
    let bundle = bundle_with_payload(serde_json::json!({ "pad": "x".repeat(4096) }));

    assert_eq!(
        verdict(
            &bundle,
            VerifyLimits {
                max_line_bytes: 64,
                ..VerifyLimits::default()
            }
        ),
        (ErrorClass::Limits, ErrorCode::LimitLineBytes),
    );
}
