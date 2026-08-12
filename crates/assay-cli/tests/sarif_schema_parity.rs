//! The two `SARIF_SCHEMA` constants must name the same document.
//!
//! `assay-core` and `assay-evidence` each declare their own `SARIF_SCHEMA`. Only one
//! direction is blocked: `assay-evidence` cannot depend on `assay-core`. The other way is
//! already open, since `assay-core -> assay-adapter-api -> assay-evidence` exists today, so
//! `assay-core` could call the evidence constant without any new edge. That makes one rule
//! reachable rather than impossible, and the choice between calling it and testing parity is
//! a design question about which crate owns the value. Until now neither was done: what held
//! the two together was a pair of reciprocal doc comments telling each other to update the
//! sibling, which is a convention rather than a constraint. Per one-rule-one-function the
//! parity test is the fallback while that question is open; deciding the owner, whether that
//! is `assay-core` calling through or `assay-common` holding it, is an ADR question rather
//! than something to settle in a test file.
//!
//! What this catches is the partial edit. A change applied to one constant leaves the tree
//! looking consistent, because every emitter still produces a well-formed document and every
//! golden fixture still matches the producer that wrote it. The documents just stop agreeing
//! about which schema they claim to be.
//!
//! What no test here can catch is the URI going dead. That failure is not local to this
//! repository: the target moves when its publisher moves it, and a check that reached the
//! network would make the build depend on someone else's uptime. The mitigation is the
//! choice of target rather than a test. `docs.oasis-open.org/.../errata01/os/schemas/...`
//! is an OASIS-published artifact at a versioned path rather than a git ref, and it is the
//! URL the schema document names as its own `id`, so the label and the document agree about
//! what the document is. A `raw.githubusercontent.com/<org>/<repo>/<branch>/...` URL resolves
//! against whatever that branch holds today, so the document a consumer validates against can
//! change without any failure on our side. Our own risk here was not hypothetical: a `$schema`
//! documented in `docs/architecture/SPEC-GitHub-Action-v2.1.md`, under a heading reading "SARIF
//! Contract", pointed at a path that had already 404'd, and nothing checked it because the link
//! checker walks markdown links rather than URLs quoted inside fenced examples or Rust
//! literals.

use assay_core::report::sarif::SARIF_SCHEMA as CORE_SCHEMA;
use assay_evidence::lint::sarif::SARIF_SCHEMA as EVIDENCE_SCHEMA;

#[test]
fn the_two_schema_constants_name_the_same_document() {
    assert_eq!(
        CORE_SCHEMA, EVIDENCE_SCHEMA,
        "assay-core and assay-evidence declare different SARIF schema URIs, so two Assay \
         producers now claim to conform to different documents. Update both, or move the \
         constant to a single home."
    );
}

#[test]
fn the_schema_uri_is_not_pinned_to_a_moving_git_ref() {
    for (crate_name, uri) in [
        ("assay-core", CORE_SCHEMA),
        ("assay-evidence", EVIDENCE_SCHEMA),
    ] {
        assert!(
            !uri.contains("raw.githubusercontent.com"),
            "{crate_name} pins $schema to a raw git URL ({uri}). Those resolve against a \
             branch, so the document a consumer validates against changes when the \
             publisher restructures, with no failure on our side. Name a published, \
             versioned artifact instead."
        );
    }
}
