//! ADR-043 §1 for the lint entrypoint.
//!
//! `lint_bundle_with_options` used to read the whole compressed input into memory with an
//! unbounded `read_to_end`, verify a `Cursor` over that buffer, and then walk the tar a second
//! time to extract `events.ndjson` through its own private decoder that carried none of the
//! caller's limits. Every dimension in `VerifyLimits` outside the compressed byte ceiling had at
//! most a partial effect: the manual second pass ignored them.
//!
//! The refactor routes lint through `BundleReader::open_with_limits`, so the same bounded pass
//! that closed the reader entrypoint governs lint too. These tests pin two things: the ceiling
//! stops the source stream, and refusals surface with the same typed classification the other
//! reader consumers already return.

use assay_evidence::bundle::writer::{
    BundleWriter, ErrorClass, ErrorCode, VerifyError, VerifyLimits,
};
use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions};
use assay_evidence::types::EvidenceEvent;
use std::cell::Cell;
use std::io::{Cursor, Read};
use std::rc::Rc;

/// Counts every byte the consumer pulls from the source, so a test can assert that ingest
/// stopped at the ceiling rather than draining the input first and refusing afterwards.
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

fn small_bundle() -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buffer);
        w.add_event(EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_lint_bounded",
            0,
            serde_json::json!({"seq": 0}),
        ));
        w.finish().expect("write bundle");
    }
    buffer
}

/// The invariant. An oversized archive must stop being read at the compressed ceiling; a lint
/// path that drains the whole input and refuses afterwards has already paid the memory even if
/// the exit code looks the same.
#[test]
fn an_oversized_bundle_stops_being_read_at_the_ceiling() {
    let bytes = small_bundle();
    let ceiling = 64u64;
    assert!(
        (bytes.len() as u64) > ceiling * 4,
        "fixture must be comfortably larger than the ceiling under test"
    );

    let limits = VerifyLimits {
        max_bundle_bytes: ceiling,
        ..VerifyLimits::default()
    };
    let (source, pulled) = counting(bytes);
    assert!(lint_bundle_with_options(source, limits, LintOptions::default()).is_err());
    assert!(
        pulled.get() <= ceiling + 1,
        "lint ingest must stop at the compressed ceiling; {} bytes were pulled for a ceiling of {ceiling}",
        pulled.get()
    );
}

/// A refusal that used to reach lint through the private extractor was `anyhow`-wrapped and
/// stringly-typed, so an operator could not tell "raise the budget" from "the producer emitted a
/// broken bundle". Every ceiling now comes back as a `VerifyError` with the same class and code
/// the verifier itself produces.
#[test]
fn a_ceiling_refusal_surfaces_as_a_typed_verify_error() {
    let bytes = small_bundle();
    let limits = VerifyLimits {
        max_bundle_bytes: (bytes.len() as u64) - 1,
        ..VerifyLimits::default()
    };
    let err = lint_bundle_with_options(Cursor::new(bytes), limits, LintOptions::default())
        .expect_err("one byte under the compressed ceiling must be refused");
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("lint refusals must surface as the same typed VerifyError as the verifier");
    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitBundleBytes);
}

/// Acceptance twin: without this, a lint refactor that simply refused everything would satisfy
/// the two tests above. A bundle within every dimension must still produce a report.
#[test]
fn a_bundle_within_every_ceiling_still_lints() {
    let bytes = small_bundle();
    let report = lint_bundle_with_options(
        Cursor::new(bytes),
        VerifyLimits::default(),
        LintOptions::default(),
    )
    .expect("a small bundle must lint under the default limits");
    assert!(
        report.report.verified,
        "the report must record verification"
    );
}
