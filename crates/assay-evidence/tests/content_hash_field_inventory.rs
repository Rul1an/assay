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
//! This test derives it, in two layers, because one is not enough. An exhaustive destructure of
//! `EvidenceEvent` breaks the *build* when a field is added, which is the only mechanism that
//! reaches a field serde does not emit — several carry `skip_serializing_if`, so a new optional
//! left `None` in a fixture would otherwise be invisible to BOUND/UNBOUND set checks. The same
//! exhaustive invocation also yields the structural field count, and `fully_populated_event()`
//! must serialize exactly that many keys: naming a new Option without populating it goes red.
//! On top of that, each emitted field is classified by *observing* whether mutating it moves the
//! content hash, and the observation is compared against a declaration a human has to write.
//!
//! So: adding a field fails compilation until it is named; naming it without classifying it fails
//! a test; leaving a skippable Option `None` in the fixture fails the structural-count test; and
//! classifying it wrongly fails a different test. That is the property the prose could not carry —
//! the list cannot silently disagree with the code.

use assay_evidence::crypto::id::{
    compute_content_hash, content_hash_bound_field_names, content_hash_scope,
    CONTENT_HASH_SCOPE_SCHEMA,
};
use assay_evidence::types::EvidenceEvent;
use serde_json::{json, Value};
use std::collections::BTreeSet;

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

/// Exhaustive `EvidenceEvent` destructure and structural field count from one invocation.
///
/// Adding a field to `EvidenceEvent` stops compilation until this call site names it. The returned
/// length is the expected serialized key count for a fully populated event: a new
/// `Option`+`skip_serializing_if` field named here but left `None` in `fully_populated_event()`
/// drops below that count and fails the structural assertion below. No `..` rest pattern.
macro_rules! evidence_event_structural_field_count {
    ($event:expr; $($field:ident),+ $(,)?) => {{
        #[allow(unused_variables)]
        let EvidenceEvent {
            $($field,)+
        } = $event;
        [$(stringify!($field)),+].len()
    }};
}

fn structural_field_count(event: &EvidenceEvent) -> usize {
    evidence_event_structural_field_count! {
        event;
        specversion,
        type_,
        source,
        id,
        time,
        data_content_type,
        subject,
        trace_parent,
        trace_state,
        run_id,
        seq,
        producer,
        producer_version,
        git_sha,
        policy_id,
        contains_pii,
        contains_secrets,
        content_hash,
        semantic_digest,
        digest_profile,
        payload,
    }
}

#[test]
fn every_event_field_is_classified_and_behaves_as_classified() {
    let event = fully_populated_event();
    let expected_keys = structural_field_count(&event);

    let serialized = serde_json::to_value(&event).expect("serialize");
    let object = serialized
        .as_object()
        .expect("event serializes to an object");
    assert_eq!(
        object.len(),
        expected_keys,
        "fully_populated_event() must serialize every EvidenceEvent field (structural count \
         from the exhaustive destructure). An Option left None under skip_serializing_if is \
         invisible to BOUND/UNBOUND set checks but drops this count"
    );

    // Baseline from the round-tripped event, not the in-memory one. If deserialization normalizes
    // anything, an in-memory baseline makes every *other* field's verdict wrong rather than
    // reporting the one field responsible.
    let round_tripped: EvidenceEvent =
        serde_json::from_value(serialized.clone()).expect("baseline round-trip");
    let baseline = compute_content_hash(&round_tripped).expect("baseline hash");

    let mut unclassified: Vec<String> = Vec::new();
    let mut wrongly_classified: Vec<String> = Vec::new();
    let mut inert_mutations: Vec<String> = Vec::new();

    for (key, value) in object {
        let mut mutated_object = object.clone();
        mutated_object.insert(key.clone(), mutate(key, value));
        let mutated: EvidenceEvent = serde_json::from_value(Value::Object(mutated_object))
            .unwrap_or_else(|e| {
                panic!("mutating {key} produced an event that will not parse: {e}")
            });

        // A mutation that does not survive the round trip would classify its field UNBOUND for the
        // wrong reason, and the failure message would then instruct someone to record that wrong
        // answer. Prove the input actually changed before reading anything into the output.
        if mutated == round_tripped {
            inert_mutations.push(format!(
                "{key}: the mutation did not survive deserialization, so this field's \
                 classification would be an artifact of the probe rather than an observation"
            ));
            continue;
        }

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
        inert_mutations.is_empty(),
        "the probe could not move these fields, so their classification would be measured on an \
         unchanged event. Teach `mutate()` to produce a genuinely different value for them before \
         trusting any verdict here:\n  {}",
        inert_mutations.join("\n  ")
    );
    assert!(
        unclassified.is_empty(),
        "EvidenceEvent carries fields this inventory does not classify. Add each to BOUND or \
         UNBOUND in this file with the reason, and if it lands in UNBOUND say so wherever the \
         attestation boundary is described (ADR-039 and `attestation.rs`), since a signature does \
         not cover it:\n  {}",
        unclassified.join("\n  ")
    );
    assert!(
        wrongly_classified.is_empty(),
        "the declared classification disagrees with what the content hash actually covers:\n  {}",
        wrongly_classified.join("\n  ")
    );
}

/// The declared names and the serialized names are the same set.
///
/// Set equality rather than a count: a rename plus a stale entry keeps the arity intact, and a
/// cardinality check would call that agreement. This catches a name that no longer exists and a
/// name that exists but is unlisted. Skippable Option fields left `None` in the fixture are
/// caught by the structural-count assertion against the exhaustive destructure above.
#[test]
fn the_inventory_names_match_the_serialized_names() {
    use std::collections::BTreeSet;

    let event = fully_populated_event();
    let serialized = serde_json::to_value(&event).expect("serialize");
    let emitted: BTreeSet<&str> = serialized
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    let declared: BTreeSet<&str> = BOUND.iter().chain(UNBOUND.iter()).copied().collect();

    let missing: Vec<&&str> = emitted.difference(&declared).collect();
    let stale: Vec<&&str> = declared.difference(&emitted).collect();

    assert!(
        missing.is_empty() && stale.is_empty(),
        "inventory and event disagree.\n  serialized but unlisted: {missing:?}\n  \
         listed but not serialized: {stale:?}"
    );
}

/// JCS key order for `ContentHashInput` with `subject` populated (RFC 8785 object key sort).
///
/// This is the order the public projection must return. Declaration order in `BOUND` is an
/// independent oracle, not the emission order.
const BOUND_JCS_ORDER: &[&str] = &["data", "datacontenttype", "specversion", "subject", "type"];

/// Production projection ↔ human `BOUND` ↔ observed hash movers (three-way; one shared function).
///
/// Non-claims (attributed): ADR-042 §3 (trust score / whole-action verdict);
/// UNBOUND inventory (producer identity via `source`); ADR-044 (archive verification);
/// preimage scope alone (completeness / truthfulness / tamper intent).
#[test]
fn production_bound_field_projection_matches_bound_and_observed_movers() {
    let projected = content_hash_bound_field_names();
    let expected: Vec<String> = BOUND_JCS_ORDER.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        projected, expected,
        "public projection must emit bound names in JCS key order with subject present"
    );

    let projected_set: BTreeSet<&str> = projected.iter().map(String::as_str).collect();
    let declared: BTreeSet<&str> = BOUND.iter().copied().collect();
    assert_eq!(
        projected_set, declared,
        "production projection must set-equal the independent BOUND declaration; \
         deriving BOUND from the projection alone would make the oracle circular"
    );

    // Observed movers: same probe as the exhaustive classifier, restricted to BOUND names.
    let event = fully_populated_event();
    let serialized = serde_json::to_value(&event).expect("serialize");
    let object = serialized
        .as_object()
        .expect("event serializes to an object");
    let round_tripped: EvidenceEvent =
        serde_json::from_value(serialized.clone()).expect("baseline round-trip");
    let baseline = compute_content_hash(&round_tripped).expect("baseline hash");

    let mut observed_movers: BTreeSet<&str> = BTreeSet::new();
    for (key, value) in object {
        let mut mutated_object = object.clone();
        mutated_object.insert(key.clone(), mutate(key, value));
        let mutated: EvidenceEvent = serde_json::from_value(Value::Object(mutated_object))
            .unwrap_or_else(|e| {
                panic!("mutating {key} produced an event that will not parse: {e}")
            });
        if mutated == round_tripped {
            continue;
        }
        if compute_content_hash(&mutated).expect("mutated hash") != baseline {
            // Leak to 'static via BOUND/UNBOUND tables only — keys are serialized names.
            let name = BOUND
                .iter()
                .chain(UNBOUND.iter())
                .copied()
                .find(|n| *n == key.as_str())
                .unwrap_or_else(|| panic!("mover {key} missing from inventory tables"));
            observed_movers.insert(name);
        }
    }

    assert_eq!(
        observed_movers, declared,
        "fields whose mutations move content_hash must set-equal BOUND and the production projection"
    );
}

/// False-green: emitting bound names from `ContentHashInput` with `subject: None` drops
/// `subject` under `skip_serializing_if`. The structural projection uses required fields so an
/// optional bound field cannot disappear from the public list when a sentinel is `None`.
#[test]
fn content_hash_scope_projection_does_not_omit_subject() {
    let projected = content_hash_bound_field_names();
    assert!(
        projected.iter().any(|name| name == "subject"),
        "subject omitted during introspection is a false-green: the bound set must include subject"
    );
    let expected: Vec<String> = BOUND_JCS_ORDER.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(projected, expected);
}

/// Value-level bite: mislabeling a separate integrity layer as content_hash coverage must fail
/// even when the key names look correct.
#[test]
fn separate_integrity_layers_state_semantic_separation() {
    let layers = content_hash_scope().separate_integrity_layers;
    for (name, value) in [
        ("events_file_digest", layers.events_file_digest),
        ("run_root", layers.run_root),
        (
            "archive_attestation_subject",
            layers.archive_attestation_subject,
        ),
    ] {
        assert!(
            value.contains("not bound by content_hash"),
            "{name}={value:?}: key presence alone is a false-green"
        );
        assert!(
            !value.contains("covered by this content_hash"),
            "{name} must not claim content_hash covers the layer"
        );
    }
}

/// Registered output identity — not an invented digest_profile string.
#[test]
fn content_hash_scope_uses_registered_schema_identity() {
    let scope = content_hash_scope();
    assert_eq!(scope.schema, CONTENT_HASH_SCOPE_SCHEMA);
    assert_eq!(CONTENT_HASH_SCOPE_SCHEMA, "assay.content_hash_scope.v1");
    assert_eq!(scope.applies_to, "reader_content_hash_recompute");
    assert_eq!(scope.not_reconciled_with, "manifest.algorithms");
    let json = serde_json::to_value(&scope).expect("serialize");
    assert!(json.get("digest_profile").is_none());
}
