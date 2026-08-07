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
//! Anchors are pinned explicitly with `attr_list` (`{#assay-w001}`) rather than left to mkdocs'
//! slugify, so the emitted fragment is a literal that appears in the source rather than something
//! derived by two implementations that could disagree.

use assay_evidence::bundle::BundleWriter;
use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions};
use assay_evidence::lint::sarif::to_sarif;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::VerifyLimits;
use chrono::{TimeZone, Utc};
use std::collections::BTreeSet;
use std::io::Cursor;

const DOCS_PAGE: &str = "../../docs/lint/index.md";
const EXPECTED_PREFIX: &str = "https://docs.getassay.dev/lint/#";

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

/// Explicit `{#anchor}` targets declared by headings in the committed page.
fn declared_anchors() -> BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DOCS_PAGE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the help target page must exist at {}: {e}", path.display()));

    text.lines()
        .filter(|line| line.starts_with('#'))
        .filter_map(|line| {
            let start = line.find("{#")? + 2;
            let end = line[start..].find('}')? + start;
            Some(line[start..end].to_string())
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

    assert_eq!(uri, "https://docs.getassay.dev/lint/", "got {uri}");
    assert!(
        !uri.contains("docs.assay.dev"),
        "docs.assay.dev is NXDOMAIN and assay.dev belongs to a third party: {uri}"
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
