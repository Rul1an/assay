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

/// The profile makes `bundle_id` normative: "the manifest's `run_root` and `bundle_id` MUST both
/// equal it" (`docs/profiles/privileged-mcp-action/v0.md:126`). Nothing derives one from the other
/// at read time, so without this check the two can disagree and the bundle still verifies — a
/// normative MUST with no mechanism behind it. Ordered after the chain check so a mutated
/// `run_root` still fails as a root mismatch; this fires only when the chain is sound and the
/// second copy of it is not.
#[test]
fn a_bundle_id_that_disagrees_with_the_chain_root_is_rejected() {
    let bundle = valid_bundle(2);
    let (manifest, events) = unpack(&bundle);

    let mut m: serde_json::Value = serde_json::from_slice(&manifest).expect("manifest json");
    assert_eq!(
        m["bundle_id"], m["run_root"],
        "the writer must emit them equal, or this test proves nothing about the verifier"
    );
    m["bundle_id"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    let tampered = serde_json::to_vec(&m).expect("reserialize");

    // No reseal: only the manifest changed, and its recorded events digest still matches.
    let (class, code) = expect_rejected(
        &repack(&[("manifest.json", &tampered), ("events.ndjson", &events)]),
        "a manifest whose bundle_id does not equal its chain root",
    );
    assert_eq!(
        (class, code),
        (ErrorClass::Contract, ErrorCode::ContractBundleIdMismatch)
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

/// The other half of the ID contract, and the reason the writer bans colons in `run_id`
/// (`write.rs`, "run_id cannot contain colons"): `id` is `run_id:seq`, so a colon inside `run_id`
/// makes the split ambiguous. `run:test` with `seq=0` yields `run:test:0`, which reads equally
/// well as `run_id="run"`, `seq="test:0"`. Without this the verifier accepts a bundle its own
/// writer refuses to produce, so the two ends of the format disagree about what is valid.
#[test]
fn a_run_id_containing_a_colon_is_rejected() {
    let bundle = valid_bundle(2);
    let (manifest, events) = unpack(&bundle);

    // Equal length so nothing else can be the reason: one underscore becomes a colon, in the
    // manifest and in every event, including inside each `id` so the equality check still holds.
    let swap = |s: &[u8]| -> Vec<u8> {
        String::from_utf8(s.to_vec())
            .expect("utf8")
            .replace("run_verifier_property_0001", "run_verifier_property:0001")
            .into_bytes()
    };
    let events_colon = swap(&events);
    let manifest_colon = swap(&manifest);
    assert_eq!(events_colon.len(), events.len(), "must preserve length");
    assert_ne!(events_colon, events, "fixture must carry the run_id");

    let resealed = reseal(&manifest_colon, &events_colon);
    let (class, code) = expect_rejected(
        &repack(&[
            ("manifest.json", &resealed),
            ("events.ndjson", &events_colon),
        ]),
        "a run_id containing a colon",
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

    // The shape that does reach the decode ceiling. A PAX extended header is consumed by the tar
    // reader itself, before any entry is handed to the verifier, so the per-file ceilings never
    // get a chance to fire on it: `max_manifest_bytes` and `max_events_bytes` are checked against
    // a member's declared size, and this is not a member. Declaring 5 MiB of extended-header data
    // therefore forces the decoded stream past `max_decode_bytes` and `LimitDecodeBytes` is the
    // only thing standing in the way. Compresses to a few kilobytes because the payload is zeros.
    let pax_decode_bomb = {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        {
            let mut builder = tar::Builder::new(&mut encoder);
            let mut header = tar::Header::new_ustar();
            header.set_entry_type(tar::EntryType::XHeader);
            header
                .set_path("PaxHeaders.0/manifest.json")
                .expect("pax path");
            header.set_size(5 << 20);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_cksum();
            builder
                .append(&header, std::io::repeat(0u8).take(5 << 20))
                .expect("append pax header");
            builder.finish().expect("finish tar");
        }
        encoder.finish().expect("finish gzip")
    };

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
        ("pax-header-past-decode-ceiling", pax_decode_bomb),
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
/// A PAX extended header used to be the one shape that reached it, because the tar reader consumed
/// the record before handing the verifier a member. Members are now walked raw and an extension
/// record is refused at its own header, pinned by
/// `a_pax_extended_header_is_refused_before_its_body_is_decoded` below.
#[test]
fn high_ratio_gzip_stops_at_the_tar_layer_not_the_decode_ceiling() {
    let seed = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fuzz/corpus/bundle_reader/high-ratio-gzip"),
    )
    .expect("the checked-in seed must exist");

    assert!(
        decoded_len(&seed) as u64 > small_limits().max_decode_bytes,
        "seed must still be high-ratio, or it has stopped being useful to the fuzzer"
    );
    let (class, code) = expect_rejected(&seed, "a high-ratio gzip of zero blocks");
    assert_eq!(
        (class, code),
        (ErrorClass::Contract, ErrorCode::ContractMissingFile),
        "if this changes, the seed's name and the limitation note above both need revisiting"
    );
}

/// A PAX extended header declaring more than `max_decode_bytes` is refused at its own header,
/// before any of its body is decoded. It used to be consumed inside the tar reader, where only the
/// decode ceiling could stop it. Walking the archive raw hands the record to the verifier, which
/// refuses every extension record as a member that is not a plain file.
#[test]
fn a_pax_extended_header_is_refused_before_its_body_is_decoded() {
    let seed = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fuzz/corpus/bundle_reader/pax-header-past-decode-ceiling"),
    )
    .expect("the checked-in seed must exist");

    assert!(
        (seed.len() as u64) < small_limits().max_bundle_bytes,
        "the compressed form must clear the byte ceiling, or this tests the wrong axis"
    );
    // Guards this test's own premise: the record declares more than the decode ceiling, so a
    // refusal in the tar layer shows the body was never decoded rather than that it was small.
    assert!(
        decoded_len(&seed) as u64 > small_limits().max_decode_bytes,
        "seed must still declare more than the decode ceiling, or it tests a different path"
    );
    let (class, code) = expect_rejected(&seed, "a PAX header declaring more than the decode limit");
    assert_eq!(
        (class, code),
        (ErrorClass::Integrity, ErrorCode::IntegrityTar)
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

// ---------------------------------------------------------------------------
// Parser differentials: the archive another reader sees must be the one that was verified
// ---------------------------------------------------------------------------

fn pax_record(key: &str, value: &str) -> Vec<u8> {
    let body = format!(" {key}={value}\n");
    let mut len = body.len() + 1;
    loop {
        let record = format!("{len}{body}");
        if record.len() == len {
            return record.into_bytes();
        }
        len = record.len();
    }
}

/// A header whose name and size are written as given, bypassing `set_path`, which refuses the
/// shapes these tests need.
fn raw_header(name: &[u8], size: u64, kind: tar::EntryType) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.as_gnu_mut().expect("gnu header").name[..name.len()].copy_from_slice(name);
    header.set_entry_type(kind);
    header.set_size(size);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_cksum();
    header
}

/// A rewrite of one member header, applied before its checksum is recomputed.
type HeaderEdit = fn(&mut [u8; 512]);

/// An extension record placed before the member under test, whose own header then claims
/// `name` and `size`. The member's bytes are always written in full.
struct Record<'a> {
    kind: tar::EntryType,
    body: Vec<u8>,
    name: &'a str,
    size: u64,
}

/// Repack a valid bundle from plain headers. `edit` may rewrite the header of `target` (the
/// checksum is recomputed afterwards), and `record`, if any, is placed before it.
fn repack_members(
    manifest: &[u8],
    events: &[u8],
    target: &str,
    record: Option<Record<'_>>,
    edit: impl Fn(&mut [u8; 512]),
) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        for (path, content) in [("manifest.json", manifest), ("events.ndjson", events)] {
            let len = content.len() as u64;
            if path != target {
                let header = raw_header(path.as_bytes(), len, tar::EntryType::Regular);
                builder.append(&header, content).expect("append member");
                continue;
            }
            let (name, size) = match &record {
                Some(record) => {
                    let record_header =
                        raw_header(b"PaxHeaders/member", record.body.len() as u64, record.kind);
                    builder
                        .append(&record_header, record.body.as_slice())
                        .expect("append extension record");
                    (record.name, record.size)
                }
                None => (path, len),
            };
            let mut header = raw_header(name.as_bytes(), size, tar::EntryType::Regular);
            edit(header.as_mut_bytes());
            header.set_cksum();
            builder.append(&header, content).expect("append member");
        }
        builder.finish().expect("finish tar");
    }
    encoder.finish().expect("finish gzip")
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes).expect("gzip write");
    encoder.finish().expect("gzip finish")
}

fn decoded_tar(bundle: &[u8]) -> Vec<u8> {
    let mut tar = Vec::new();
    GzDecoder::new(Cursor::new(bundle))
        .read_to_end(&mut tar)
        .expect("valid gzip");
    tar
}

const TARGETS: [&str; 2] = ["manifest.json", "events.ndjson"];

/// The control for the differential tests below. The writer emits the old-style regular type
/// `'\0'`; `'0'` means the same, and zero padding after the archive is what a well-formed archive
/// carries. If these stop verifying, the checks below have become "refuse anything unusual".
#[test]
fn plain_headers_of_either_regular_type_and_zero_padding_still_verify() {
    let (manifest, events) = unpack(&valid_bundle(3));
    assert_eq!(
        decoded_tar(&valid_bundle(3))[156],
        0,
        "the writer emits '\\0'"
    );
    for target in TARGETS {
        let bundle = repack_members(&manifest, &events, target, None, |_| {});
        assert_eq!(classify(&bundle), Ok(()), "plain '0' header on {target}");
        // libarchive ends the size with a space where this writer uses a NUL.
        let bundle = repack_members(&manifest, &events, target, None, |h| h[135] = b' ');
        assert_eq!(
            classify(&bundle),
            Ok(()),
            "space-terminated size on {target}"
        );
    }
    let mut tar = decoded_tar(&valid_bundle(3));
    tar.extend_from_slice(&[0u8; 10240]);
    assert_eq!(
        classify(&gzip(&tar)),
        Ok(()),
        "zero padding after the archive"
    );
}

/// Readers disagree on extension records: which of two wins, what a malformed one falls back
/// to, whether a PAX name beats a GNU one. Comparing the tar crate's reading with the header
/// only asks the tar crate, so every record is refused, including one that agrees.
#[test]
fn every_extension_record_is_refused_even_one_that_agrees() {
    let (manifest, events) = unpack(&valid_bundle(3));
    for target in TARGETS {
        let len = if target == "manifest.json" {
            manifest.len()
        } else {
            events.len()
        } as u64;
        let pax = |body: Vec<u8>, name, size| Record {
            kind: tar::EntryType::XHeader,
            body,
            name,
            size,
        };
        let cases = [
            (
                "agreeing PAX size",
                pax(pax_record("size", &len.to_string()), target, len),
            ),
            (
                "PAX size 0",
                pax(pax_record("size", &len.to_string()), target, 0),
            ),
            (
                "PAX size len-1",
                pax(pax_record("size", &len.to_string()), target, len - 1),
            ),
            (
                "PAX size len+1",
                pax(pax_record("size", &len.to_string()), target, len + 1),
            ),
            (
                "PAX path",
                pax(pax_record("path", target), "notes.txt", len),
            ),
            (
                "two PAX sizes, first one true",
                pax(
                    [
                        pax_record("size", &len.to_string()),
                        pax_record("size", "10"),
                    ]
                    .concat(),
                    target,
                    len,
                ),
            ),
            (
                "two PAX paths, first one true",
                pax(
                    [pax_record("path", target), pax_record("path", "../evil")].concat(),
                    target,
                    len,
                ),
            ),
            (
                "unparsable PAX size",
                pax(pax_record("size", "1_0"), target, len),
            ),
            (
                "PAX global header",
                Record {
                    kind: tar::EntryType::XGlobalHeader,
                    body: pax_record("path", "../evil"),
                    name: target,
                    size: len,
                },
            ),
            (
                "GNU long name",
                Record {
                    kind: tar::EntryType::GNULongName,
                    body: [target.as_bytes(), b"\0"].concat(),
                    name: "notes.txt",
                    size: len,
                },
            ),
        ];
        for (what, record) in cases {
            let bundle = repack_members(&manifest, &events, target, Some(record), |_| {});
            assert_eq!(
                expect_rejected(&bundle, what),
                (ErrorClass::Integrity, ErrorCode::IntegrityTar),
                "{what} before {target}"
            );
        }
    }
}

/// A link, directory or sparse file carrying data: other readers unpack a link or nothing,
/// where the tar crate reads the data as the member.
#[test]
fn a_member_that_is_not_a_regular_file_is_refused() {
    let (manifest, events) = unpack(&valid_bundle(3));
    for target in TARGETS {
        for kind in [b'1', b'2', b'5', b'7', b'S', b'x', b'g', b'L'] {
            let bundle = repack_members(&manifest, &events, target, None, |h| h[156] = kind);
            assert_eq!(
                expect_rejected(&bundle, "a member that is not a regular file"),
                (ErrorClass::Integrity, ErrorCode::IntegrityTar),
                "type {:?} on {target}",
                kind as char
            );
        }
    }
}

/// Python and Go read bytes 345 onwards of a GNU header as a name prefix, where the tar crate
/// ignores them; a link name on a regular file is noise no reader should be asked to resolve.
#[test]
fn a_link_name_or_anything_from_the_name_prefix_on_is_refused() {
    let (manifest, events) = unpack(&valid_bundle(3));
    let edits: [(&str, HeaderEdit); 4] = [
        ("link name", |h| h[157..160].copy_from_slice(b"etc")),
        ("name prefix", |h| {
            h[345..355].copy_from_slice(b"../../evil")
        }),
        ("star trailer", |h| h[508..512].copy_from_slice(b"tar\0")),
        ("padding", |h| h[511] = 1),
    ];
    for target in TARGETS {
        for (what, edit) in edits {
            let bundle = repack_members(&manifest, &events, target, None, edit);
            assert_eq!(
                expect_rejected(&bundle, what),
                (ErrorClass::Integrity, ErrorCode::IntegrityTar),
                "{what} on {target}"
            );
        }
    }
}

/// An old-style regular file whose name ends in a slash is a directory to Python and Go. The
/// allowlist would refuse the name too, but in another class; this pins the header rule.
#[test]
fn a_member_name_ending_in_a_slash_is_refused() {
    let (manifest, events) = unpack(&valid_bundle(3));
    for target in TARGETS {
        let bundle = repack_members(&manifest, &events, target, None, |h| {
            h[156] = 0;
            h[target.len()] = b'/';
        });
        assert_eq!(
            expect_rejected(&bundle, "a name ending in a slash"),
            (ErrorClass::Integrity, ErrorCode::IntegrityTar),
            "{target}/"
        );
    }
}

/// The tar crate trims spaces around a size and reads base-256; Python also accepts separators
/// the tar crate refuses. Only the POSIX form is accepted: eleven octal digits ended by a NUL or a
/// space. Every variant here still parses to the member's true length in the tar crate.
#[test]
fn a_size_not_written_as_eleven_octal_digits_is_refused() {
    let (manifest, events) = unpack(&valid_bundle(3));
    for target in TARGETS {
        let len = if target == "manifest.json" {
            manifest.len()
        } else {
            events.len()
        } as u64;
        let mut base256 = [0u8; 12];
        base256[0] = 0x80;
        base256[4..].copy_from_slice(&len.to_be_bytes());
        let variants: [(&str, Vec<u8>); 3] = [
            ("leading spaces", format!("{len:>11o}\0").into_bytes()),
            (
                "twelve digits, no terminator",
                format!("{len:012o}").into_bytes(),
            ),
            ("base-256", base256.to_vec()),
        ];
        for (what, field) in variants {
            let bundle = repack_members(&manifest, &events, target, None, |h| {
                h[124..136].copy_from_slice(&field)
            });
            assert_eq!(
                expect_rejected(&bundle, what),
                (ErrorClass::Integrity, ErrorCode::IntegrityTar),
                "{what} size on {target}"
            );
        }
    }
}

/// Bytes after the end-of-archive marker are tar padding and must be zeros. Anything else is
/// content this verifier never parsed but a reader that skips zero blocks would.
#[test]
fn non_zero_data_after_the_end_of_the_archive_is_refused() {
    let mut tar = decoded_tar(&valid_bundle(3));
    tar.extend_from_slice(b"bytes after the archive");
    assert_eq!(
        expect_rejected(&gzip(&tar), "non-zero data after the end of the archive"),
        (ErrorClass::Integrity, ErrorCode::IntegrityTar)
    );
}

/// The verifier decodes one gzip member. A second member, or any other trailing bytes, is data
/// it never looked at and a reader that decodes concatenated members would unpack.
#[test]
fn data_after_the_gzip_member_is_refused() {
    let mut second_member = valid_bundle(3);
    second_member.extend_from_slice(&gzip(b"a second member"));
    assert_eq!(
        expect_rejected(&second_member, "a second gzip member"),
        (ErrorClass::Integrity, ErrorCode::IntegrityGzip)
    );
    let mut trailing = valid_bundle(3);
    trailing.extend_from_slice(b"trailing bytes");
    assert_eq!(
        expect_rejected(&trailing, "trailing bytes after the gzip member"),
        (ErrorClass::Integrity, ErrorCode::IntegrityGzip)
    );
}
