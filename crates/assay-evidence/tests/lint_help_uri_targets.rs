//! Every `helpUri` this producer emits must land on a section that exists.
//!
//! This test exists because the emitter shipped a dead pointer for a long time and nothing caught
//! it. The rule `helpUri`s and the driver `informationUri` pointed at `docs.assay.dev`, a host that
//! does not resolve — and `assay.dev` is not even ours, so the pointers named a third party's
//! domain. The repo does run a link checker (`.github/workflows/docs-link-check.yml`), and it could
//! not have caught this three times over: it triggers only on `paths: [docs/**]` so a change to
//! Rust source never fires it, it explicitly skips anything starting with `http`, and it only reads
//! files changed in the PR.
//!
//! The general shape is that the link checker verifies the *docs* while the URIs a consumer
//! actually receives live in Rust string literals, checked by nothing. A pointer is a promise made
//! to someone reading the artifact, so it belongs to the artifact's tests.
//!
//! Scope, stated so nobody reads more into a pass than is here. This asserts the anchor exists in
//! the committed markdown. It does not assert the page is deployed, and it emphatically does not
//! assert the prose is true: if a rule's behaviour changes and its section does not, the anchor
//! keeps resolving and starts lying. That failure is quieter than a dead link, and this test does
//! not reach it.
//!
//! Anchors are pinned explicitly with `<a id="assay-w001"></a>` elements rather than left to a
//! slugifier, so the emitted fragment is a literal that appears in the source rather than something
//! derived by two implementations that could disagree. They were mkdocs `attr_list` (`{#assay-w001}`)
//! until the pointers moved to the tagged markdown: GitHub does not honour `attr_list`, so the brace
//! form would have rendered as literal text in the heading and slugified to
//! `assay-w001--subject-may-contain-a-secret`, landing every emitted fragment at the top of the page
//! instead of at its rule. An `<a id>` is honoured by both renderers.
//!
//! The second thing asserted here is that the pointer is version-pinned. An unversioned pointer
//! resolves against whatever the page says today, so a consumer reading an old report silently gets
//! today's prose — a failure quieter than a dead link and the one the peer emitters in
//! `aliksir/claude-code-skill-security-check#24` converged on forbidding. Note what pinning does and
//! does not buy, because the distinction is the whole reason the scope note above exists: it stops a
//! later rewrite from retroactively rewriting an old report, and it does not notice that the prose
//! moved. Nothing here reaches divergence; pinning makes divergence possible to see, not visible.

use assay_evidence::bundle::BundleWriter;
use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions};
use assay_evidence::lint::sarif::to_sarif;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::VerifyLimits;
use chrono::{TimeZone, Utc};
use std::collections::BTreeSet;
use std::io::Cursor;

const DOCS_PAGE: &str = "../../docs/lint/index.md";

/// The tagged source document every emitted pointer must name. Built the same way the emitter builds
/// it, from this crate's own version, so the test cannot pass by agreeing with a stale copy of the
/// prefix — if the shape in `rules.rs` changes, this changes with it and only the *anchor* is
/// compared.
const EXPECTED_DOC: &str = concat!(
    "https://github.com/Rul1an/assay/blob/v",
    env!("CARGO_PKG_VERSION"),
    "/docs/lint/index.md"
);
const EXPECTED_PREFIX: &str = concat!(
    "https://github.com/Rul1an/assay/blob/v",
    env!("CARGO_PKG_VERSION"),
    "/docs/lint/index.md#"
);

/// Refs that resolve against whatever they point at today. A pointer through any of these is the
/// defect this file exists to keep out, whichever host it names.
const MUTABLE_REFS: &[&str] = &["/blob/main/", "/blob/master/", "/main/", "/master/"];

/// A bundle with one clean event. The rule registry is emitted in full regardless of what fired,
/// so this is enough to see every `helpUri` the producer can ship.
fn minimal_sarif() -> serde_json::Value {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    let mut event = EvidenceEvent::new(
        "assay.net.connect",
        "urn:assay:test",
        "run_help_uri",
        0,
        serde_json::json!({"url": "https://api.example.com"}),
    );
    event.time = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    writer.add_event(event);
    writer.finish().unwrap();

    let report = lint_bundle_with_options(
        Cursor::new(&buffer),
        VerifyLimits::default(),
        LintOptions {
            packs: Vec::new(),
            max_results: None,
            bundle_path: None,
        },
    )
    .expect("lint should succeed")
    .report;

    to_sarif(&report)
}

/// Explicit `<a id="...">` targets declared in the committed page.
fn declared_anchors() -> BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOCS_PAGE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the help target page must exist at {}: {e}", path.display()));

    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("<a id=\"")?;
            let end = rest.find('"')?;
            Some(rest[..end].to_string())
        })
        .collect()
}

#[test]
fn every_rule_help_uri_resolves_to_a_declared_anchor() {
    let sarif = minimal_sarif();
    let anchors = declared_anchors();
    assert!(
        !anchors.is_empty(),
        "no {{#anchor}} declarations found — the extraction is broken, not the page"
    );

    let rules = sarif["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .expect("the driver must declare its rules");
    assert!(!rules.is_empty(), "the registry must not be empty");

    let mut checked = 0;
    for rule in rules {
        let id = rule["id"].as_str().expect("rule id");
        let uri = rule["helpUri"]
            .as_str()
            .unwrap_or_else(|| panic!("rule {id} ships no helpUri"));

        let fragment = uri
            .strip_prefix(EXPECTED_PREFIX)
            .unwrap_or_else(|| panic!("rule {id} points outside the help page: {uri}"));
        assert!(
            anchors.contains(fragment),
            "rule {id} points at #{fragment}, which no heading declares. \
             Declared: {anchors:?}"
        );
        checked += 1;
    }
    assert_eq!(
        checked,
        rules.len(),
        "every registry rule must be covered, not merely the ones that fired"
    );
}

/// The negative control. A test that only ever passes on correct input proves nothing about its
/// own sensitivity, and this whole class of defect is one that silently passed for months.
#[test]
fn a_fragment_with_no_heading_is_rejected() {
    let anchors = declared_anchors();
    assert!(
        !anchors.contains("assay-w999-does-not-exist"),
        "the anchor check must be able to fail; it accepted a fabricated target"
    );
}

/// The driver-level pointer is the one a consumer follows when no rule fired, which is exactly the
/// report most likely to be read as an all-clear.
#[test]
fn the_driver_information_uri_points_at_the_help_page() {
    let sarif = minimal_sarif();
    let uri = sarif["runs"][0]["tool"]["driver"]["informationUri"]
        .as_str()
        .expect("the driver must carry an informationUri");

    assert_eq!(uri, EXPECTED_DOC, "got {uri}");
    assert!(
        !uri.contains("docs.assay.dev"),
        "docs.assay.dev is NXDOMAIN and assay.dev belongs to a third party: {uri}"
    );
}

/// Every pointer in the emitted document must name an immutable target. This is the constraint the
/// peer emitters in `aliksir/claude-code-skill-security-check#24` settled on after three emitters were
/// audited and two were found pointing through a branch; ours was one of the two, in the document
/// describing our own SARIF contract.
///
/// Asserted over the serialised document rather than over the fields this file knows about, so a
/// pointer added later in a place nobody thought to check here is still covered.
#[test]
fn no_emitted_pointer_resolves_through_a_mutable_ref() {
    let serialised = serde_json::to_string(&minimal_sarif()).expect("serialise");
    for bad in MUTABLE_REFS {
        assert!(
            !serialised.contains(bad),
            "the emitted SARIF carries a pointer through `{bad}`, which resolves against whatever \
             that ref holds today. Name an immutable target: a release tag for our own documents, a \
             commit SHA for documents we do not own, a published artifact for a schema."
        );
    }
}

/// The pin has to name *this* build's version, not merely some version. A pointer frozen at an older
/// tag keeps resolving and quietly serves prose the report was not written against, which is the
/// same silent-staleness failure as an unpinned pointer with a longer fuse.
#[test]
fn every_pointer_is_pinned_to_this_builds_version() {
    let sarif = minimal_sarif();
    let expected_tag = concat!("/blob/v", env!("CARGO_PKG_VERSION"), "/");

    let driver = &sarif["runs"][0]["tool"]["driver"];
    let mut pointers = vec![driver["informationUri"]
        .as_str()
        .expect("informationUri")
        .to_string()];
    for rule in driver["rules"].as_array().expect("rules") {
        pointers.push(
            rule["helpUri"]
                .as_str()
                .unwrap_or_else(|| panic!("rule {} ships no helpUri", rule["id"]))
                .to_string(),
        );
    }

    assert!(
        pointers.len() > 1,
        "expected the driver pointer plus one per registry rule, got {}",
        pointers.len()
    );
    for uri in &pointers {
        assert!(
            uri.contains(expected_tag),
            "pointer is not pinned to this build's version ({}): {uri}",
            env!("CARGO_PKG_VERSION")
        );
    }
}

/// The registry pointer and the finding pointer for one rule must agree. They are built from the same
/// macro but named at twelve separate call sites, so a one-sided edit is the live risk — and the two
/// travel by different routes: the registry pointer reaches SARIF `rules[].helpUri`, while the
/// finding pointer reaches only the JSON report, since SARIF results carry no `helpUri` member. A
/// consumer comparing the two formats would see the disagreement before we did.
#[test]
fn the_registry_and_finding_pointers_agree_for_the_same_rule() {
    use assay_evidence::lint::rules::RULES;

    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    let mut event = EvidenceEvent::new(
        "assay.test.secret",
        "urn:assay:test",
        "run_parity",
        0,
        serde_json::json!({}),
    );
    event.subject = Some("token=abc123".into());
    event.time = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    writer.add_event(event);
    writer.finish().unwrap();

    let report = lint_bundle_with_options(
        Cursor::new(&buffer),
        VerifyLimits::default(),
        LintOptions {
            packs: Vec::new(),
            max_results: None,
            bundle_path: None,
        },
    )
    .expect("lint should succeed")
    .report;

    let finding = report
        .findings
        .iter()
        .find(|f| f.rule_id == "ASSAY-W001")
        .expect("the secret-in-subject rule must fire on a token= subject");
    let registry = RULES
        .iter()
        .find(|r| r.id == "ASSAY-W001")
        .and_then(|r| r.help_uri)
        .expect("ASSAY-W001 must declare a help_uri in the registry");

    assert_eq!(
        finding.help_uri.as_deref(),
        Some(registry),
        "the finding-level pointer and the registry pointer for ASSAY-W001 disagree, so the JSON \
         report and the SARIF report send a reader to different places"
    );
}

/// No pointer anywhere in the emitted document may name the dead host again.
#[test]
fn nothing_emitted_names_the_dead_host() {
    let sarif = minimal_sarif();
    let serialised = serde_json::to_string(&sarif).expect("serialise");
    assert!(
        !serialised.contains("docs.assay.dev"),
        "the emitted SARIF still names the dead host"
    );
}
