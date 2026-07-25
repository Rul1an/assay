//! ADR-043 §3: deterministic fail-closed properties for the evidence-chain verifier.
//!
//! These are the reviewable oracle that sits beside the fuzz target. The division of labour is
//! deliberate: the fuzzer explores for crashes, hangs and unbounded resource use on inputs nobody
//! wrote down, while these tests pin the *semantic* outcome of inputs we can name. A fuzzer that
//! finds nothing proves the verifier did not fall over; only a named expectation proves it rejected
//! the right thing for the right reason.
//!
//! They are also distinct from `assay-sim`'s integrity attacks, which run the same broad shapes
//! (bitflip, truncate, inject, tar duplicate) and assert `AttackStatus::Blocked`. "Blocked" is
//! satisfied by any error, so a mutation that starts failing for an unrelated reason — a limit trip
//! instead of a hash mismatch, say — still reads green there. Here the `ErrorCode` itself is the
//! assertion, because a verifier that rejects everything is fail-closed and useless, and the way
//! you tell the two apart is which reason it gives.

use assay_evidence::bundle::writer::BundleWriter;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::{verify_bundle_with_limits, ErrorClass, ErrorCode, VerifyError, VerifyLimits};
use chrono::{TimeZone, Utc};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Small explicit limits. Deliberately not `VerifyLimits::default()`: the defaults are large
/// enough that a limit property would need a 100 MB fixture to trip, and a test that expensive
/// does not get run. These also become the limits the fuzz target uses.
fn small_limits() -> VerifyLimits {
    VerifyLimits {
        max_bundle_bytes: 1 << 20,
        max_decode_bytes: 4 << 20,
        max_manifest_bytes: 64 << 10,
        max_events_bytes: 256 << 10,
        max_events: 64,
        max_line_bytes: 8 << 10,
        max_path_len: 128,
        max_json_depth: 16,
    }
}

fn event(seq: u64) -> EvidenceEvent {
    let mut e = EvidenceEvent::new(
        "assay.verifier.property",
        "urn:assay:verifier-property",
        "run_verifier_property_0001",
        seq,
        serde_json::json!({ "seq": seq, "payload": "aaaaaaaa" }),
    );
    // Fixed so the fixture is byte-stable: a property that depends on wall-clock time is a
    // property that fails on a slow machine for reasons unrelated to the verifier.
    e.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
    e.producer = "assay-evidence-property-test".to_string();
    e.producer_version = "0.0.0-test".to_string();
    e.git_sha = "0000000".to_string();
    e
}

fn valid_bundle(event_count: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buf);
        for seq in 0..event_count {
            w.add_event(event(seq));
        }
        w.finish().expect("writer produces a bundle");
    }
    buf
}

fn unpack(bundle: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut manifest = Vec::new();
    let mut events = Vec::new();
    let mut archive = tar::Archive::new(GzDecoder::new(Cursor::new(bundle)));
    for entry in archive.entries().expect("entries") {
        let mut entry = entry.expect("entry");
        let path = entry.path().expect("path").to_string_lossy().to_string();
        match path.as_str() {
            "manifest.json" => entry
                .read_to_end(&mut manifest)
                .map(|_| ())
                .expect("manifest"),
            "events.ndjson" => entry.read_to_end(&mut events).map(|_| ()).expect("events"),
            _ => {}
        }
    }
    (manifest, events)
}

/// Repack in contract order. Used to isolate one mutation at a time: everything the verifier
/// checks before the mutated field must still be intact, or the test proves nothing about the
/// field it claims to be testing.
fn repack(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        for (path, content) in members {
            let mut header = tar::Header::new_gnu();
            header.set_path(path).expect("set_path");
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_cksum();
            builder.append(&header, *content).expect("append");
        }
        builder.finish().expect("finish tar");
    }
    encoder.finish().expect("finish gzip")
}

/// Decoded length of a gzip stream, used to assert a seed really does expand.
fn decoded_len(gzipped: &[u8]) -> usize {
    let mut sink = Vec::new();
    GzDecoder::new(Cursor::new(gzipped))
        .read_to_end(&mut sink)
        .expect("seed must be valid gzip");
    sink.len()
}

fn sha256_prefixed(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

/// Rewrite the manifest's recorded hash and size for `events.ndjson`, so a test that targets a
/// later check (ordering, sequence contract) is not short-circuited by the earlier hash check.
fn reseal(manifest: &[u8], events: &[u8]) -> Vec<u8> {
    let mut m: serde_json::Value = serde_json::from_slice(manifest).expect("manifest json");
    m["files"]["events.ndjson"]["sha256"] = serde_json::json!(sha256_prefixed(events));
    m["files"]["events.ndjson"]["bytes"] = serde_json::json!(events.len());
    serde_json::to_vec(&m).expect("reserialize manifest")
}

/// The classification under test. `Ok` means the bundle verified.
fn classify_with(bundle: &[u8], limits: VerifyLimits) -> Result<(), (ErrorClass, ErrorCode)> {
    match verify_bundle_with_limits(Cursor::new(bundle), limits) {
        Ok(_) => Ok(()),
        Err(e) => {
            let ve = e.downcast_ref::<VerifyError>().unwrap_or_else(|| {
                panic!("verifier must fail with a typed VerifyError, got: {e:#}")
            });
            Err((ve.class, ve.code))
        }
    }
}

fn classify(bundle: &[u8]) -> Result<(), (ErrorClass, ErrorCode)> {
    classify_with(bundle, small_limits())
}

fn expect_rejected(bundle: &[u8], what: &str) -> (ErrorClass, ErrorCode) {
    classify(bundle).expect_err(&format!("{what} must not verify"))
}

// ---------------------------------------------------------------------------
// Control arm
// ---------------------------------------------------------------------------

/// Without this the whole file is worthless: every rejection below would be explained equally well
/// by a fixture the verifier never accepts in the first place.
#[test]
fn an_unmutated_bundle_verifies() {
    assert_eq!(classify(&valid_bundle(3)), Ok(()));
}

/// The repack path must also be neutral, for the same reason: the mutation has to be the only
/// difference between a bundle that verifies and one that does not.
#[test]
fn repacking_an_unmutated_bundle_is_neutral() {
    let bundle = valid_bundle(3);
    let (manifest, events) = unpack(&bundle);
    let repacked = repack(&[("manifest.json", &manifest), ("events.ndjson", &events)]);
    assert_eq!(classify(&repacked), Ok(()));
}

// ---------------------------------------------------------------------------
// Payload and digest mutation
// ---------------------------------------------------------------------------

/// Integrity is two layers, and which one fires tells you what was touched. The per-event content
/// hash covers `specversion`, `type`, `datacontenttype`, `subject` and `payload`, so a payload
/// edit is caught there — per event, before the whole-file digest is even finalized.
#[test]
fn a_payload_mutation_is_caught_by_the_per_event_content_hash() {
    let bundle = valid_bundle(3);
    let (manifest, events) = unpack(&bundle);

    // Equal byte length on purpose: a length change would also trip the size check, and then the
    // test would pass without the hash check ever being the reason.
    let mutated = String::from_utf8(events.clone())
        .expect("utf8")
        .replacen("aaaaaaaa", "aaaaaaab", 1)
        .into_bytes();
    assert_eq!(mutated.len(), events.len(), "mutation must preserve length");
    assert_ne!(mutated, events, "mutation must actually change the payload");

    let (class, code) = expect_rejected(
        &repack(&[("manifest.json", &manifest), ("events.ndjson", &mutated)]),
        "a mutated event payload",
    );
    assert_eq!(
        (class, code),
        (ErrorClass::Integrity, ErrorCode::IntegrityEventHash)
    );
}

/// The other layer. `git_sha` is deliberately outside the content hash, so an edit there leaves
/// every per-event hash intact and is caught only by the file digest. Together with the test above
/// this pins the division: neither layer is redundant, and a change that silently moved a field
/// across that boundary would flip exactly one of these two codes.
#[test]
fn a_mutation_outside_the_content_hash_is_caught_by_the_file_digest() {
    let bundle = valid_bundle(2);
    let (manifest, events) = unpack(&bundle);

    let mutated = String::from_utf8(events.clone())
        .expect("utf8")
        .replacen("\"0000000\"", "\"0000001\"", 1)
        .into_bytes();
    assert_eq!(mutated.len(), events.len(), "mutation must preserve length");
    assert_ne!(mutated, events, "fixture must actually carry the git sha");

    let (class, code) = expect_rejected(
        &repack(&[("manifest.json", &manifest), ("events.ndjson", &mutated)]),
        "an edit to a field outside the content hash",
    );
    assert_eq!(
        (class, code),
        (ErrorClass::Integrity, ErrorCode::IntegrityManifestHash)
    );
}

#[test]
fn a_mutated_recorded_digest_is_an_events_hash_mismatch() {
    let bundle = valid_bundle(2);
    let (manifest, events) = unpack(&bundle);

    let mut m: serde_json::Value = serde_json::from_slice(&manifest).expect("manifest json");
    m["files"]["events.ndjson"]["sha256"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    let tampered = serde_json::to_vec(&m).expect("reserialize");

    let (class, code) = expect_rejected(
        &repack(&[("manifest.json", &tampered), ("events.ndjson", &events)]),
        "a manifest whose recorded digest does not match its payload",
    );
    assert_eq!(
        (class, code),
        (ErrorClass::Integrity, ErrorCode::IntegrityManifestHash)
    );
}

#[test]
fn a_mutated_run_root_is_a_run_root_mismatch() {
    let bundle = valid_bundle(2);
    let (manifest, events) = unpack(&bundle);

    let mut m: serde_json::Value = serde_json::from_slice(&manifest).expect("manifest json");
    m["run_root"] = serde_json::json!(format!("sha256:{}", "1".repeat(64)));
    let tampered = serde_json::to_vec(&m).expect("reserialize");

    // Resealed so the events hash still matches: this isolates the chain root from the file digest.
    let resealed = reseal(&tampered, &events);
    let (class, code) = expect_rejected(
        &repack(&[("manifest.json", &resealed), ("events.ndjson", &events)]),
        "a manifest whose chain root does not match its events",
    );
    assert_eq!(
        (class, code),
        (ErrorClass::Integrity, ErrorCode::IntegrityRunRootMismatch)
    );
}

/// `verify.rs` documents check 9 as "ID Contract: event.id == run_id:seq". The id is outside the
/// per-event content hash, so nothing else catches a forged one once the manifest is resealed.
/// A documented check that does not run is the exact failure ADR-043 is about, and it sits on the
/// golden path.
#[test]
fn a_forged_event_id_is_rejected_by_the_id_contract() {
    let bundle = valid_bundle(3);
    let (manifest, events) = unpack(&bundle);

    // Equal length so nothing else can be the reason: only the trailing seq digit changes.
    let forged = String::from_utf8(events.clone())
        .expect("utf8")
        .replacen(
            "\"id\":\"run_verifier_property_0001:0\"",
            "\"id\":\"run_verifier_property_0001:9\"",
            1,
        )
        .into_bytes();
    assert_eq!(forged.len(), events.len(), "forgery must preserve length");
    assert_ne!(forged, events, "fixture must actually carry the id field");

    // Resealed: the container is internally consistent again, so the id contract is the only
    // remaining thing standing between a forged stream identity and a verified bundle.
    let resealed = reseal(&manifest, &forged);
    let (class, code) = expect_rejected(
        &repack(&[("manifest.json", &resealed), ("events.ndjson", &forged)]),
        "an event whose id does not match run_id:seq",
    );
    assert_eq!(
        (class, code),
        (ErrorClass::Contract, ErrorCode::ContractInvalidEvent)
    );
}

// ---------------------------------------------------------------------------
// Chain order
// ---------------------------------------------------------------------------

#[test]
fn reordering_the_chain_is_rejected_on_the_sequence_contract() {
    let bundle = valid_bundle(3);
    let (manifest, events) = unpack(&bundle);

    let mut lines: Vec<&[u8]> = events
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), 3, "fixture shape");
    lines.swap(0, 2);
    let mut reordered = Vec::new();
    for line in lines {
        reordered.extend_from_slice(line);
        reordered.push(b'\n');
    }

    // Resealed: without this the reorder trips the hash check and the sequence contract is never
    // reached, so the test would pass while proving nothing about ordering.
    let resealed = reseal(&manifest, &reordered);
    let (class, code) = expect_rejected(
        &repack(&[("manifest.json", &resealed), ("events.ndjson", &reordered)]),
        "an out-of-order chain",
    );
    // Pinned exactly rather than as "one of the sequence codes": swapping 0 and 2 puts `seq=2`
    // first, so the start-of-sequence check is the one that must fire. An allowance over both
    // codes would let a change in check order pass unnoticed, which is the regression this is for.
    assert_eq!(
        (class, code),
        (ErrorClass::Contract, ErrorCode::ContractSequenceStart)
    );
}

#[test]
fn dropping_the_last_event_is_rejected_rather_than_silently_accepted() {
    let bundle = valid_bundle(3);
    let (manifest, events) = unpack(&bundle);

    let mut lines: Vec<&[u8]> = events
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    lines.pop();
    let mut shortened = Vec::new();
    for line in lines {
        shortened.extend_from_slice(line);
        shortened.push(b'\n');
    }

    // Resealed so this is a truncated *chain*, not a corrupted file: the manifest still claims
    // three events, which is the claim the verifier has to catch.
    let resealed = reseal(&manifest, &shortened);
    let (class, code) = expect_rejected(
        &repack(&[("manifest.json", &resealed), ("events.ndjson", &shortened)]),
        "a chain with its last event removed",
    );
    // Pinned exactly for the same reason as the reordering case: the manifest still claims three
    // events, so the count mismatch is what must catch this. An either-class assertion would stay
    // green if the chain check stopped running and something else rejected the bundle instead.
    assert_eq!(
        (class, code),
        (ErrorClass::Contract, ErrorCode::ContractSequenceGap)
    );
}

// ---------------------------------------------------------------------------
// Container-level shapes
// ---------------------------------------------------------------------------

#[test]
fn a_duplicate_member_is_a_duplicate_file() {
    let bundle = valid_bundle(1);
    let (manifest, events) = unpack(&bundle);

    let (class, code) = expect_rejected(
        &repack(&[
            ("manifest.json", &manifest),
            ("events.ndjson", &events),
            ("events.ndjson", &events),
        ]),
        "a bundle carrying two events members",
    );
    assert_eq!(
        (class, code),
        (ErrorClass::Contract, ErrorCode::ContractDuplicateFile)
    );
}

#[test]
fn an_unexpected_member_is_refused_by_the_allowlist() {
    let bundle = valid_bundle(1);
    let (manifest, events) = unpack(&bundle);

    let (class, code) = expect_rejected(
        &repack(&[
            ("manifest.json", &manifest),
            ("events.ndjson", &events),
            ("extra.txt", b"anything"),
        ]),
        "a bundle carrying an unexpected member",
    );
    assert_eq!(
        (class, code),
        (ErrorClass::Contract, ErrorCode::ContractUnexpectedFile)
    );
}

/// Truncation is swept at every offset, not sampled, because the interesting failures sit on the
/// boundaries between gzip, tar and NDJSON framing and a stride can step over all of them. The
/// fixture is under a kilobyte, so the exhaustive sweep is cheaper than reasoning about which
/// offsets would have mattered.
#[test]
fn truncation_at_every_offset_fails_closed_and_never_panics() {
    let bundle = valid_bundle(3);
    assert!(bundle.len() > 32, "fixture must be long enough to sweep");

    for cut in 1..bundle.len() {
        let truncated = &bundle[..cut];
        match classify(truncated) {
            Ok(()) => panic!("a truncated bundle verified at cut={cut}"),
            Err((class, _code)) => assert_ne!(
                class,
                ErrorClass::Limits,
                "truncation must not read as a limit trip at cut={cut}"
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

/// The ceiling is exercised by tightening it around a bundle that is otherwise valid, rather than
/// by feeding in a large blob. A blob of the right size would be refused for its gzip header long
/// before the ceiling was reached, so it would prove nothing about the ceiling. Exact limit is
/// accepted and one byte less is refused, which is the boundary itself and not a value near it.
#[test]
fn the_byte_ceiling_accepts_exactly_the_limit_and_refuses_one_byte_less() {
    let bundle = valid_bundle(2);
    let exact = bundle.len() as u64;

    assert_eq!(
        classify_with(
            &bundle,
            VerifyLimits {
                max_bundle_bytes: exact,
                ..small_limits()
            }
        ),
        Ok(()),
        "a bundle of exactly the permitted size must verify"
    );

    let (class, code) = classify_with(
        &bundle,
        VerifyLimits {
            max_bundle_bytes: exact - 1,
            ..small_limits()
        },
    )
    .expect_err("one byte over the ceiling must not verify");
    assert_eq!(
        (class, code),
        (ErrorClass::Limits, ErrorCode::LimitBundleBytes)
    );
}

/// The decode ceiling is a separate axis from the byte ceiling: it bounds what the input expands
/// to, which is what a decompression bomb attacks. Pinned here by tightening it below the decoded
/// size of a valid bundle whose compressed form is comfortably within the byte ceiling, so only
/// the decode axis can be the reason. The bomb shape itself is covered by `assay-sim`.
#[test]
fn the_decode_ceiling_is_enforced_independently_of_the_byte_ceiling() {
    let bundle = valid_bundle(4);
    let limits = VerifyLimits {
        max_decode_bytes: 128,
        ..small_limits()
    };
    assert!(
        (bundle.len() as u64) < limits.max_bundle_bytes,
        "the compressed form must clear the byte ceiling, or this tests the wrong axis"
    );

    let (class, code) = classify_with(&bundle, limits)
        .expect_err("expansion past the decode ceiling must not verify");
    assert_eq!(
        (class, code),
        (ErrorClass::Limits, ErrorCode::LimitDecodeBytes)
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Seed corpus
// ---------------------------------------------------------------------------

/// Regenerates `fuzz/corpus/bundle_reader/`. Ignored by default so it never writes during a normal
/// test run; run it deliberately with
/// `cargo test -p assay-evidence --test verifier_fail_closed_properties -- --ignored`.
///
/// The seeds are generated here rather than committed as opaque blobs so a reviewer can see what
/// each one is and regenerate it. They are small, deterministic and carry no provenance: the only
/// content is the fixed test event above.
#[test]
#[ignore = "writes the fuzz seed corpus; run deliberately"]
fn generate_seed_corpus() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/corpus/bundle_reader");
    std::fs::create_dir_all(&dir).expect("corpus dir");

    let one = valid_bundle(1);
    let three = valid_bundle(3);
    let (manifest, events) = unpack(&three);

    let mutated_payload = String::from_utf8(events.clone())
        .expect("utf8")
        .replacen("aaaaaaaa", "aaaaaaab", 1)
        .into_bytes();

    let mut digest_tampered: serde_json::Value =
        serde_json::from_slice(&manifest).expect("manifest json");
    digest_tampered["files"]["events.ndjson"]["sha256"] =
        serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    let digest_tampered = serde_json::to_vec(&digest_tampered).expect("reserialize");

    let mut root_tampered: serde_json::Value =
        serde_json::from_slice(&manifest).expect("manifest json");
    root_tampered["run_root"] = serde_json::json!(format!("sha256:{}", "1".repeat(64)));
    let root_tampered = serde_json::to_vec(&root_tampered).expect("reserialize");

    let long_line = {
        let mut v = Vec::new();
        v.extend_from_slice(&events);
        v.extend_from_slice(&vec![b'x'; 9 << 10]);
        v.push(b'\n');
        v
    };

    // A high-ratio gzip: 8 KB on disk, 8 MiB decoded. Every other seed decodes to a few kilobytes,
    // so without this one the fuzzer has no compressible material to mutate at all.
    //
    // Named for what it is, not what it would be nice for it to be. It does NOT drive the verifier
    // past `max_decode_bytes`: measured through the real target API it returns
    // `Contract/ContractMissingFile`, because the decoded stream is zero blocks and the tar reader
    // reads that as an empty archive and stops. `high_ratio_gzip_stops_at_the_tar_layer` below
    // pins that outcome so this comment cannot quietly go stale.
    let high_ratio_gzip = {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        let chunk = vec![0u8; 1 << 20];
        for _ in 0..8 {
            encoder.write_all(&chunk).expect("write chunk");
        }
        encoder.finish().expect("finish")
    };
    assert!(
        decoded_len(&high_ratio_gzip) > 4 << 20,
        "the seed must at least decompress past the ceiling, even though the tar layer stops first"
    );

    let seeds: Vec<(&str, Vec<u8>)> = vec![
        ("valid-single-event", one.clone()),
        ("valid-three-events", three.clone()),
        ("truncated-half", three[..three.len() / 2].to_vec()),
        (
            "truncated-gzip-header",
            three[..8.min(three.len())].to_vec(),
        ),
        (
            "payload-mutated",
            repack(&[
                ("manifest.json", &manifest),
                ("events.ndjson", &mutated_payload),
            ]),
        ),
        (
            "digest-mutated",
            repack(&[
                ("manifest.json", &digest_tampered),
                ("events.ndjson", &events),
            ]),
        ),
        (
            "run-root-mutated",
            repack(&[
                ("manifest.json", &reseal(&root_tampered, &events)),
                ("events.ndjson", &events),
            ]),
        ),
        (
            "duplicate-member",
            repack(&[
                ("manifest.json", &manifest),
                ("events.ndjson", &events),
                ("events.ndjson", &events),
            ]),
        ),
        (
            "unexpected-member",
            repack(&[
                ("manifest.json", &manifest),
                ("events.ndjson", &events),
                ("extra.txt", b"anything"),
            ]),
        ),
        ("manifest-only", repack(&[("manifest.json", &manifest)])),
        (
            "wrong-member-order",
            repack(&[("events.ndjson", &events), ("manifest.json", &manifest)]),
        ),
        (
            "oversize-line",
            repack(&[
                ("manifest.json", &reseal(&manifest, &long_line)),
                ("events.ndjson", &long_line),
            ]),
        ),
        ("high-ratio-gzip", high_ratio_gzip),
    ];

    for (name, bytes) in &seeds {
        assert!(bytes.len() < 64 << 10, "seed {name} must stay small");
        std::fs::write(dir.join(name), bytes).expect("write seed");
    }
    eprintln!("wrote {} seeds to {}", seeds.len(), dir.display());
}

/// Pins where the high-ratio seed actually stops, and with it a limitation of this format worth
/// writing down: for a bundle-shaped archive the orthogonal per-file ceilings fire long before
/// `max_decode_bytes` can. The manifest is capped at `max_manifest_bytes` and the events member at
/// `max_events_bytes`, both checked against the declared tar header size before any content is
/// read, and any third member is refused by the allowlist. So a decompression bomb is stopped by
/// the file ceilings on this path, and `max_decode_bytes` is the backstop pinned separately by
/// `the_decode_ceiling_is_enforced_independently_of_the_byte_ceiling`.
///
/// Not established: whether some non-bundle tar shape, a GNU long-name entry for instance, can
/// make the reader consume past the decode ceiling. That is left as an open question rather than
/// claimed either way.
#[test]
fn high_ratio_gzip_stops_at_the_tar_layer_not_the_decode_ceiling() {
    let seed = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fuzz/corpus/bundle_reader/high-ratio-gzip"),
    )
    .expect("the checked-in seed must exist");

    assert!(
        decoded_len(&seed) > 4 << 20,
        "seed must still be high-ratio, or it has stopped being useful to the fuzzer"
    );
    let (class, code) = expect_rejected(&seed, "a high-ratio gzip of zero blocks");
    assert_eq!(
        (class, code),
        (ErrorClass::Contract, ErrorCode::ContractMissingFile),
        "if this changes, the seed's name and the limitation note above both need revisiting"
    );
}

/// The classification is the contract, so it has to be stable. An unstable code would make every
/// assertion above flaky and would make the fuzz corpus non-reproducible.
#[test]
fn classification_is_stable_across_repeated_runs() {
    let bundle = valid_bundle(3);
    let (manifest, events) = unpack(&bundle);
    let mutated = String::from_utf8(events)
        .expect("utf8")
        .replacen("aaaaaaaa", "aaaaaaab", 1)
        .into_bytes();
    let tampered = repack(&[("manifest.json", &manifest), ("events.ndjson", &mutated)]);

    let first = classify(&tampered);
    for _ in 0..16 {
        assert_eq!(
            classify(&tampered),
            first,
            "classification must be deterministic"
        );
    }
}
