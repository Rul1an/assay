//! The strings `assay.session.finding` carries must be the ones `assay-core` defines.
//!
//! `PayloadSessionFinding::outcome` and `::extent` are `String`, documented as carrying the values
//! of `RuleOutcome::label()` and `TraceExtent::label()`. Two vocabularies for one thing drift, and
//! `crates/assay-core/src/metrics_api.rs` has already written down what that costs here: "a reader
//! with its own copy of `\"not_exercised\"` would match nothing the day the spelling moved,
//! reporting a clean run instead of a broken one."
//!
//! Ideally the payload would call `label()`. It cannot: `assay-core` reaches `assay-evidence`
//! through `assay-adapter-api`, so a production edge the other way is a cycle. `CLAUDE.md` names a
//! parity test as the sanctioned fallback for exactly that, and `tests/claim_gate_parity.rs`
//! already uses it in this crate for the claim gate.
//!
//! This reads `assay-core`'s source rather than taking a dev-dependency on it. A dev-only edge
//! would be legal, but it would pull the whole crate into this crate's test build to compare six
//! string literals, and the existing AEE version-parity test set the lighter precedent.

use assay_evidence::types::PayloadSessionFinding;
use assay_evidence::{
    coding_agent_claim_decision, session_finding_coverage_state, CodingAgentClaimKind,
    CodingAgentCoverageGap, CodingAgentCoverageState, CodingAgentGateDecision,
    CodingAgentSourceClass,
};
use std::path::Path;

fn sequence_eval_src() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
        .join("crates/assay-core/src/sequence_eval.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The string literals returned by a `label()` in the named `impl` block.
///
/// Scoped to one `impl` so the two enums cannot borrow each other's spellings and pass.
fn labels_of(src: &str, impl_name: &str) -> Vec<String> {
    let start = src
        .find(&format!("impl {impl_name} {{"))
        .unwrap_or_else(|| panic!("no `impl {impl_name}` in sequence_eval.rs"));
    let body = &src[start..];
    let end = body
        .find("\n}")
        .expect("the impl block is terminated at column 0");
    body[..end]
        .lines()
        .filter(|l| l.contains("=>"))
        .filter_map(|l| {
            let after = l.split("=>").nth(1)?;
            let open = after.find('"')?;
            let rest = &after[open + 1..];
            Some(rest[..rest.find('"')?].to_string())
        })
        .collect()
}

#[test]
fn the_outcome_vocabulary_matches_assay_core() {
    let got = labels_of(&sequence_eval_src(), "RuleOutcome");
    assert_eq!(
        got,
        vec!["held", "violated", "not_exercised"],
        "`RuleOutcome::label` changed. `PayloadSessionFinding::outcome` documents these three \
         spellings and every consumer keying on them reads what this enum emits, so a rename has \
         to reach the payload doc and any consumer before it lands."
    );
}

#[test]
fn the_extent_vocabulary_matches_assay_core() {
    let got = labels_of(&sequence_eval_src(), "TraceExtent");
    assert_eq!(
        got,
        vec!["complete", "partial"],
        "`TraceExtent::label` changed; `PayloadSessionFinding::extent` documents these two."
    );
}

/// The extractor, against fixture text.
///
/// Without this, a `labels_of` that silently returned nothing would make both tests above compare
/// two empty vectors and pass — the exact vacuity the assertions exist to prevent.
#[test]
fn the_label_extractor_is_scoped_to_one_impl_and_reads_values() {
    let src = "\
impl First {
    pub const fn label(self) -> &'static str {
        match self {
            Self::A => \"alpha\",
            Self::B => \"beta\",
        }
    }
}

impl Second {
    pub const fn label(self) -> &'static str {
        match self {
            Self::C => \"gamma\",
        }
    }
}
";
    assert_eq!(labels_of(src, "First"), vec!["alpha", "beta"]);
    assert_eq!(
        labels_of(src, "Second"),
        vec!["gamma"],
        "a second impl must not inherit the first one's labels"
    );
}

/// A missing impl is a panic, not an empty list.
#[test]
fn a_missing_impl_fails_loudly() {
    let result = std::panic::catch_unwind(|| labels_of("impl Other {\n}\n", "RuleOutcome"));
    assert!(
        result.is_err(),
        "a renamed or deleted impl must fail rather than compare empty vectors"
    );
}

/// The constructor accepts the vocabulary it documents, and the event type is the registered tag.
///
/// `PayloadSessionFinding::new` exists so a producer that can see both crates does not restate the
/// field order, and `EVENT_TYPE` so it does not respell the tag. Both are only worth having if they
/// agree with the enum that owns them, which is what the rest of this file already checks for the
/// strings; this checks that the constructor is wired to the same ones and that the tag is the one
/// `Payload` actually maps.
#[test]
fn the_constructor_and_the_event_type_agree_with_the_registered_variant() {
    use assay_evidence::types::{Payload, PayloadSessionFinding};

    assert_eq!(PayloadSessionFinding::EVENT_TYPE, "assay.session.finding");

    let f = PayloadSessionFinding::new(
        "never_after:bash[command]->bash[command]",
        "never_after",
        "violated",
        vec![1, 2],
        "complete",
        Some("credential read at 1 followed by egress at 2".to_string()),
    );
    assert_eq!(f.spanned, vec![1, 2]);
    assert_eq!(f.outcome, "violated");

    // The tag round-trips into the variant, so `EVENT_TYPE` is the name a reader will meet rather
    // than a constant that merely looks right.
    let tagged = serde_json::json!({
        "type": PayloadSessionFinding::EVENT_TYPE,
        "payload": serde_json::to_value(&f).expect("payload serialises"),
    });
    match serde_json::from_value::<Payload>(tagged).expect("the tag resolves to the variant") {
        Payload::SessionFinding(back) => assert_eq!(back, f, "round trip is lossless"),
        other => panic!("expected the session-finding variant, got {other:?}"),
    }
}

fn types_src() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/types.rs")
        .canonicalize()
        .expect("types.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Option 1: `extent` docs must refuse the stronger reading. A grep is a weak
/// control — it proves the sentence is present, not that a reader heeds it —
/// and the PR should say so. It still fails if the disclaimer is deleted.
#[test]
fn extent_docs_make_no_fidelity_claim() {
    let core = sequence_eval_src();
    let extent_docs = core
        .split("pub enum TraceExtent")
        .next()
        .expect("TraceExtent is documented above its declaration");
    assert!(
        extent_docs.contains("no fidelity claim"),
        "TraceExtent docs must state that extent makes no fidelity claim"
    );
    assert!(
        extent_docs.contains("nothing is missing"),
        "TraceExtent docs must say complete must not be read as nothing is missing"
    );

    let notes = types_src();
    let extent_field = notes
        .split("pub extent:")
        .next()
        .expect("extent field is documented");
    // Take the last doc block before `pub extent`.
    let start = extent_field
        .rfind("/// Whether the trace")
        .expect("extent field keeps its temporal-claim lead-in");
    let field_docs = &extent_field[start..];
    assert!(
        field_docs.contains("no fidelity claim"),
        "session-finding notes must state that extent makes no fidelity claim"
    );
    assert!(
        field_docs.contains("nothing is missing"),
        "session-finding notes must say complete must not be read as nothing is missing"
    );
}

fn coverage_wire(state: assay_evidence::CodingAgentCoverageState) -> String {
    match serde_json::to_value(state).expect("coverage state serialises") {
        serde_json::Value::String(s) => s,
        other => panic!("coverage state must serialise as a string, got {other}"),
    }
}

fn complete_extent_label() -> String {
    labels_of(&sequence_eval_src(), "TraceExtent")
        .into_iter()
        .next()
        .expect("TraceExtent::label has a first member, pinned as complete above")
}

/// The reported bypass: `extent: complete` and no coverage field. A reader who
/// treats complete as "the trace is whole" exits 0; the existing coverage
/// vocabulary exits 1 because silence over the rest is not a fact.
///
/// Same mutant, both guards, both exit codes. Labels come from shipping
/// serde / `label()`, not copied literals.
#[test]
fn a_complete_narrowed_finding_fails_closed_on_existing_coverage_vocabulary() {
    let finding = PayloadSessionFinding::new(
        "never_after:bash[command]->bash[command]",
        "never_after",
        "held",
        vec![],
        complete_extent_label(),
        None,
    );
    assert_eq!(
        finding.coverage, None,
        "the constructor must not invent a coverage declaration"
    );

    let old_guard_exit = i32::from(finding.extent != complete_extent_label());
    let coverage = session_finding_coverage_state(finding.coverage.as_deref());
    let absence = coding_agent_claim_decision(
        CodingAgentSourceClass::BoundaryObserved,
        coverage,
        CodingAgentClaimKind::BoundedNegative,
    );
    let new_guard_exit = i32::from(absence.decision != CodingAgentGateDecision::Allowed);

    assert_eq!(
        old_guard_exit, 0,
        "the reported bypass: complete with no coverage looks faithful to a temporal-only reader"
    );
    assert_eq!(
        coverage,
        CodingAgentCoverageState::Partial,
        "absence of coverage is Partial, not Observed"
    );
    assert_eq!(
        absence.gap,
        Some(CodingAgentCoverageGap::PartialOnly),
        "Partial + absence is the existing PartialOnly rule, not a second vocabulary"
    );
    assert_eq!(
        new_guard_exit, 1,
        "the same mutant must not pass an absence claim after the coverage key"
    );
}

#[test]
fn unrecognised_coverage_is_partial_and_observed_is_the_declared_faithful_value() {
    assert_eq!(
        session_finding_coverage_state(Some("narrowed")),
        CodingAgentCoverageState::Partial,
        "unrecognised values fail closed rather than as a faithful trace"
    );

    let observed = coverage_wire(CodingAgentCoverageState::Observed);
    assert_eq!(
        session_finding_coverage_state(Some(observed.as_str())),
        CodingAgentCoverageState::Observed
    );
    let allowed = coding_agent_claim_decision(
        CodingAgentSourceClass::BoundaryObserved,
        session_finding_coverage_state(Some(observed.as_str())),
        CodingAgentClaimKind::BoundedNegative,
    );
    assert_eq!(allowed.decision, CodingAgentGateDecision::Allowed);
}

#[test]
fn session_finding_coverage_reuses_coding_agent_coverage_state_wire_names() {
    for state in [
        CodingAgentCoverageState::Observed,
        CodingAgentCoverageState::Unavailable,
        CodingAgentCoverageState::SelfReported,
        CodingAgentCoverageState::Absent,
        CodingAgentCoverageState::Partial,
    ] {
        let name = coverage_wire(state);
        assert_eq!(
            session_finding_coverage_state(Some(name.as_str())),
            state,
            "the session-finding field must accept this enum's serde name {name}"
        );
    }
}
