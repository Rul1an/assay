//! ADR-043 section 2 claim-vocabulary backstop for `EvidenceEvent` envelopes.
//!
//! This test covers the serialized top-level envelope shape. It does not establish that
//! `assay-mcp-server` emits an `EvidenceEvent` on a live path, and the shared word list remains a
//! backstop rather than a certificate. Caller/reflected strings and nested `data` are preserved by
//! the real event before being neutralized for the Assay-authored-value scan.

#[path = "../src/claims_backstop.rs"]
mod claims_backstop;

use assay_evidence::{
    crypto::id::compute_content_hash,
    types::{EvidenceEvent, ProducerMeta},
};
use claims_backstop::assert_no_unearned_status;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

const ASSAY_AUTHORED_OR_NON_TEXT_KEYS: [&str; 11] = [
    "specversion",
    "time",
    "datacontenttype",
    "assayseq",
    "assayproducer",
    "assayproducerversion",
    "assaygit",
    "assaypii",
    "assaysecrets",
    "assaycontenthash",
    "assaysemanticdigest",
];

const CALLER_OR_REFLECTED_KEYS: [&str; 10] = [
    "type",
    "source",
    "id",
    "subject",
    "traceparent",
    "tracestate",
    "assayrunid",
    "assaypolicyid",
    "assaydigestprofile",
    "data",
];

const PRODUCER_OVERRIDE_KEYS: [&str; 3] = ["assayproducer", "assayproducerversion", "assaygit"];

fn fully_populated_hostile_event() -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        "certified.partner.event",
        "urn:approved:source",
        "compliant-run",
        7,
        json!({
            "certification": "caller payload",
            "nested": {"status": "accredited"}
        }),
    )
    .with_subject("endorsed-subject")
    .with_trace("00-certified-traceparent")
    .with_policy_id("partner-policy")
    .with_privacy(true, true)
    .with_semantic_digest(&[], "certified-profile")
    .expect("semantic digest");
    event.trace_state = Some("approved=1".into());
    event.content_hash = Some(compute_content_hash(&event).expect("content hash"));
    event
}

fn assert_structural_partition(event: &EvidenceEvent, serialized: &Value) {
    // No rest pattern: adding a field stops compilation until this classification is updated.
    let EvidenceEvent {
        specversion: _,
        type_: _,
        source: _,
        id: _,
        time: _,
        data_content_type: _,
        subject: _,
        trace_parent: _,
        trace_state: _,
        run_id: _,
        seq: _,
        producer: _,
        producer_version: _,
        git_sha: _,
        policy_id: _,
        contains_pii: _,
        contains_secrets: _,
        content_hash: _,
        semantic_digest: _,
        digest_profile: _,
        payload: _,
    } = event;

    let actual = serialized
        .as_object()
        .expect("EvidenceEvent serializes as an object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let authored = ASSAY_AUTHORED_OR_NON_TEXT_KEYS
        .into_iter()
        .collect::<BTreeSet<_>>();
    let reflected = CALLER_OR_REFLECTED_KEYS
        .into_iter()
        .collect::<BTreeSet<_>>();
    let overlap = authored
        .intersection(&reflected)
        .copied()
        .collect::<Vec<_>>();
    assert!(
        overlap.is_empty(),
        "each serialized field must have exactly one default classification; overlap: {overlap:?}"
    );

    let classified = authored
        .into_iter()
        .chain(reflected)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual, classified,
        "every serialized field must be classified"
    );
}

fn assay_authored_projection(event: &EvidenceEvent) -> Value {
    projection_without(event, &CALLER_OR_REFLECTED_KEYS)
}

fn projection_without(event: &EvidenceEvent, keys: &[&str]) -> Value {
    let mut serialized = serde_json::to_value(event).expect("serialize EvidenceEvent");
    assert_structural_partition(event, &serialized);
    let object = serialized
        .as_object_mut()
        .expect("EvidenceEvent serializes as an object");

    for key in keys {
        object.insert(key.to_string(), Value::Null);
    }
    serialized
}

#[test]
fn evidence_event_scans_only_assay_authored_top_level_values() {
    let event = fully_populated_hostile_event();
    let serialized = serde_json::to_value(&event).expect("serialize EvidenceEvent");

    assert_eq!(serialized["type"], "certified.partner.event");
    assert_eq!(serialized["source"], "urn:approved:source");
    assert_eq!(serialized["id"], "compliant-run:7");
    assert_eq!(
        serialized["id"],
        format!(
            "{}:{}",
            serialized["assayrunid"].as_str().expect("run id"),
            serialized["assayseq"].as_u64().expect("sequence")
        )
    );
    assert_eq!(serialized["subject"], "endorsed-subject");
    assert_eq!(serialized["data"]["nested"]["status"], "accredited");

    assert!(
        catch_unwind(AssertUnwindSafe(|| assert_no_unearned_status(
            "raw hostile EvidenceEvent",
            &serialized
        )))
        .is_err(),
        "the raw hostile control must prove that the observation channel is live"
    );

    assert_no_unearned_status(
        "EvidenceEvent Assay-authored top-level values",
        &assay_authored_projection(&event),
    );
}

#[test]
fn caller_overridden_producer_metadata_is_reflected_input() {
    let producer =
        ProducerMeta::new("certified-producer", "approved-version").with_git("endorsed-git");
    let event = fully_populated_hostile_event().with_producer(&producer);
    let serialized = serde_json::to_value(&event).expect("serialize EvidenceEvent");

    assert_eq!(serialized["assayproducer"], producer.name);
    assert_eq!(serialized["assayproducerversion"], producer.version);
    assert_eq!(serialized["assaygit"], producer.git.unwrap());
    assert!(
        catch_unwind(AssertUnwindSafe(|| assert_no_unearned_status(
            "raw hostile producer override",
            &serialized
        )))
        .is_err(),
        "the raw producer override must prove that the observation channel is live"
    );

    let keys = CALLER_OR_REFLECTED_KEYS
        .into_iter()
        .chain(PRODUCER_OVERRIDE_KEYS)
        .collect::<Vec<_>>();
    let scrubbed = projection_without(&event, &keys);
    assert_no_unearned_status("EvidenceEvent with reflected producer override", &scrubbed);

    for key in PRODUCER_OVERRIDE_KEYS {
        let mut mutant = scrubbed.clone();
        mutant[key] = serialized[key].clone();
        assert!(
            catch_unwind(AssertUnwindSafe(|| assert_no_unearned_status(
                &format!("EvidenceEvent with reflected producer field {key} restored"),
                &mutant
            )))
            .is_err(),
            "restoring hostile producer field {key} must make the backstop fire"
        );
    }
}

#[test]
fn every_reflected_text_excision_is_load_bearing() {
    let event = fully_populated_hostile_event();
    let serialized = serde_json::to_value(&event).expect("serialize EvidenceEvent");
    let scrubbed = assay_authored_projection(&event);

    for key in CALLER_OR_REFLECTED_KEYS {
        let mut mutant = scrubbed.clone();
        mutant[key] = serialized[key].clone();
        assert!(
            catch_unwind(AssertUnwindSafe(|| assert_no_unearned_status(
                &format!("EvidenceEvent with {key} excision removed"),
                &mutant
            )))
            .is_err(),
            "restoring hostile caller/reflected field {key} must make the backstop fire"
        );
    }
}
