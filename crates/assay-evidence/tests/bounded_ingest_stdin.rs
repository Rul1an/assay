//! ADR-043 §1 for the stdin verify entrypoint.
//!
//! `cmd_verify` on stdin used to `read_to_end` the pipe into a `Vec` and then hand a `Cursor`
//! to `verify_bundle`. The verifier is streaming and applies `max_bundle_bytes` to bytes as
//! they arrive; interposing a full `read_to_end` between the pipe and the verifier defeats
//! that. On stdin specifically there is no size to check beforehand either, so an oversized
//! upload was materialised in full before any limit was consulted.
//!
//! The CLI change is a one-line contract-fix (hand `stdin.lock()` to `verify_bundle`). This
//! file pins the invariant at the layer the CLI hands to: given the same input the CLI would
//! now hand it, verify must stop reading at the ceiling and classify the refusal as data.
//!
//! Stdin cannot be substituted in tests, so we drive `verify_bundle_with_limits` directly. The
//! streaming ceiling is a property of the verifier, not of the CLI wrapper, so exercising it at
//! this layer is what the CLI now inherits by construction.

use assay_evidence::bundle::writer::{
    verify_bundle_with_limits, BundleWriter, ErrorClass, ErrorCode, VerifyError, VerifyLimits,
};
use assay_evidence::types::EvidenceEvent;
use std::cell::Cell;
use std::io::{Cursor, Read};
use std::rc::Rc;

/// Counts every byte the consumer pulls from the source, so a test can distinguish "verify
/// refused" from "verify stopped reading". Both look the same at the exit code.
struct Counting<R> {
    inner: R,
    pulled: Rc<Cell<u64>>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.pulled.set(self.pulled.get() + n as u64);
        Ok(n)
    }
}

fn counting(bytes: Vec<u8>) -> (Counting<Cursor<Vec<u8>>>, Rc<Cell<u64>>) {
    let pulled = Rc::new(Cell::new(0));
    (
        Counting {
            inner: Cursor::new(bytes),
            pulled: Rc::clone(&pulled),
        },
        pulled,
    )
}

fn bundle() -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buffer);
        w.add_event(EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_stdin_bounded",
            0,
            serde_json::json!({"seq": 0}),
        ));
        w.finish().expect("write bundle");
    }
    buffer
}

#[test]
fn stdin_verify_stops_reading_at_the_compressed_ceiling() {
    let bytes = bundle();
    let ceiling = 64u64;
    assert!(
        (bytes.len() as u64) > ceiling * 4,
        "fixture must be comfortably larger than the ceiling under test"
    );

    let (source, pulled) = counting(bytes);
    let limits = VerifyLimits {
        max_bundle_bytes: ceiling,
        ..VerifyLimits::default()
    };
    assert!(verify_bundle_with_limits(source, limits).is_err());
    assert!(
        pulled.get() <= ceiling + 1,
        "verify must stop at the compressed ceiling; {} bytes were pulled for a ceiling of {ceiling}",
        pulled.get()
    );
}

#[test]
fn stdin_verify_classifies_the_refusal_as_a_limit() {
    let bytes = bundle();
    let limits = VerifyLimits {
        max_bundle_bytes: (bytes.len() as u64) - 1,
        ..VerifyLimits::default()
    };
    let err = verify_bundle_with_limits(Cursor::new(bytes), limits)
        .expect_err("one byte over the compressed ceiling must be refused");
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("stdin verify refusals must surface as the same typed VerifyError");
    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitBundleBytes);
}

/// Acceptance twin. Without this, a ceiling that refused everything would satisfy both
/// assertions above.
#[test]
fn stdin_verify_still_accepts_a_bundle_within_the_defaults() {
    let bytes = bundle();
    verify_bundle_with_limits(Cursor::new(bytes), VerifyLimits::default())
        .expect("a small bundle must verify under the default limits");
}
