//! For every bundle the writer refuses to emit, the verifier refuses to accept.
//!
//! The two ends of a format disagreeing about what a bundle is has a specific failure mode: a
//! producer other than `BundleWriter` emits something this repository's own writer would have
//! rejected, and it verifies anyway. Five such shapes existed — an empty bundle, an inconsistent
//! `source`, a `source` that is not a URI, and a blank line standing in for an event. Four were
//! found by enumerating the writer's refusals rather than by anyone reporting them, which is the
//! argument for a property over a set of patches.
//!
//! The mechanism is the exhaustive match in [`bundle_violating`]. A ninth [`StreamRule`] will not
//! compile until it has a case here, and the case is only satisfied by showing both ends reject.
//! An enum variant is not a test; the pairing is.

use assay_evidence::bundle::{
    verify_bundle_with_limits, BundleWriter, ErrorClass, StreamRule, VerifyLimits,
};
use assay_evidence::types::EvidenceEvent;
use serde_json::json;
use std::io::Cursor;

/// A bundle the writer would emit: three contiguous events, one run, one source.
fn sound_events() -> Vec<EvidenceEvent> {
    (0..3u64)
        .map(|seq| {
            EvidenceEvent::new(
                "assay.symmetry.probe",
                "urn:assay:symmetry",
                "run_symmetry_0001",
                seq,
                json!({"seq": seq}),
            )
        })
        .collect()
}

/// Hand-pack a bundle from events, sealing the manifest so only the rule under test is violated.
///
/// The writer cannot be used here — refusing these is the behaviour being tested — so the manifest
/// is built the way a foreign producer would build it: chain recomputed, digests re-pinned, counts
/// honest. Anything the verifier rejects is therefore the rule, not the packing.
fn pack(events: &[EvidenceEvent], blank_lines: usize) -> Vec<u8> {
    use sha2::{Digest, Sha256};

    let mut lines: Vec<String> = Vec::new();
    let mut hashes: Vec<String> = Vec::new();
    for e in events {
        let mut e = e.clone();
        let h = assay_evidence::crypto::id::compute_content_hash(&e).expect("hash");
        if e.content_hash.is_none() {
            e.content_hash = Some(h.clone());
        }
        hashes.push(e.content_hash.clone().unwrap());
        lines.push(serde_json::to_string(&e).expect("event json"));
    }
    let mut body = lines.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    for _ in 0..blank_lines {
        body.push('\n');
    }
    let body = body.into_bytes();

    let run_root = assay_evidence::crypto::id::compute_run_root(&hashes);
    let events_sha = format!("sha256:{}", hex::encode(Sha256::digest(&body)));
    let run_id = events
        .first()
        .map(|e| e.run_id.clone())
        .unwrap_or_else(|| "run_symmetry_0001".to_string());

    // Count lines the way a producer reading its own output would: one per event, plus the blanks
    // it believes it wrote. That is what makes a blank-line bundle internally consistent and
    // therefore a real test of the rule rather than of the count check.
    let event_count = events.len() + blank_lines;

    let manifest = json!({
        "schema_version": 1,
        "bundle_id": run_root,
        "run_id": run_id,
        "run_root": run_root,
        "event_count": event_count,
        "producer": {"name": "symmetry-test", "version": "0.0.0", "git": "0000000"},
        "algorithms": {
            "canon": "jcs-rfc8785",
            "hash": "sha256",
            "root": "sha256(concat(content_hash + \"\\n\"))"
        },
        "files": {"events.ndjson": {
            "path": "events.ndjson",
            "sha256": events_sha,
            "bytes": body.len()
        }}
    });
    let manifest_bytes = serde_json::to_vec(&manifest).expect("manifest json");

    let mut tar = tar::Builder::new(Vec::new());
    for (name, data) in [("manifest.json", &manifest_bytes), ("events.ndjson", &body)] {
        let mut h = tar::Header::new_gnu();
        h.set_path(name).expect("path");
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(0);
        h.set_cksum();
        tar.append(&h, data.as_slice()).expect("append");
    }
    let tar_bytes = tar.into_inner().expect("tar");

    use flate2::write::GzEncoder;
    use std::io::Write;
    let mut gz = GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar_bytes).expect("gz");
    gz.finish().expect("gz finish")
}

/// A bundle that violates exactly the named rule, and nothing else.
///
/// The match is exhaustive on purpose: adding a rule to the format breaks this function until
/// someone constructs the shape it forbids.
fn bundle_violating(rule: StreamRule) -> Vec<u8> {
    let mut events = sound_events();
    match rule {
        StreamRule::NonEmpty => return pack(&[], 0),
        StreamRule::SeqContiguousFromZero => {
            events[2].seq = 7;
            events[2].id = "run_symmetry_0001:7".into();
        }
        StreamRule::RunIdConsistent => {
            events[1].run_id = "run_symmetry_0002".into();
            events[1].id = "run_symmetry_0002:1".into();
        }
        StreamRule::SourceConsistent => events[1].source = "urn:assay:elsewhere".into(),
        StreamRule::SourceIsUri => {
            for e in &mut events {
                e.source = "not-a-uri".into();
            }
        }
        StreamRule::RunIdHasNoColon => {
            for (i, e) in events.iter_mut().enumerate() {
                e.run_id = "run:symmetry".into();
                e.id = format!("run:symmetry:{i}");
            }
        }
        StreamRule::ContentHashMatchesEvent => {
            events[1].content_hash = Some(format!("sha256:{}", "0".repeat(64)));
        }
        StreamRule::IdIsRunIdColonSeq => events[1].id = "run_symmetry_0001:9".into(),
    }
    pack(&events, 0)
}

/// The same shape, offered to the writer.
fn writer_refuses(rule: StreamRule) -> bool {
    let mut events = sound_events();
    match rule {
        StreamRule::NonEmpty => events.clear(),
        StreamRule::SeqContiguousFromZero => {
            events[2].seq = 7;
            events[2].id = "run_symmetry_0001:7".into();
        }
        StreamRule::RunIdConsistent => {
            events[1].run_id = "run_symmetry_0002".into();
            events[1].id = "run_symmetry_0002:1".into();
        }
        StreamRule::SourceConsistent => events[1].source = "urn:assay:elsewhere".into(),
        StreamRule::SourceIsUri => {
            for e in &mut events {
                e.source = "not-a-uri".into();
            }
        }
        StreamRule::RunIdHasNoColon => {
            for (i, e) in events.iter_mut().enumerate() {
                e.run_id = "run:symmetry".into();
                e.id = format!("run:symmetry:{i}");
            }
        }
        StreamRule::ContentHashMatchesEvent => {
            events[1].content_hash = Some(format!("sha256:{}", "0".repeat(64)));
        }
        StreamRule::IdIsRunIdColonSeq => events[1].id = "run_symmetry_0001:9".into(),
    }
    let mut w = BundleWriter::new(Cursor::new(Vec::new()));
    w.add_events(events);
    w.finish().is_err()
}

const ALL_RULES: &[StreamRule] = &[
    StreamRule::NonEmpty,
    StreamRule::SeqContiguousFromZero,
    StreamRule::RunIdConsistent,
    StreamRule::SourceConsistent,
    StreamRule::SourceIsUri,
    StreamRule::RunIdHasNoColon,
    StreamRule::ContentHashMatchesEvent,
    StreamRule::IdIsRunIdColonSeq,
];

#[test]
fn the_writer_refuses_every_rule_it_declares() {
    let missed: Vec<_> = ALL_RULES
        .iter()
        .filter(|r| !writer_refuses(**r))
        .map(|r| format!("{r:?}: {}", r.describe()))
        .collect();
    assert!(
        missed.is_empty(),
        "these rules are declared but the writer emits a bundle violating them, so the rule is a \
         description of nothing:\n  {}",
        missed.join("\n  ")
    );
}

#[test]
fn the_verifier_rejects_every_bundle_the_writer_refuses() {
    let accepted: Vec<_> = ALL_RULES
        .iter()
        .filter_map(|rule| {
            let bytes = bundle_violating(*rule);
            match verify_bundle_with_limits(bytes.as_slice(), VerifyLimits::default()) {
                Ok(_) => Some(format!("{rule:?}: {}", rule.describe())),
                Err(_) => None,
            }
        })
        .collect();
    assert!(
        accepted.is_empty(),
        "the verifier accepted bundles the writer refuses to emit. Each of these is a shape no \
         producer in this repository can create and every consumer will trust:\n  {}",
        accepted.join("\n  ")
    );
}

/// A blank line is not a rule violation an event can carry — it is a shape of the file — so it sits
/// outside the enum and needs its own case. The writer emits one line per event and never a blank,
/// and the verifier counted blank lines toward `event_count` while they contributed no content
/// hash, so a padded bundle satisfied both the count check and the chain by measuring different
/// things.
#[test]
fn a_blank_line_is_not_an_event() {
    let bytes = pack(&sound_events(), 5);
    let err = verify_bundle_with_limits(bytes.as_slice(), VerifyLimits::default())
        .expect_err("a blank-padded bundle must be rejected");
    assert!(
        err.to_string().contains("Blank line"),
        "expected the blank line to be named, got: {err}"
    );
}

/// The property is only worth anything if a sound bundle still verifies. Without this, rejecting
/// everything would pass every assertion above.
#[test]
fn a_sound_bundle_still_verifies() {
    let bytes = pack(&sound_events(), 0);
    let result = verify_bundle_with_limits(bytes.as_slice(), VerifyLimits::default())
        .expect("a bundle satisfying every rule must verify");
    assert_eq!(result.event_count, 3);
}

/// The new rejections are Contract failures, not Integrity ones: the bytes are intact and the
/// chain recomputes: what fails is the format contract. Classifying them as Integrity would tell
/// an operator to suspect tampering when the answer is a producer that does not follow the format.
#[test]
fn the_new_rejections_are_contract_failures() {
    for rule in [
        StreamRule::NonEmpty,
        StreamRule::SourceConsistent,
        StreamRule::SourceIsUri,
    ] {
        let bytes = bundle_violating(rule);
        let err = verify_bundle_with_limits(bytes.as_slice(), VerifyLimits::default())
            .expect_err("must reject");
        let text = format!("{err}");
        assert!(
            text.starts_with(&format!("{:?}", ErrorClass::Contract)),
            "{rule:?} should be a Contract failure, got: {text}"
        );
    }
}
