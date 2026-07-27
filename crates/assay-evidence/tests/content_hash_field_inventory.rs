//! Every field of `EvidenceEvent` is either bound by the content hash or declared unbound.
//!
//! The content-hash input is deliberately a subset of the event: excluding `time`, stream identity
//! and producer metadata is what lets a bundle be re-exported at a different moment by a different
//! producer and keep the same hash, which the profile requires and the clean-room conformance pack
//! depends on. That subset is correct. What was missing is a mechanism that keeps the *inventory*
//! of it honest.
//!
//! Prose lists went stale the way prose lists do. `crypto/id.rs` enumerated the excluded fields and
//! omitted `source`, `semantic_digest` and `digest_profile`; `source` is the CloudEvents field that
//! says which system produced the stream, so a reader consulting that list would have concluded the
//! chain binds something it does not. Nothing caught it because nothing derived the list from the
//! type.
//!
//! This test derives it. Each field is classified by *observing* whether mutating it moves the
//! content hash, and the observation is compared against a declaration that a human has to write.
//! Adding a field to `EvidenceEvent` therefore fails this test until someone states which side it
//! is on, and naming the wrong side fails it too. That is the property the prose could not carry:
//! the list cannot silently disagree with the code.

use assay_evidence::crypto::id::compute_content_hash;
use assay_evidence::types::EvidenceEvent;
use serde_json::{json, Value};

/// Fields the content hash covers. Mutating any of these MUST change the hash.
const BOUND: &[&str] = &["specversion", "type", "datacontenttype", "subject", "data"];

/// Fields the content hash deliberately does not cover, with the reason it is safe.
///
/// "Safe" means: a re-export may legitimately differ here, so binding it would break the
/// determinism the profile requires. It does NOT mean the field is unimportant — several of these
/// are exactly what an attestation consumer would assume a signature covers, which is why they are
/// enumerated here rather than left implicit.
const UNBOUND: &[&str] = &[
    // Self-referential: the hash cannot cover its own output.
    "assaycontenthash",
    // Derived from run_id + seq, so covered transitively by the id contract, not by the hash.
    "id",
    // Re-export happens at a different moment.
    "time",
    // Stream identity: the same content can be replayed under a new run.
    "assayrunid",
    "assayseq",
    // Provenance of the packaging, not of the content.
    "assayproducer",
    "assayproducerversion",
    "assaygit",
    // Operational metadata.
    "traceparent",
    "tracestate",
    "assaypolicyid",
    // Privacy classification: a judgement about the payload, not the payload.
    "assaypii",
    "assaysecrets",
    // Digest sidecars: they describe the payload under another canonicalization.
    "assaysemanticdigest",
    "assaydigestprofile",
    // The producing system. Unbound so a re-export by another system keeps the hash, which means
    // an attestation over the chain does NOT establish who produced the events.
    "source",
];

/// An event with every optional field populated, so serde emits the complete key set.
fn fully_populated_event() -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        "assay.inventory.probe",
        "urn:assay:inventory",
        "run_inventory_0001",
        0,
        json!({"payload": "value"}),
    );
    event.subject = Some("urn:assay:subject".to_string());
    event.trace_parent =
        Some("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".to_string());
    event.trace_state = Some("assay=1".to_string());
    event.policy_id = Some("policy-inventory".to_string());
    event.semantic_digest = Some("sha256:".to_string() + &"a".repeat(64));
    event.digest_profile = Some("jcs-rfc8785".to_string());
    event.content_hash = Some("sha256:".to_string() + &"b".repeat(64));
    event
}

/// Change a JSON value into a different value of the same shape, so the mutated event still
/// deserializes and only the field under test differs.
fn mutate(key: &str, value: &Value) -> Value {
    match value {
        Value::Bool(b) => json!(!b),
        Value::Number(n) => json!(n.as_u64().unwrap_or(0) + 1),
        Value::Object(_) | Value::Array(_) => json!({"mutated": true}),
        Value::String(s) => {
            // `time` has to stay an RFC 3339 timestamp or the event will not parse, which would
            // make the test measure deserialization instead of the hash.
            if key == "time" {
                json!("2031-02-03T04:05:06Z")
            } else {
                json!(format!("{s}-mutated"))
            }
        }
        Value::Null => json!("mutated"),
    }
}

#[test]
fn every_event_field_is_classified_and_behaves_as_classified() {
    let event = fully_populated_event();
    let baseline = compute_content_hash(&event).expect("baseline hash");

    let serialized = serde_json::to_value(&event).expect("serialize");
    let object = serialized
        .as_object()
        .expect("event serializes to an object");

    let mut unclassified: Vec<String> = Vec::new();
    let mut wrongly_classified: Vec<String> = Vec::new();

    for (key, value) in object {
        let mut mutated_object = object.clone();
        mutated_object.insert(key.clone(), mutate(key, value));
        let mutated: EvidenceEvent = serde_json::from_value(Value::Object(mutated_object))
            .unwrap_or_else(|e| {
                panic!("mutating {key} produced an event that will not parse: {e}")
            });

        let hash_moved = compute_content_hash(&mutated).expect("mutated hash") != baseline;

        let declared_bound = BOUND.contains(&key.as_str());
        let declared_unbound = UNBOUND.contains(&key.as_str());

        if !declared_bound && !declared_unbound {
            unclassified.push(format!(
                "{key}: not in BOUND or UNBOUND (observed: mutating it {} the content hash)",
                if hash_moved {
                    "changes"
                } else {
                    "does not change"
                }
            ));
            continue;
        }
        if declared_bound && declared_unbound {
            wrongly_classified.push(format!("{key}: declared in both BOUND and UNBOUND"));
            continue;
        }
        if declared_bound != hash_moved {
            wrongly_classified.push(format!(
                "{key}: declared {}, but mutating it {} the content hash",
                if declared_bound { "BOUND" } else { "UNBOUND" },
                if hash_moved {
                    "changes"
                } else {
                    "does not change"
                }
            ));
        }
    }

    assert!(
        unclassified.is_empty(),
        "EvidenceEvent carries fields this inventory does not classify. Add each to BOUND or \
         UNBOUND in this file, with the reason, and say so in the attestation boundary docs if it \
         lands in UNBOUND:\n  {}",
        unclassified.join("\n  ")
    );
    assert!(
        wrongly_classified.is_empty(),
        "the declared classification disagrees with what the content hash actually covers:\n  {}",
        wrongly_classified.join("\n  ")
    );
}

/// The inventory only means something if it covers the whole type. A field that serde never emits
/// cannot be probed above, so pin the count too: `EvidenceEvent` has no skipped fields today, and
/// adding one would hide it from the classification.
#[test]
fn the_inventory_covers_every_serialized_field() {
    let event = fully_populated_event();
    let serialized = serde_json::to_value(&event).expect("serialize");
    let keys: Vec<&String> = serialized.as_object().expect("object").keys().collect();

    assert_eq!(
        keys.len(),
        BOUND.len() + UNBOUND.len(),
        "the inventory lists {} fields but a fully populated event serializes {}: {:?}",
        BOUND.len() + UNBOUND.len(),
        keys.len(),
        keys
    );
}
