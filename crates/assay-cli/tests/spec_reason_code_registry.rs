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

/// Every `ReasonCode` variant name, keyed by the string it serialises to.
fn variant_by_code() -> std::collections::BTreeMap<String, String> {
    let src = read("crates/assay-cli/src/exit_codes.rs");
    let body = after(&src, "pub fn as_str(&self) -> &'static str {");
    let body = before(body, "\n    }");
    let mut map = std::collections::BTreeMap::new();
    for line in body.lines() {
        let Some(rest) = line.trim().strip_prefix("ReasonCode::") else {
            continue;
        };
        let Some((variant, value)) = rest.split_once("=>") else {
            continue;
        };
        if let Some(code) = string_literal(value) {
            map.insert(code, variant.trim().to_owned());
        }
    }
    map
}

/// Every `.rs` file under `crates/`, as workspace-relative paths.
fn walk_rust_sources() -> Vec<String> {
    fn visit(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, root, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.push(rel.to_string_lossy().into_owned());
                }
            }
        }
    }
    let root = workspace_root();
    let mut out = Vec::new();
    visit(&root.join("crates"), &root, &mut out);
    assert!(
        out.len() > 100,
        "walked only {} source files; the layout moved",
        out.len()
    );
    out
}

#[test]
fn the_reserved_section_claims_nothing_a_variant_construction_contradicts() {
    // The sibling test below reads one file, `policy_engine.rs`, because that is where the two
    // string-only codes are originated. That left the commoner shape unchecked: a `ReasonCode`
    // variant constructed anywhere in the workspace while §5.4 says nothing constructs it. §5.4 had
    // exactly one such row -- `E_POLICY_PARSE`, built at `cli_failure.rs:27` and reached from
    // `policy/validate.rs` -- and it survived long enough for a version-history note to cite it as
    // precedent for a new code. Deriving the property from the sources is the difference between an
    // inventory that is checked and one that is merely maintained.
    let variants = variant_by_code();
    let sources = walk_rust_sources();
    let mut live = Vec::new();
    for code in registered("5.4") {
        let Some(variant) = variants.get(&code) else {
            continue;
        };
        let needle = format!("ReasonCode::{variant}");
        for path in &sources {
            // The enum's own file declares and matches every variant, so it is not a construction
            // site. Tests construct freely and are not the CLI's shipped behaviour either.
            if path.contains("exit_codes.rs") || path.contains("tests/") {
                continue;
            }
            if read(path).contains(&needle) {
                live.push(format!("{code} ({needle} in {path})"));
                break;
            }
        }
    }
    assert!(
        live.is_empty(),
        "§5.4 calls these reserved, and something constructs the variant: {live:?}. A consumer told \
         a code may start appearing, about a code already appearing, has been misled in the \
         direction that matters."
    );
}

/// Declared `ErrorCode` variants whose name starts with `prefix`.
fn error_code_unit_variants_with_prefix(prefix: &str) -> BTreeSet<String> {
    error_code_unit_variants_with_prefix_from(
        &read("crates/assay-evidence/src/bundle/writer_next/errors.rs"),
        prefix,
    )
}

fn error_code_unit_variants_with_prefix_from(src: &str, prefix: &str) -> BTreeSet<String> {
    let body = after(src, "pub enum ErrorCode {");
    let body = before(body, "\n}");
    let found: BTreeSet<String> = body
        .lines()
        .filter_map(|line| declared_error_code_variant(line, prefix))
        .collect();
    assert!(
        !found.is_empty(),
        "ErrorCode parser found no {prefix}* unit variants; the enum shape moved"
    );
    found
}

/// One `ErrorCode` variant name, or none if the line is not a live declaration.
///
/// A trailing `//` note is dropped first so `ContractRogueCommented, // new` still counts.
/// Payload in `Name(...)` is stripped so a tuple variant is still a declared member.
/// A line that is only a comment is not a declaration.
fn declared_error_code_variant(line: &str, prefix: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with("//") {
        return None;
    }
    let code = match line.find("//") {
        Some(at) => line[..at].trim(),
        None => line,
    };
    let name = code.strip_suffix(',')?.trim();
    let ident = name.split_once('(').map(|(head, _)| head).unwrap_or(name);
    if ident.starts_with(prefix)
        && ident.starts_with(|c: char| c.is_ascii_uppercase())
        && ident.chars().all(char::is_alphanumeric)
    {
        Some(ident.to_string())
    } else {
        None
    }
}

/// Backtick identifiers in `text` whose name starts with `prefix`.
///
/// `Contract*` is a prefix mention, not an identifier, and is dropped because `*` is not
/// alphanumeric. That is the difference between naming the set and being a member of it.
fn backtick_identifiers_with_prefix(text: &str, prefix: &str) -> BTreeSet<String> {
    let found = backtick_identifiers_with_prefix_allowing_none(text, prefix);
    assert!(
        !found.is_empty(),
        "boundary parser found no `{prefix}*` identifiers; the fence shape moved"
    );
    found
}

/// The same collection rule without the non-empty guard.
///
/// A caller asserting that a boundary names *no* code of some other class needs the empty set to
/// be an answer rather than a panic. One collection rule, two callers, so the two cannot disagree
/// about what counts as a named identifier.
fn backtick_identifiers_with_prefix_allowing_none(text: &str, prefix: &str) -> BTreeSet<String> {
    text.split('`')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, token)| token)
        .filter(|token| {
            token.starts_with(prefix)
                && token.starts_with(|c: char| c.is_ascii_uppercase())
                && token.chars().all(char::is_alphanumeric)
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_boundary_names_error_codes_that_still_exist_in_assay_evidence() {
    // The integrity rule names seven `assay_evidence::ErrorCode` variants across a crate
    // boundary, four it requires and three it forbids. They are correct today, but they are
    // strings here: a rename in `assay-evidence` would leave a normative MUST pointing at a
    // variant that does not exist, and nothing would fail. Negative control for this test:
    // renaming `IntegrityRunRootMismatch` throughout `assay-evidence` only -- what a developer
    // who has never read this spec would do -- fails it.
    //
    // Integrity is a named subset, not the whole `Integrity*` prefix (`IntegrityZipBomb` is
    // outside this rule). Contract* is the complementary exact-set test below.
    let errors = read("crates/assay-evidence/src/bundle/writer_next/errors.rs");
    let boundary = read(BOUNDARY);
    for code in [
        "IntegrityManifestHash",
        "IntegrityEventHash",
        "IntegrityFileSizeMismatch",
        "IntegrityRunRootMismatch",
        "IntegrityIo",
        "IntegrityGzip",
        "IntegrityTar",
    ] {
        assert!(
            boundary.contains(code),
            "{code} is checked here and the boundary no longer names it; update this list in the \
             same change that drops it, and say why"
        );
        assert!(
            errors.contains(code),
            "the boundary names `ErrorCode::{code}` and `assay-evidence` no longer declares it. A \
             normative MUST pointing at a renamed variant is worse than one pointing at none."
        );
    }
}

#[test]
fn the_contract_boundary_names_exactly_the_declared_contract_star_variants() {
    // A hand list of five `Contract*` names left eight unguarded: renaming
    // `ContractSchemaVersion` throughout `assay-evidence`, or adding `ContractRogueNew`,
    // stayed green. The sets must be derived and equal, so either change fails until the
    // boundary is re-decided in the same change.
    let declared = error_code_unit_variants_with_prefix("Contract");
    let named = backtick_identifiers_with_prefix(&read(CONTRACT_BOUNDARY), "Contract");
    assert_eq!(
        declared, named,
        "CONTRACT_BOUNDARY and ErrorCode disagree on the Contract* set. A new variant is a new \
         mapping decision, not an automatic member; a rename leaves a normative MUST pointing at \
         a variant that does not exist."
    );
}

#[test]
fn error_code_parser_counts_a_bare_unit_variant() {
    let src = "pub enum ErrorCode {\n    ContractRogueNew,\n}";
    let found = error_code_unit_variants_with_prefix_from(src, "Contract");
    assert_eq!(found, BTreeSet::from(["ContractRogueNew".to_string()]));
}

#[test]
fn error_code_parser_counts_a_tuple_variant() {
    let src = "pub enum ErrorCode {\n    ContractRogueSeq(u32),\n}";
    let found = error_code_unit_variants_with_prefix_from(src, "Contract");
    assert!(
        found.contains("ContractRogueSeq"),
        "a tuple variant is a declared Contract* member; dropping it lets addition stay green: \
         {found:?}"
    );
}

#[test]
fn error_code_parser_counts_a_trailing_comment_variant() {
    let src = "pub enum ErrorCode {\n    ContractRogueCommented, // new\n}";
    let found = error_code_unit_variants_with_prefix_from(src, "Contract");
    assert!(
        found.contains("ContractRogueCommented"),
        "a trailing-comment unit variant is live; dropping it lets addition stay green: {found:?}"
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
const CONTRACT_BOUNDARY: &str = "crates/assay-cli/src/exit_codes/evidence_contract_boundary.md";
const LIMIT_BOUNDARY: &str = "crates/assay-cli/src/exit_codes/evidence_limit_boundary.md";
const PATH_BOUNDARY: &str = "crates/assay-cli/src/exit_codes/evidence_path_boundary.md";

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
fn e_evidence_contract_is_registered_and_emittable() {
    // #2219: a format-contract defect is a different fact from a recorded-value mismatch.
    // These two memberships are the registry identity; a rename or a missing §5.1 row
    // must fail here rather than leave consumers to invent a local string.
    assert!(
        emittable_codes().contains("E_EVIDENCE_CONTRACT"),
        "ReasonCode::as_str does not emit E_EVIDENCE_CONTRACT; the #2219 registry gap is still open"
    );
    assert!(
        registered("5.1").contains("E_EVIDENCE_CONTRACT"),
        "{SPEC} §5.1 has no E_EVIDENCE_CONTRACT row"
    );
    assert!(
        !registered("5.4").contains("E_EVIDENCE_CONTRACT"),
        "{SPEC} §5.4 must not keep E_EVIDENCE_CONTRACT reserved once a production site constructs it"
    );
}

#[test]
fn e_evidence_profile_invalid_is_registered_constructed_and_not_reserved() {
    assert!(
        emittable_codes().contains("E_EVIDENCE_PROFILE_INVALID"),
        "ReasonCode::as_str does not emit E_EVIDENCE_PROFILE_INVALID"
    );
    assert!(
        registered("5.1").contains("E_EVIDENCE_PROFILE_INVALID"),
        "{SPEC} §5.1 has no E_EVIDENCE_PROFILE_INVALID row"
    );
    assert!(
        !registered("5.4").contains("E_EVIDENCE_PROFILE_INVALID"),
        "{SPEC} §5.4 must not reserve E_EVIDENCE_PROFILE_INVALID; register and construct it atomically"
    );
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
fn the_evidence_contract_boundary_reads_the_same_in_the_spec_and_the_code() {
    assert_eq!(
        spec_description("5.1", "E_EVIDENCE_CONTRACT"),
        flowed(&read(CONTRACT_BOUNDARY)),
        "the §5.1 registry row is no longer the text of {CONTRACT_BOUNDARY}. That file is the \
         boundary; the row and the `ReasonCode::EEvidenceContract` doc comment transport it. \
         Edit the file and reflow it into the row rather than editing the row."
    );
}

#[test]
fn the_integrity_boundary_no_longer_states_the_contract_gap() {
    let boundary = read(BOUNDARY);
    assert!(
        !boundary.contains("Stated gap") && !boundary.contains("#2219 tracks"),
        "{BOUNDARY} still describes format-contract as an unregistered gap"
    );
    assert!(
        boundary.contains("E_EVIDENCE_CONTRACT"),
        "{BOUNDARY} must point Contract* defects at E_EVIDENCE_CONTRACT"
    );
}

const REGISTRY_HEADER: &str = "pub enum ReasonCode {";
const INTEGRITY_DECL: &str = "\n    EEvidenceIntegrity,";
const CONTRACT_DECL: &str = "\n    EEvidenceContract,";
const BOUNDARY_INCLUDE: &str =
    "#[doc = include_str!(\"exit_codes/evidence_integrity_boundary.md\")]";
const CONTRACT_INCLUDE: &str =
    "#[doc = include_str!(\"exit_codes/evidence_contract_boundary.md\")]";
const LIMIT_DECL: &str = "\n    EEvidenceLimitExceeded,";
const PATH_DECL: &str = "\n    EEvidencePathRejected,";
const LIMIT_INCLUDE: &str = "#[doc = include_str!(\"exit_codes/evidence_limit_boundary.md\")]";
const PATH_INCLUDE: &str = "#[doc = include_str!(\"exit_codes/evidence_path_boundary.md\")]";

/// `text` with its string literals removed, and the number of brackets it leaves open.
///
/// The literals go first so that what is written *about* an attribute is never read as part of it:
/// `#[serde(rename = "doc")]` names no `doc` path, and a `)` inside a note is not a closing
/// bracket. `None` when the text closes a bracket it never opened or ends inside a literal —
/// neither is a complete attribute, and both are refused rather than guessed at.
fn code_of(text: &str) -> Option<(String, i32)> {
    let mut code = String::with_capacity(text.len());
    let mut depth = 0i32;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => loop {
                match chars.next()? {
                    '\\' => {
                        chars.next()?;
                    }
                    '"' => break,
                    _ => {}
                }
            },
            '(' | '[' | '{' => {
                depth += 1;
                code.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
                code.push(c);
            }
            _ => code.push(c),
        }
    }
    Some((code, depth))
}

/// The identifiers in `code`, in source order. The first is the attribute's path.
fn identifiers(code: &str) -> Vec<&str> {
    code.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|token| !token.is_empty())
        .collect()
}

/// Can this unit of source put text on the variant it precedes?
///
/// Two channels reach the rendered variant and both are refused. The `doc` namespace is written by
/// `///`, `//!`, `/** */` and by an attribute whose path is `doc`; `#[deprecated]`'s note is
/// written into the item-info stab, which rustdoc renders *ahead of* the doc block. A `cfg_attr`
/// naming either path expands to one, so it goes with them.
///
/// The rule is the attribute's *path*, not the letters in the line. Asking whether the line
/// contains `doc` refuses `#[allow(rustdoc::bare_urls)]`, which documents nothing, and a guard
/// that fails on correct code is weakened by the next author who trips over it.
///
/// Admitted: a blank line, a `//` comment, and an attribute whose path is neither of those two,
/// whether rustfmt kept it on one line or broke it across several. The one `doc`-path exception is
/// `#[doc(alias = "…")]`, whose whole content is a search-index alias and which puts no prose on
/// the item; `#[doc(hidden)]` is not admitted, because it removes the page the boundary text is
/// meant to reach. Anything else this does not recognise — an attribute whose brackets do not
/// balance, a `/* */` block, a bare token — is refused, since an unfamiliar shape is likelier to
/// be a new way of documenting the variant than a new way of not documenting it.
fn cannot_document(unit: &str) -> bool {
    let unit = unit.trim();
    if unit.is_empty() {
        return true;
    }
    if unit.starts_with("//") {
        return !unit.starts_with("///") && !unit.starts_with("//!");
    }
    let Some(attribute) = unit
        .strip_prefix("#[")
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return false;
    };
    let Some((code, 0)) = code_of(attribute) else {
        return false;
    };
    let tokens = identifiers(&code);
    match tokens.first() {
        Some(&"doc") => tokens == ["doc", "alias"],
        Some(&"deprecated") => false,
        Some(&"cfg_attr") => !tokens
            .iter()
            .any(|token| *token == "doc" || *token == "deprecated"),
        Some(_) => true,
        None => false,
    }
}

/// The lines above the variant, grouped into the units rustc reads: an attribute rustfmt has
/// broken across source lines is one unit, every other line is a unit of its own, and each unit
/// carries the index of its last line.
///
/// Grouping is what lets the rule above be about the attribute rather than about the line, so that
/// a wrapped `#[cfg_attr(feature = "serde", serde(alias = "…"))]` is read as the one attribute it
/// is. An attribute whose brackets never balance runs to the end of the region and is refused
/// there, which is the direction every unrecognised shape goes.
fn source_units(lines: &[&str]) -> Vec<(usize, String)> {
    let mut units = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        let mut last = start;
        let mut text = lines[start].trim().to_string();
        if text.starts_with("#[") {
            while code_of(&text).map(|(_, depth)| depth) != Some(0) && last + 1 < lines.len() {
                last += 1;
                text.push(' ');
                text.push_str(lines[last].trim());
            }
        }
        units.push((last, text));
        start = last + 1;
    }
    units
}

/// Is this line a unit-variant declaration, i.e. the end of the previous variant's own text?
///
/// A trailing `// …` is dropped first. rustfmt keeps one, so a variant carrying a note about
/// itself would otherwise not be recognised as the line the walk stops on, and the failure would
/// name a line the author has no reason to connect to this variant.
fn declares_a_variant(line: &str) -> bool {
    let code = match line.find("//") {
        Some(at) => &line[..at],
        None => line,
    };
    let Some(name) = code.trim().strip_suffix(',') else {
        return false;
    };
    name.starts_with(|c: char| c.is_ascii_uppercase()) && name.chars().all(char::is_alphanumeric)
}

fn assert_variant_documented_only_by_include(decl: &str, include: &str, boundary: &str) {
    let src = read("crates/assay-cli/src/exit_codes.rs");
    assert_eq!(
        src.matches(REGISTRY_HEADER).count(),
        1,
        "`{REGISTRY_HEADER}` occurs more than once in crates/assay-cli/src/exit_codes.rs, so the \
         slice read below is not necessarily the registry"
    );
    let body = src
        .split_once(REGISTRY_HEADER)
        .and_then(|(_, body)| body.split_once("\n}"))
        .map(|(body, _)| body)
        .expect("`pub enum ReasonCode` has no closing brace");
    let name = decl.trim().trim_end_matches(',');
    assert_eq!(
        body.matches(decl).count(),
        1,
        "`{name},` occurs {} times in the `ReasonCode` body; the declaration this reads is then \
         not necessarily the one it names",
        body.matches(decl).count()
    );
    let head = body
        .split_once(decl)
        .map(|(above_decl, _)| above_decl)
        .unwrap_or_else(|| panic!("no `{name}` variant in `ReasonCode`"));
    let lines: Vec<&str> = head.lines().collect();
    let units = source_units(&lines);
    let previous_variant = units
        .iter()
        .rposition(|(_, unit)| !(cannot_document(unit) || unit.as_str() == include));
    let own_text: &[&str] = match previous_variant {
        Some(index) => {
            let (last_line, unit) = &units[index];
            assert!(
                declares_a_variant(unit),
                "`{name}`'s own text reaches back to {unit:?}, which does not declare the \
                 previous variant. Every line between the two variants documents this one, so \
                 anything there that is not the boundary include is a second copy of the rule, \
                 free to contradict it while the §5.1 row stays correct. Blank lines, `//` \
                 comments, and attributes whose path is neither `doc` nor `deprecated` — on one \
                 line or broken across several — carry no text and are fine on either side of the \
                 include; a `///` or `/** */` comment, a `#[doc = ...]`, a `#[deprecated]` note, \
                 any `cfg_attr` naming either path, and any attribute whose brackets do not \
                 balance are refused here rather than guessed at."
            );
            &lines[last_line + 1..]
        }
        None => &lines,
    };
    assert_eq!(
        own_text
            .iter()
            .filter(|line| line.trim() == include)
            .count(),
        1,
        "`{name}` is documented by {own_text:?} rather than by including {boundary} exactly once"
    );
}

#[test]
fn nothing_but_the_boundary_include_documents_the_integrity_variant() {
    // `include_str!` is what makes the equality above cover the doc comment too. Replace it with a
    // hand-written `///` block and the row could be pinned while the definition drifts.
    //
    // The whole run of lines back to the previous variant is checked, not the line nearest the
    // declaration. Checking only that line asserts "the include is present, adjacent to the
    // variant", which is a neighbouring property: rustc concatenates `///` lines and `#[doc]`
    // attributes on one item in source order, so a `///` line placed *above* the include also
    // documents this variant and renders ahead of the boundary text in the shipped rustdoc while
    // leaving that line untouched. That gap re-admitted a withdrawn "Merkle root ... inclusion
    // proofs" claim past the test written to forbid it, because that test reads the fragment and
    // the claim was not in the fragment.
    //
    // Walking up while a line *looks like* documentation is the same neighbouring property one
    // step out: the walk stops at the first line without a `///` or `#[doc` prefix, and a
    // continuation line, a `/**` opener and a `#[cfg_attr(` opener are all such lines, so live doc
    // text above any of them was collected as absent. The walk therefore runs on units that
    // *cannot* carry text and stops everywhere else, which makes an unrecognised shape a failure
    // instead of a pass, and it must stop on the previous variant's declaration — anything else
    // between the two variants is the second copy of the rule this forbids.
    //
    // Both rendered channels are refused, not just the `doc` namespace: `#[deprecated]`'s note is
    // markdown-rendered into the item-info stab *ahead of* the doc block, which is this test's own
    // failure shape reached through an attribute that is not a doc attribute.
    //
    // What this does not pin, so that it is not read as more than it is: doc text on `enum
    // ReasonCode` itself, which documents the enum rather than this variant; the rendering step,
    // since it reads the source rustdoc is given and not rustdoc's output; and an attribute macro,
    // which expands to source this never sees.
    assert_variant_documented_only_by_include(INTEGRITY_DECL, BOUNDARY_INCLUDE, BOUNDARY);
}

#[test]
fn nothing_but_the_boundary_include_documents_the_contract_variant() {
    assert_variant_documented_only_by_include(CONTRACT_DECL, CONTRACT_INCLUDE, CONTRACT_BOUNDARY);
}

#[test]
fn the_boundary_states_the_canonical_flat_digest_formula() {
    let boundary = flowed(&read(BOUNDARY));
    assert!(
        boundary.contains(
            "`run_root` is SHA-256 over newline-delimited event content-hash strings, with a \
             trailing newline, in event sequence order."
        ),
        "{BOUNDARY} must state the canonical run_root formula"
    );
    assert!(
        boundary.contains("flat digest"),
        "{BOUNDARY} must identify run_root as a flat digest"
    );
    assert!(
        !boundary.contains("hash chain")
            && !boundary.contains("integrity chain")
            && !boundary.contains("concatenated"),
        "{BOUNDARY} must not describe run_root as a chain or delimiter-free concatenation"
    );
    assert!(
        !boundary.contains("Merkle"),
        "{BOUNDARY} must not name a structure the verifier does not build"
    );
}

#[test]
fn e_evidence_limit_exceeded_is_registered_and_emittable() {
    // #2415: a ceiling refusal is not a finding about the bundle. Registering the identity is what
    // stops a consumer inventing a local string or folding the refusal into a content code.
    assert!(
        emittable_codes().contains("E_EVIDENCE_LIMIT_EXCEEDED"),
        "ReasonCode::as_str does not emit E_EVIDENCE_LIMIT_EXCEEDED; the #2415 gap is still open"
    );
    assert!(
        registered("5.1").contains("E_EVIDENCE_LIMIT_EXCEEDED"),
        "{SPEC} §5.1 has no E_EVIDENCE_LIMIT_EXCEEDED row"
    );
    assert!(
        registered("5.4").contains("E_EVIDENCE_LIMIT_EXCEEDED"),
        "{SPEC} §5.4 must keep E_EVIDENCE_LIMIT_EXCEEDED reserved until a production site \
         constructs it"
    );
}

#[test]
fn e_evidence_path_rejected_is_registered_and_emittable() {
    // #2415: an archive-path refusal establishes that a member path was unsafe to extract, and
    // nothing about who produced it or why. It needs its own identity for the same reason.
    assert!(
        emittable_codes().contains("E_EVIDENCE_PATH_REJECTED"),
        "ReasonCode::as_str does not emit E_EVIDENCE_PATH_REJECTED; the #2415 gap is still open"
    );
    assert!(
        registered("5.1").contains("E_EVIDENCE_PATH_REJECTED"),
        "{SPEC} §5.1 has no E_EVIDENCE_PATH_REJECTED row"
    );
    assert!(
        registered("5.4").contains("E_EVIDENCE_PATH_REJECTED"),
        "{SPEC} §5.4 must keep E_EVIDENCE_PATH_REJECTED reserved until a production site \
         constructs it"
    );
}

#[test]
fn the_limit_boundary_names_exactly_the_declared_limit_star_variants() {
    // Derived on both sides, so a new `Limit*` variant is a mapping decision rather than an
    // automatic member, and dropping one from the boundary fails rather than narrowing the rule
    // silently. The boundary states the class and makes no reachability claim: which consumer
    // reaches which code is a property of call graphs, and pinning it here would be one more
    // thing to go stale in normative text.
    let declared = error_code_unit_variants_with_prefix("Limit");
    let named = backtick_identifiers_with_prefix(&read(LIMIT_BOUNDARY), "Limit");
    assert_eq!(
        declared, named,
        "{LIMIT_BOUNDARY} and ErrorCode disagree on the Limit* set. A new variant is a new \
         mapping decision: name it here in the same change, or say why it is excluded."
    );
}

#[test]
fn the_path_boundary_names_exactly_the_declared_security_star_variants() {
    // The path code owns the whole `Security*` prefix, so the same derivation applies. A future
    // `Security*` variant that is not an archive-path fact must not join by prefix alone.
    let declared = error_code_unit_variants_with_prefix("Security");
    let named = backtick_identifiers_with_prefix(&read(PATH_BOUNDARY), "Security");
    assert_eq!(
        declared, named,
        "{PATH_BOUNDARY} and ErrorCode disagree on the Security* set. A new variant is a new \
         mapping decision: name it here in the same change, or say why it is excluded."
    );
}

#[test]
fn both_new_boundaries_require_the_class_as_well_as_the_code() {
    // A code prefix alone is not the key. `impl From<serde_json::Error>` already shows a class
    // reaching a code that means something else, so an emitter that keys on the prefix without
    // the class can classify a failure the verifier never attributed to that class.
    // Keying on the MUST clause, not on mere presence: both files also name their class in the
    // opening description, so `contains("ErrorClass::Limits")` stays true after the requirement
    // itself is deleted. That weaker assertion was written first and a mutation survived it.
    let limit = read(LIMIT_BOUNDARY);
    assert!(
        flowed(&limit).contains("MUST key on `ErrorClass::Limits` together with"),
        "{LIMIT_BOUNDARY} must require the class as well as the code prefix, in the MUST clause"
    );
    let path = read(PATH_BOUNDARY);
    assert!(
        flowed(&path).contains("MUST key on `ErrorClass::Security` together with"),
        "{PATH_BOUNDARY} must require the class as well as the code prefix, in the MUST clause"
    );
}

#[test]
fn neither_new_boundary_names_a_code_from_another_class() {
    // The set tests are prefix-scoped, so a `Contract*` name pasted into the limit boundary is
    // invisible to them: it is neither a missing member nor an extra one. A mutation adding
    // `ContractInvalidJson` to this file survived until this assertion existed.
    for (file, own) in [(LIMIT_BOUNDARY, "Limit"), (PATH_BOUNDARY, "Security")] {
        let text = read(file);
        for foreign in ["Integrity", "Contract", "Limit", "Security"] {
            if foreign == own {
                continue;
            }
            let named = backtick_identifiers_with_prefix_allowing_none(&text, foreign);
            assert!(
                named.is_empty(),
                "{file} names {foreign}* ErrorCode identifiers {named:?}; this boundary maps one \
                 class, and naming another class's codes here invites an emitter to fold them in"
            );
        }
    }
}

#[test]
fn the_evidence_limit_boundary_reads_the_same_in_the_spec_and_the_code() {
    assert_eq!(
        spec_description("5.1", "E_EVIDENCE_LIMIT_EXCEEDED"),
        flowed(&read(LIMIT_BOUNDARY)),
        "the §5.1 row and {LIMIT_BOUNDARY} state the same rule; they must not drift apart"
    );
}

#[test]
fn the_evidence_path_boundary_reads_the_same_in_the_spec_and_the_code() {
    assert_eq!(
        spec_description("5.1", "E_EVIDENCE_PATH_REJECTED"),
        flowed(&read(PATH_BOUNDARY)),
        "the §5.1 row and {PATH_BOUNDARY} state the same rule; they must not drift apart"
    );
}

#[test]
fn nothing_but_the_boundary_include_documents_the_limit_variant() {
    assert_variant_documented_only_by_include(LIMIT_DECL, LIMIT_INCLUDE, LIMIT_BOUNDARY);
}

#[test]
fn nothing_but_the_boundary_include_documents_the_path_variant() {
    assert_variant_documented_only_by_include(PATH_DECL, PATH_INCLUDE, PATH_BOUNDARY);
}
