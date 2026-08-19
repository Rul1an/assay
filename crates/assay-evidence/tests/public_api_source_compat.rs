//! Compile probes for public generic signatures that `cargo-semver-checks` does not cover.
//!
//! `cargo-semver-checks 0.50.0` reported all 223 checks passing across a change that altered an
//! accepted generic iterator item type from `Item = Option<CodingAgentClaimCeiling>` to
//! `Item = &CodingAgentClaimDecision`, while a direct old-call-site probe failed with `E0271`
//! (#2356). That merge was safe — the signature had never been in a release tag — but it located
//! the gate's reliability boundary: a green semver job is not on its own evidence of source
//! compatibility for generic associated-type changes.
//!
//! This file is the missing signal, and it needs no new tooling. An integration test compiles
//! against the crate's PUBLIC API exactly as a downstream consumer does, so a source-breaking
//! signature change fails the build here even when the semver gate stays green.
//!
//! What each probe pins is the *accepted argument shape*, not the function's behaviour. Passing an
//! owned collection is the assertion: if `Item` narrows to a reference, the owned value stops
//! satisfying the bound and this stops compiling. Nothing here asserts on output, because output is
//! already covered elsewhere and would make a failure ambiguous between behaviour and signature.
//!
//! Measured on `eb62b34d0`, seeding exactly the #2356 mutation
//! (`Item = EvidenceEvent` -> `Item = &'a EvidenceEvent`) into `add_events`:
//!
//! ```text
//! cargo semver-checks check-release -p assay-evidence --baseline-rev v5.4.0
//!   223 checks: 223 pass, 31 skip
//!   Summary no semver update required          <- exit 0, GREEN
//!
//! cargo test -p assay-evidence --test public_api_source_compat
//!   error[E0271]: type mismatch resolving
//!     `<Vec<EvidenceEvent> as IntoIterator>::Item == &EvidenceEvent`   <- RED
//! ```
//!
//! So the two signals disagree on the same change, which is the whole reason this file exists. A
//! green semver job means "no break this tool models", not "no break".
//!
//! Scope, so this is not read as more than it is: these are the generic public entry points that a
//! caller can pass a collection to. This is not an exhaustive public-API snapshot — `cargo public-api`
//! is the tool for that, and `optional-public-api-drift.sh` already runs it. The point here is
//! narrower: cover the specific class the semver gate is known to miss.

use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::EvidenceEvent;
use serde_json::json;
use std::io::Cursor;

fn event(seq: u64) -> EvidenceEvent {
    EvidenceEvent::new(
        "assay.public_api.probe",
        "urn:assay:public-api-probe",
        "run_public_api_0001",
        seq,
        json!({ "seq": seq }),
    )
}

/// `BundleWriter::add_events` accepts an iterator of OWNED events.
///
/// The probe is the ownership, not the call. Narrowing to `Item = &EvidenceEvent` is source
/// breaking for every caller passing a `Vec`, and is the shape `cargo-semver-checks 0.50.0` did not
/// report in #2356.
#[test]
fn add_events_accepts_an_owned_iterator_of_events() {
    let mut writer = BundleWriter::new(Cursor::new(Vec::new()));

    // A `Vec<EvidenceEvent>` by value: satisfies `Item = EvidenceEvent`, and does not satisfy
    // `Item = &EvidenceEvent`.
    let owned: Vec<EvidenceEvent> = (0..3).map(event).collect();
    writer.add_events(owned);

    // A non-`Vec` owned iterator too, so the bound is pinned rather than one concrete collection.
    let mut writer = BundleWriter::new(Cursor::new(Vec::new()));
    writer.add_events((3..5).map(event));

    // `add_event` singular takes an owned event on the same terms.
    let mut writer = BundleWriter::new(Cursor::new(Vec::new()));
    writer.add_event(event(5));
}
