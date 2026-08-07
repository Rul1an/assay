//! ADR-045's producer-vocabulary table must name every `assay*` member the seal payload carries.
//!
//! It named four while `SealPayload` declared ten, for as long as the struct had existed. The table
//! was written against the design and never re-read against the code, which is what a hand-kept list
//! beside a type does. The omission was not uniform either: two of the six missing were
//! `assayDropProofModel` and `assayDropProofBasis`, so the table hid the member that says whether a
//! drop count was verified or merely asserted — the most consequential thing in the payload for a
//! consumer deciding what the seal is worth.
//!
//! Both sides are parsed here. A test that compared the table against a second hand-kept list would
//! reproduce the defect one level up.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Every `assay*` wire name in `SealPayload`, from its serde renames.
///
/// Bounded to the struct rather than the whole file: the tests below it construct payloads with the
/// same names in string literals, and counting those would make the check pass by accident.
fn payload_members() -> BTreeSet<String> {
    let src = read("crates/assay-cli/src/aee_seal.rs");
    let start = src
        .find("pub struct SealPayload {")
        .expect("aee_seal.rs has no `pub struct SealPayload`");
    let body = &src[start..];
    let end = body.find("\n}\n").expect("SealPayload is unterminated");
    let body = &body[..end];

    let mut found = BTreeSet::new();
    for line in body.lines() {
        let Some(rest) = line.trim().strip_prefix("#[serde(rename = \"") else {
            continue;
        };
        let Some(name) = rest.split('"').next() else {
            continue;
        };
        if name.starts_with("assay") {
            found.insert(name.to_owned());
        }
    }
    assert!(
        found.len() > 3,
        "parsed only {} members from SealPayload; the struct shape moved and this check went blind",
        found.len()
    );
    found
}

/// Every `assay*` member named in the ADR's producer-vocabulary table.
fn table_members() -> BTreeSet<String> {
    let doc = read("docs/architecture/ADR-045-aee-substrate-signed-run-end-seal.md");
    let start = doc
        .find("### AEE normative fields vs Assay producer vocabulary")
        .expect("ADR-045 has no producer-vocabulary section");
    let section = &doc[start..];
    let end = section[1..]
        .find("\n### ")
        .map(|i| i + 1)
        .unwrap_or(section.len());

    let mut found = BTreeSet::new();
    for line in section[..end].lines() {
        let line = line.trim();
        // Table rows only. Prose below the table mentions these names too, and counting those would
        // let a member be "documented" by an aside rather than by a row.
        if !line.starts_with("| `assay") {
            continue;
        }
        if let Some(name) = line.trim_start_matches("| `").split('`').next() {
            found.insert(name.to_owned());
        }
    }
    found
}

#[test]
fn the_adr_table_names_every_producer_member_the_payload_carries() {
    let payload = payload_members();
    let table = table_members();

    let undocumented: Vec<&String> = payload.difference(&table).collect();
    let phantom: Vec<&String> = table.difference(&payload).collect();

    assert!(
        undocumented.is_empty(),
        "SealPayload carries these `assay*` members and ADR-045's table does not name them: {undocumented:?}\n\
         A consumer reading the ADR to learn what the prefix covers would not know they exist."
    );
    assert!(
        phantom.is_empty(),
        "ADR-045's table names these members and SealPayload does not carry them: {phantom:?}"
    );
}

/// The checker's required set is a subset of what the payload declares.
///
/// Not equality, and four members are legitimately unrequired: `assayObservedLabels` is a debugging
/// aid, `assayDropProofBasis` and `assayDropChannels` postdate the checker, and
/// `assayDropProofModel` was deliberately removed — requiring a producer member is what made one
/// load-bearing for structural validity, which ADR-045's prefix rule forbids.
///
/// What must never happen is the checker requiring a field the producer does not emit, because then
/// every real seal fails for a reason that is the checker's fault.
#[test]
fn the_checker_requires_nothing_the_payload_does_not_carry() {
    let src = read("scripts/experiments/aee_landlock_seal_fixture.py");
    let start = src
        .find("REQUIRED_SEAL_FIELDS = (")
        .expect("the fixture checker has no REQUIRED_SEAL_FIELDS");
    let body = &src[start..];
    let end = body
        .find(')')
        .expect("REQUIRED_SEAL_FIELDS is unterminated");

    let required: BTreeSet<String> = body[..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix('"'))
        .filter_map(|l| l.split('"').next())
        .filter(|n| n.starts_with("assay"))
        .map(str::to_owned)
        .collect();
    assert!(!required.is_empty(), "parsed no required assay* fields");

    let payload = payload_members();
    let missing: Vec<&String> = required.difference(&payload).collect();
    assert!(
        missing.is_empty(),
        "the checker requires these and SealPayload does not emit them: {missing:?}"
    );
}
