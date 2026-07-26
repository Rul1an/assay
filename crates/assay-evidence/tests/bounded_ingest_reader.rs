//! ADR-043 §1 for the bundle reader entrypoints.
//!
//! `BundleReader::open_internal` read the whole input with `read_to_end` and only then handed a
//! `Cursor` to the verifier, so `max_bundle_bytes` was consulted after the archive had already
//! sized the allocation. `open_unverified` and the manifest peek had no ceiling at all, on the
//! reasoning that they do not verify; not verifying is not a reason to let an untrusted archive
//! decide how much memory to take.
//!
//! These tests pin *when* the ceiling applies, not merely whether the call fails. Asserting only
//! `is_err()` is not enough: `verify_bundle_with_limits` refuses an oversized bundle downstream
//! anyway, so an unbounded ingest still ends in an error, having first read the whole archive
//! into memory. The invariant is that the bytes stop flowing at the ceiling, so the tests count
//! what the reader actually pulled from the source.
//!
//! Every refusal case is paired with an acceptance case built from the same bundle, so a ceiling
//! that simply refused everything would not pass.

use assay_evidence::bundle::writer::{
    BundleWriter, ErrorClass, ErrorCode, VerifyError, VerifyLimits,
};
use assay_evidence::bundle::BundleReader;
use assay_evidence::types::EvidenceEvent;
use std::cell::Cell;
use std::io::{Cursor, Read};
use std::rc::Rc;

fn bundle_bytes() -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut writer = BundleWriter::new(&mut buffer);
        writer.add_event(EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_bounded_ingest",
            0,
            serde_json::json!({"seq": 0}),
        ));
        writer.finish().expect("write bundle");
    }
    buffer
}

/// Rebuild a bundle without one member, so an absent-file case is a real archive rather than a
/// truncated one. Repacking keeps every other member byte-identical.
fn strip_member(bundle: &[u8], drop_path: &str) -> Vec<u8> {
    use flate2::read::GzDecoder;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let mut kept: Vec<(String, Vec<u8>)> = Vec::new();
    {
        let mut ar = tar::Archive::new(GzDecoder::new(Cursor::new(bundle.to_vec())));
        for entry in ar.entries().expect("entries") {
            let mut e = entry.expect("entry");
            let path = e.path().expect("path").to_string_lossy().to_string();
            let mut data = Vec::new();
            e.read_to_end(&mut data).expect("read member");
            if path != drop_path {
                kept.push((path, data));
            }
        }
    }

    let mut out = Vec::new();
    {
        let enc = GzEncoder::new(&mut out, Compression::default());
        let mut tar = tar::Builder::new(enc);
        for (path, data) in kept {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, &path, Cursor::new(data))
                .expect("append");
        }
        tar.into_inner().expect("tar").finish().expect("gz");
    }
    out
}

fn limits_with_bundle_ceiling(max_bundle_bytes: u64) -> VerifyLimits {
    VerifyLimits {
        max_bundle_bytes,
        ..VerifyLimits::default()
    }
}

/// A reader that never fills the caller's buffer. A ceiling that only counts whole reads could
/// be walked past one byte at a time.
struct DripFeed(Cursor<Vec<u8>>);

impl Read for DripFeed {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.0.read(&mut buf[..1])
    }
}

/// Counts every byte handed to the consumer, so a test can assert the stream stopped at the
/// ceiling instead of being drained and rejected afterwards.
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

#[test]
fn verified_open_accepts_exactly_the_ceiling() {
    let bytes = bundle_bytes();
    let exact = limits_with_bundle_ceiling(bytes.len() as u64);
    BundleReader::open_with_limits(Cursor::new(bytes), exact)
        .expect("a bundle of exactly max_bundle_bytes must be accepted");
}

#[test]
fn verified_open_refuses_one_byte_over_the_ceiling() {
    let bytes = bundle_bytes();
    let one_short = limits_with_bundle_ceiling(bytes.len() as u64 - 1);
    let err = match BundleReader::open_with_limits(Cursor::new(bytes), one_short) {
        Ok(_) => panic!("a bundle one byte over the ceiling must be refused"),
        Err(e) => e,
    };
    // Through the typed cause, not the message. A public reader entrypoint must classify its
    // ceilings exactly as the verifier does, or a caller cannot tell a refusal from a truncated
    // file without reading prose.
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("a reader ceiling refusal must surface as a typed VerifyError");
    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitBundleBytes);
}

/// The invariant itself. An oversized archive must stop being read at the ceiling; a verifier
/// that drains the whole input and refuses afterwards has already paid the memory.
#[test]
fn an_oversized_archive_stops_being_read_at_the_ceiling() {
    let bytes = bundle_bytes();
    let ceiling = 64u64;
    assert!(
        (bytes.len() as u64) > ceiling * 4,
        "the fixture must be comfortably larger than the ceiling under test"
    );

    let (source, pulled) = counting(bytes);
    assert!(BundleReader::open_with_limits(source, limits_with_bundle_ceiling(ceiling)).is_err());
    assert!(
        pulled.get() <= ceiling + 1,
        "ingest must stop at the ceiling; {} bytes were pulled for a ceiling of {ceiling}",
        pulled.get()
    );
}

/// Same invariant on the path that never verifies, where nothing downstream would catch it.
#[test]
fn an_unverified_open_also_stops_reading_at_the_ceiling() {
    let bytes = bundle_bytes();
    let ceiling = 64u64;
    let (source, pulled) = counting(bytes);
    assert!(
        BundleReader::open_unverified_with_limits(source, limits_with_bundle_ceiling(ceiling))
            .is_err()
    );
    assert!(
        pulled.get() <= ceiling + 1,
        "unverified ingest must stop at the ceiling too; {} bytes were pulled",
        pulled.get()
    );
}

#[test]
fn unverified_open_is_still_bounded() {
    // The bite: before this change `open_unverified` passed `None` for limits and read the whole
    // input regardless of size. Skipping verification skipped the ceiling with it.
    let bytes = bundle_bytes();
    let one_short = limits_with_bundle_ceiling(bytes.len() as u64 - 1);
    assert!(
        BundleReader::open_unverified_with_limits(Cursor::new(bytes.clone()), one_short).is_err(),
        "an unverified open must still refuse an oversized archive"
    );
    let exact = limits_with_bundle_ceiling(bytes.len() as u64);
    BundleReader::open_unverified_with_limits(Cursor::new(bytes), exact)
        .expect("and must still accept one that fits");
}

#[test]
fn the_default_open_carries_a_ceiling_rather_than_none() {
    // `open` and `open_unverified` take no limits argument. Neither may therefore be unbounded:
    // the default ceiling applies. A bundle far under it is accepted, which is what keeps this
    // from being satisfied by refusing everything.
    let bytes = bundle_bytes();
    assert!(bytes.len() < VerifyLimits::default().max_bundle_bytes as usize);
    BundleReader::open(Cursor::new(bytes.clone())).expect("a small bundle passes the default");
    BundleReader::open_unverified(Cursor::new(bytes)).expect("unverified likewise");
}

#[test]
fn short_reads_cannot_walk_past_the_ceiling() {
    let bytes = bundle_bytes();
    let one_short = limits_with_bundle_ceiling(bytes.len() as u64 - 1);
    assert!(
        BundleReader::open_with_limits(DripFeed(Cursor::new(bytes.clone())), one_short).is_err(),
        "a one-byte-at-a-time stream must not slip past the ceiling"
    );
    let exact = limits_with_bundle_ceiling(bytes.len() as u64);
    BundleReader::open_with_limits(DripFeed(Cursor::new(bytes)), exact)
        .expect("and must still be accepted when it fits");
}

/// The ceiling counts bytes yielded, not bytes announced.
///
/// This was written as a "hostile metadata" case, which overclaimed: `Read` has no size hint, so
/// a plain reader has no metadata to lie with and the wrapper below announced nothing. The narrow
/// and true statement is that no source metadata participates in the ceiling at all; it is a
/// running count of what the reader handed over. A chunked source that delivers in irregular
/// pieces is the shape that could plausibly desynchronise such a count, so that is what is tested.
#[test]
fn a_chunked_source_cannot_desynchronise_the_count() {
    struct Chunked {
        inner: Cursor<Vec<u8>>,
        sizes: Vec<usize>,
        next: usize,
    }
    impl Read for Chunked {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let want = self.sizes[self.next % self.sizes.len()].min(buf.len());
            self.next += 1;
            self.inner.read(&mut buf[..want.max(1)])
        }
    }

    let bytes = bundle_bytes();
    let source = Chunked {
        inner: Cursor::new(bytes.clone()),
        sizes: vec![1, 7, 3, 64, 2],
        next: 0,
    };
    let one_short = limits_with_bundle_ceiling(bytes.len() as u64 - 1);
    assert!(
        BundleReader::open_with_limits(source, one_short).is_err(),
        "irregular chunk sizes must not let the source past the ceiling"
    );

    let source = Chunked {
        inner: Cursor::new(bytes.clone()),
        sizes: vec![1, 7, 3, 64, 2],
        next: 0,
    };
    BundleReader::open_with_limits(source, limits_with_bundle_ceiling(bytes.len() as u64))
        .expect("the same chunked source must be accepted at exactly the ceiling");
}

/// A bundle whose `events.ndjson` is highly compressible. Compressed it sits far inside
/// `max_bundle_bytes`; expanded it does not. This is the case ADR-043 §1 means by "a single byte
/// ceiling is not that set".
fn compressible_bundle(event_count: u64) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut writer = BundleWriter::new(&mut buffer);
        for seq in 0..event_count {
            writer.add_event(EvidenceEvent::new(
                "assay.test",
                "urn:assay:test",
                "run_bomb",
                seq,
                serde_json::json!({ "pad": "A".repeat(512) }),
            ));
        }
        writer.finish().expect("write bundle");
    }
    buffer
}

#[test]
fn a_decompression_bomb_inside_the_compressed_ceiling_is_refused() {
    let bytes = compressible_bundle(400);
    let compressed_len = bytes.len() as u64;

    // Generous compressed ceiling, so the compressed-byte guard cannot be what refuses this.
    let limits = VerifyLimits {
        max_bundle_bytes: compressed_len * 8,
        max_decode_bytes: 4096,
        ..VerifyLimits::default()
    };

    assert!(
        BundleReader::open_with_limits(Cursor::new(bytes.clone()), limits).is_err(),
        "expansion past max_decode_bytes must be refused even when the compressed input fits"
    );

    // The acceptance twin: the same bundle under a decode ceiling that accommodates it.
    let roomy = VerifyLimits {
        max_bundle_bytes: compressed_len * 8,
        ..VerifyLimits::default()
    };
    BundleReader::open_with_limits(Cursor::new(bytes), roomy)
        .expect("the same bundle must pass when the decode ceiling accommodates it");
}

#[test]
fn the_peek_path_is_bounded_on_expansion_too() {
    // The peek path never runs verification, so this pass is its only consumption. Nothing
    // downstream would catch an expansion here.
    let bytes = compressible_bundle(400);
    let limits = VerifyLimits {
        max_bundle_bytes: bytes.len() as u64 * 8,
        max_decode_bytes: 4096,
        ..VerifyLimits::default()
    };
    assert!(
        BundleReader::open_unverified_with_limits(Cursor::new(bytes), limits).is_err(),
        "an unverified open must refuse expansion past the decode ceiling"
    );
}

#[test]
fn peek_called_directly_is_bounded_on_the_compressed_input_too() {
    // Reached through `BundleReader` the input has already passed a ceiling. `peek_with_limits`
    // is public in its own right, so a direct caller must get the compressed bound as well and
    // not merely the expansion one.
    let bytes = bundle_bytes();
    let ceiling = 64u64;
    assert!((bytes.len() as u64) > ceiling * 4);

    let (source, pulled) = counting(bytes.clone());
    assert!(
        assay_evidence::bundle::reader::BundleInfo::peek_with_limits(
            source,
            limits_with_bundle_ceiling(ceiling)
        )
        .is_err()
    );
    assert!(
        pulled.get() <= ceiling + 1,
        "a direct peek must stop at max_bundle_bytes; {} bytes were pulled",
        pulled.get()
    );

    // Acceptance twin: the same bundle under a ceiling that fits.
    assay_evidence::bundle::reader::BundleInfo::peek_with_limits(
        Cursor::new(bytes.clone()),
        limits_with_bundle_ceiling(bytes.len() as u64),
    )
    .expect("a direct peek must still accept a bundle that fits");
}

#[test]
fn the_unverified_path_classifies_its_ceilings_too() {
    // The path with no downstream verifier is where an unclassified io error would be hardest to
    // interpret, so it must carry the same typed cause.
    let bytes = bundle_bytes();
    let one_short = limits_with_bundle_ceiling(bytes.len() as u64 - 1);
    let err = match BundleReader::open_unverified_with_limits(Cursor::new(bytes), one_short) {
        Ok(_) => panic!("must be refused"),
        Err(e) => e,
    };
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("an unverified ceiling refusal must be typed as well");
    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitBundleBytes);
}

#[test]
fn a_direct_peek_classifies_its_ceilings_too() {
    // A ceiling of `len - 1` would not do here: peek stops reading once it has the manifest, so
    // it never reaches the end of the archive and a near-total ceiling is never crossed. The
    // bound has to be small enough that the partial read itself hits it.
    let bytes = bundle_bytes();
    let err = match assay_evidence::bundle::reader::BundleInfo::peek_with_limits(
        Cursor::new(bytes.clone()),
        limits_with_bundle_ceiling(64),
    ) {
        Ok(_) => panic!("must be refused"),
        Err(e) => e,
    };
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("a peek ceiling refusal must be typed as well");
    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitBundleBytes);
}

#[test]
fn an_expansion_refusal_classifies_as_decode_not_source() {
    // The dimension has to survive classification, otherwise every ceiling looks alike to a
    // caller deciding whether to retry with a larger budget.
    let bytes = compressible_bundle(400);
    let limits = VerifyLimits {
        max_bundle_bytes: bytes.len() as u64 * 8,
        max_decode_bytes: 4096,
        ..VerifyLimits::default()
    };
    let err = match BundleReader::open_unverified_with_limits(Cursor::new(bytes), limits) {
        Ok(_) => panic!("must be refused"),
        Err(e) => e,
    };
    let ve = err.downcast_ref::<VerifyError>().expect("typed");
    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitDecodeBytes);
}

/// The ceiling has to cover the whole source, not the prefix the parser happens to consume.
///
/// gzip and tar stop once the archive is logically complete, so a valid bundle followed by an
/// arbitrary suffix used to pass a ceiling far below the real input size: the trailing bytes were
/// never requested and therefore never counted. A 10 KiB suffix cleared a 598 byte ceiling.
#[test]
fn a_valid_bundle_with_an_unread_suffix_is_measured_whole() {
    use assay_evidence::bundle::writer::verify_bundle_with_limits;

    let valid = bundle_bytes();
    let mut with_suffix = valid.clone();
    with_suffix.extend(std::iter::repeat_n(b'Z', 10_000));
    let total = with_suffix.len() as u64;

    // Exactly the real input size is accepted, including the suffix nothing parses.
    verify_bundle_with_limits(
        Cursor::new(with_suffix.clone()),
        limits_with_bundle_ceiling(total),
    )
    .expect("a source of exactly the ceiling must be accepted, suffix included");

    // One byte under is refused, which is only observable once the suffix is counted.
    let err = match verify_bundle_with_limits(
        Cursor::new(with_suffix),
        limits_with_bundle_ceiling(total - 1),
    ) {
        Ok(_) => panic!("the suffix must count towards the source ceiling"),
        Err(e) => e,
    };
    let ve = err.downcast_ref::<VerifyError>().expect("typed");
    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitBundleBytes);

    // Control: the bundle without its suffix still verifies under a ceiling that fits it.
    verify_bundle_with_limits(
        Cursor::new(valid.clone()),
        limits_with_bundle_ceiling(valid.len() as u64),
    )
    .expect("the bundle alone must still verify");
}

/// `events.ndjson` on the unverified path used to be read whole under `max_events_bytes` and only
/// then checked for line length and event count, so the allocation happened before the dimensions
/// governing it were consulted. `max_json_depth` never reached event lines on this path at all,
/// and an absent member read as a well-formed bundle with zero events.
mod unverified_events {
    use super::*;
    use assay_evidence::bundle::writer::BundleWriter;

    fn bundle_of(count: u64, pad: usize, depth: usize) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = BundleWriter::new(&mut buf);
            for seq in 0..count {
                let mut payload = serde_json::json!({ "pad": "A".repeat(pad) });
                for _ in 0..depth {
                    payload = serde_json::json!({ "n": payload });
                }
                w.add_event(EvidenceEvent::new(
                    "assay.test",
                    "urn:assay:test",
                    "run_unverified",
                    seq,
                    payload,
                ));
            }
            w.finish().expect("write bundle");
        }
        buf
    }

    fn refuse(bytes: &[u8], limits: VerifyLimits) -> (ErrorClass, ErrorCode) {
        let err =
            match BundleReader::open_unverified_with_limits(Cursor::new(bytes.to_vec()), limits) {
                Ok(_) => panic!("the unverified path must apply this ceiling"),
                Err(e) => e,
            };
        let ve = err
            .downcast_ref::<VerifyError>()
            .expect("an unverified refusal must be typed");
        (ve.class, ve.code)
    }

    #[test]
    fn a_deeply_nested_event_is_refused_at_the_caller_depth() {
        let bytes = bundle_of(1, 8, 12);
        assert_eq!(
            refuse(
                &bytes,
                VerifyLimits {
                    max_json_depth: 4,
                    ..VerifyLimits::default()
                }
            ),
            (ErrorClass::Limits, ErrorCode::LimitJsonDepth)
        );
        BundleReader::open_unverified_with_limits(Cursor::new(bytes), VerifyLimits::default())
            .expect("the same events must open under the default depth");
    }

    #[test]
    fn a_line_over_the_ceiling_is_refused() {
        let bytes = bundle_of(1, 512, 0);
        assert_eq!(
            refuse(
                &bytes,
                VerifyLimits {
                    max_line_bytes: 64,
                    ..VerifyLimits::default()
                }
            ),
            (ErrorClass::Limits, ErrorCode::LimitLineBytes)
        );
        BundleReader::open_unverified_with_limits(Cursor::new(bytes), VerifyLimits::default())
            .expect("the same line must open under the default ceiling");
    }

    #[test]
    fn one_event_over_the_count_ceiling_is_refused() {
        let bytes = bundle_of(4, 8, 0);
        assert_eq!(
            refuse(
                &bytes,
                VerifyLimits {
                    max_events: 3,
                    ..VerifyLimits::default()
                }
            ),
            (ErrorClass::Limits, ErrorCode::LimitTotalEvents)
        );
        // Exactly the ceiling is accepted, so the refusal above is the boundary and not the shape.
        BundleReader::open_unverified_with_limits(
            Cursor::new(bytes),
            VerifyLimits {
                max_events: 4,
                ..VerifyLimits::default()
            },
        )
        .expect("exactly the event ceiling must be accepted");
    }

    /// An absent member is not an empty one. Accepting a stripped archive as a zero-event run is
    /// the difference between "there were no events" and "the events were removed".
    #[test]
    fn a_bundle_without_events_ndjson_is_refused() {
        let full = bundle_of(1, 8, 0);
        let stripped = super::strip_member(&full, "events.ndjson");

        let err = match BundleReader::open_unverified_with_limits(
            Cursor::new(stripped),
            VerifyLimits::default(),
        ) {
            Ok(_) => panic!("a bundle without events.ndjson must not read as an empty run"),
            Err(e) => e,
        };
        let ve = err.downcast_ref::<VerifyError>().expect("typed");
        assert_eq!(ve.class, ErrorClass::Contract);
        assert_eq!(ve.code, ErrorCode::ContractMissingFile);

        BundleReader::open_unverified_with_limits(Cursor::new(full), VerifyLimits::default())
            .expect("the intact bundle must still open");
    }
}

/// The existing expansion test goes through `open_unverified_with_limits`, which reaches peek but
/// also runs the reader's own second pass, so it cannot show which of the two refused. This calls
/// `BundleInfo::peek_with_limits` directly and pins that peek carries its own decode ceiling.
///
/// The ceiling has to sit under what peek actually expands. Peek stops once it has the manifest,
/// which the writer emits first, so a large trailing member never reaches the decoder however big
/// it is: a ceiling above the manifest is simply never crossed. The bound is real within the
/// prefix peek reads, and that is what this pins.
#[test]
fn peek_alone_refuses_expansion_past_the_decode_ceiling() {
    let bytes = compressible_bundle(400);
    let limits = VerifyLimits {
        max_bundle_bytes: bytes.len() as u64 * 8,
        max_decode_bytes: 64,
        ..VerifyLimits::default()
    };

    let err = match assay_evidence::bundle::reader::BundleInfo::peek_with_limits(
        Cursor::new(bytes.clone()),
        limits,
    ) {
        Ok(_) => panic!("peek must refuse expansion past its own decode ceiling"),
        Err(e) => e,
    };
    let ve = err
        .downcast_ref::<VerifyError>()
        .expect("a peek expansion refusal must be typed");
    assert_eq!(ve.class, ErrorClass::Limits);
    assert_eq!(ve.code, ErrorCode::LimitDecodeBytes);

    // Acceptance twin: the same bundle under a decode ceiling that accommodates it.
    assay_evidence::bundle::reader::BundleInfo::peek_with_limits(
        Cursor::new(bytes),
        VerifyLimits::default(),
    )
    .expect("the same bundle must peek under the default decode ceiling");
}
