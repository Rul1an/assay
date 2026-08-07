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
