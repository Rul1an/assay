//! `SPEC-PR-Gate-Outputs-v1.md` §5 says `reason_code` MUST be one of the registered values. Until
//! this test, nothing held the registry to what the implementation can actually emit, and it had
//! drifted: `E_INVALID_ARGS` and `E_NETWORK_ERROR` were emittable and unregistered, while
//! `E_JUDGE_UNCERTAIN` and `E_POLICY_VIOLATION` were named in no table at all.
//!
//! The emittable set is read from the `as_str` match in `exit_codes.rs`. That match is exhaustive
//! over the enum, so parsing its arms gives every variant — a hand-kept list beside the enum would
//! be one more thing to drift, which is the failure being fixed here.

use std::collections::BTreeSet;
use std::path::PathBuf;

const SPEC: &str = "docs/architecture/SPEC-PR-Gate-Outputs-v1.md";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = workspace_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Every string `ReasonCode::as_str` can return, except the empty success value.
///
/// A `=>` arm whose right-hand side is not a string literal is a hard failure, not a skip: a
/// computed reason code would leave the registry silently short of what the CLI emits.
fn emittable_codes() -> BTreeSet<String> {
    let src = read("crates/assay-cli/src/exit_codes.rs");
    let body = after(&src, "pub fn as_str(&self) -> &'static str {");
    let body = before(body, "\n    }");
    let mut found = BTreeSet::new();
    for line in body.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("ReasonCode::") else {
            continue;
        };
        let Some((_variant, value)) = rest.split_once("=>") else {
            continue;
        };
        let value = string_literal(value)
            .unwrap_or_else(|| panic!("`as_str` arm is not a string literal: {line}"));
        // `Success => ""` is the exit-0 value. §5's normative rule is about exit_code != 0, and it
        // explicitly forbids an empty reason_code there, so the empty string is not a registrable
        // code and its absence from every table is correct.
        if value.is_empty() {
            continue;
        }
        assert!(
            found.insert(value.clone()),
            "duplicate as_str value {value:?}"
        );
    }
    assert!(
        found.len() > 10,
        "parsed only {} codes; the `as_str` shape moved",
        found.len()
    );
    found
}

/// The codes in one of §5's tables, read from the first column of each row.
fn registered(section: &str) -> BTreeSet<String> {
    let spec = read(SPEC);
    let body = after(&spec, &format!("### {section}"));
    let body = before(body, "\n### ");
    body.lines()
        .filter_map(|line| line.strip_prefix('|'))
        .filter_map(|line| line.split('|').next())
        // A cell may carry several codes that share one note, so split it. The header row and the
        // `|---|` separator survive the split and are dropped by the shape filter below.
        .flat_map(|cell| cell.split(','))
        .map(str::trim)
        .filter(|token| is_code_shaped(token))
        .map(str::to_owned)
        .collect()
}

/// SCREAMING_SNAKE, starting with a letter. Deliberately strict: a cell that is prose contributes
/// nothing rather than contributing a word, so a table this parser reads wrongly shows up as a
/// missing code.
fn is_code_shaped(token: &str) -> bool {
    !token.is_empty()
        && token.starts_with(|c: char| c.is_ascii_uppercase())
        && token
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
}

fn after<'a>(haystack: &'a str, needle: &str) -> &'a str {
    let idx = haystack
        .find(needle)
        .unwrap_or_else(|| panic!("not found: {needle}"));
    &haystack[idx + needle.len()..]
}

fn before<'a>(haystack: &'a str, needle: &str) -> &'a str {
    haystack
        .split_once(needle)
        .map(|(a, _)| a)
        .unwrap_or(haystack)
}

fn string_literal(s: &str) -> Option<String> {
    let start = s.find('"')? + 1;
    let end = s[start..].find('"')? + start;
    Some(s[start..end].to_owned())
}

/// Every code named in a `match` arm of the agentic remediation builder.
fn remediation_matched_codes() -> BTreeSet<String> {
    let src = read("crates/assay-core/src/agentic/builder.rs");
    let mut found = BTreeSet::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if !trimmed.ends_with("=> {") || !trimmed.starts_with('"') {
            continue;
        }
        for part in trimmed.trim_end_matches("=> {").split('|') {
            if let Some(code) = string_literal(part) {
                found.insert(code);
            }
        }
    }
    assert!(
        !found.is_empty(),
        "parsed no remediation match arms; the builder's shape moved"
    );
    found
}

fn all_registered() -> BTreeSet<String> {
    ["5.1", "5.2", "5.3", "5.4", "5.5"]
        .iter()
        .flat_map(|s| registered(s))
        .collect()
}

#[test]
fn every_emittable_code_is_registered() {
    let unregistered: Vec<String> = emittable_codes()
        .difference(&all_registered())
        .cloned()
        .collect();
    assert!(
        unregistered.is_empty(),
        "`ReasonCode::as_str` can emit these, and {SPEC} §5 registers none of them: {unregistered:?}. \
         §5's normative rule requires reason_code to be a registered value, so an unregistered \
         emittable code makes the CLI violate its own spec."
    );
}

#[test]
fn every_registered_code_is_emittable_or_reserved() {
    let emittable = emittable_codes();
    let reserved = registered("5.4");
    let policy_engine = registered("5.5");
    let orphans: Vec<String> = ["5.1", "5.2", "5.3"]
        .iter()
        .flat_map(|s| registered(s))
        .filter(|code| {
            !emittable.contains(code) && !reserved.contains(code) && !policy_engine.contains(code)
        })
        .collect();
    assert!(
        orphans.is_empty(),
        "{SPEC} registers these in an exit-code table, but nothing emits them and §5.4 does not \
         list them as reserved: {orphans:?}. Either wire the code up, or record it as reserved — \
         §175 forbids deleting it outright."
    );
}

#[test]
fn every_remediation_branch_keys_on_a_registered_code() {
    let known = all_registered();
    // The `codes::` registry is a second source of live codes reaching `Diagnostic.code`; those are
    // inventoried in REASON-CODE-VOCABULARIES.md rather than in this spec.
    let diagnostic_codes: BTreeSet<String> = {
        let src = read("crates/assay-core/src/errors/diagnostic.rs");
        let body = before(after(&src, "pub mod codes {"), "\n}");
        body.lines()
            .filter(|l| l.trim().starts_with("pub const "))
            .filter_map(|l| l.split_once('=').and_then(|(_, v)| string_literal(v)))
            .collect()
    };
    let unknown: Vec<String> = remediation_matched_codes()
        .into_iter()
        .filter(|code| !known.contains(code) && !diagnostic_codes.contains(code))
        .collect();
    assert!(
        unknown.is_empty(),
        "`agentic::builder` picks a remediation for these codes, and no registry knows them: \
         {unknown:?}. A branch keyed on a code nothing emits and nothing registers cannot fire and \
         cannot be reviewed — register it (§5.4 reserved is enough) or delete the arm."
    );
}

#[test]
fn the_reserved_section_claims_nothing_that_is_live() {
    // A reserved code that something constructs is a stale record, and a consumer told "this may
    // start appearing" about a code already appearing has been misinformed in the wrong direction.
    let src = read("crates/assay-core/src/policy_engine.rs");
    let live_policy_codes: BTreeSet<String> = src
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//") && !l.starts_with("pub reason_code"))
        .filter_map(|l| l.strip_prefix("reason_code:"))
        .filter_map(string_literal)
        .collect();
    let contradictions: Vec<String> = registered("5.4")
        .into_iter()
        .filter(|code| live_policy_codes.contains(code))
        // §5.4 records exactly this split for two codes: the `ReasonCode` variant is dead while the
        // string is originated by the policy engine. Named here so the assertion covers the rest.
        .filter(|code| code != "E_ARG_SCHEMA" && code != "E_SEQUENCE_VIOLATION")
        .collect();
    assert!(
        contradictions.is_empty(),
        "§5.4 lists these as reserved, but `policy_engine` constructs them: {contradictions:?}"
    );
}

/// The one file that states the evidence-integrity boundary. Both the §5.1 registry row and the
/// `ReasonCode::EEvidenceIntegrity` doc comment are transports for it, so it is the only place the
/// rule is written.
const BOUNDARY: &str = "crates/assay-cli/src/exit_codes/evidence_integrity_boundary.md";

/// One line of flowed prose: every run of whitespace collapsed.
///
/// The boundary file wraps at 100 columns and the §5.1 row is a single markdown table cell, so a
/// byte comparison of the two would be an assertion about line wrapping. Flowing both first makes
/// the comparison about the words.
fn flowed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The description cell of one code's row in a §5 table.
fn spec_description(section: &str, code: &str) -> String {
    let spec = read(SPEC);
    let body = after(&spec, &format!("### {section}"));
    let body = before(body, "\n### ");
    let prefix = format!("| {code} ");
    let row = body
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("§{section} has no row for {code}"));
    let cells: Vec<&str> = row.trim_matches('|').split('|').collect();
    assert_eq!(
        cells.len(),
        2,
        "§{section}'s row for {code} has {} cells, not the code/description pair this reads: {row}",
        cells.len()
    );
    flowed(cells[1])
}

#[test]
fn the_evidence_integrity_boundary_reads_the_same_in_the_spec_and_the_code() {
    // One statement is normative for consumers and the other is what an author reads at the
    // definition, and they have to say the same thing. Asserting that each separately contains the
    // right phrases is a neighbouring property, not this one: it passes when the two are exact
    // opposites, which is how a doc comment claiming the bundle was tampered with and an I/O
    // failure is in scope sat next to a row denying both, green.
    assert_eq!(
        spec_description("5.1", "E_EVIDENCE_INTEGRITY"),
        flowed(&read(BOUNDARY)),
        "the §5.1 registry row is no longer the text of {BOUNDARY}. That file is the boundary; the \
         row and the `ReasonCode::EEvidenceIntegrity` doc comment transport it. Edit the file and \
         reflow it into the row rather than editing the row."
    );
}

#[test]
fn the_doc_comment_transports_the_boundary_rather_than_restating_it() {
    // `include_str!` is what makes the equality above cover the doc comment too. Replace it with a
    // hand-written `///` block and the row could be pinned while the definition drifts.
    let src = read("crates/assay-cli/src/exit_codes.rs");
    let head = src
        .split_once("    EEvidenceIntegrity,")
        .map(|(before_decl, _)| before_decl)
        .expect("no `EEvidenceIntegrity` variant in the enum");
    let last = head.lines().next_back().unwrap_or_default().trim();
    assert_eq!(
        last, "#[doc = include_str!(\"exit_codes/evidence_integrity_boundary.md\")]",
        "`EEvidenceIntegrity` is documented by {last:?} rather than by including {BOUNDARY}"
    );
}

#[test]
fn the_boundary_claims_no_structure_the_verifier_does_not_build() {
    // `compute_run_root` is a flat sha256 over the concatenated content hashes, so "Merkle" would
    // promise inclusion proofs and sub-range verification that do not exist. ADR-042's stop list
    // makes that an unearned claim rather than loose wording, and one file now carries it.
    let boundary = read(BOUNDARY);
    assert!(
        !boundary.contains("Merkle"),
        "{BOUNDARY} names a Merkle structure; `run_root` is a hash chain"
    );
}
