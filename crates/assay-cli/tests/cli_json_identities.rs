//! The CLI JSON identity inventory in `docs/architecture/CLI-JSON-IDENTITIES.md` is checked here.
//!
//! #2167 settled one convention for public CLI JSON documents: the discriminator is `schema`,
//! carrying `assay.<segments>.vN`. Nothing held the *set* of those identities together — an
//! emitter could arrive and no record would notice, which is how two counts of that set were
//! published and withdrawn before this guard existed.
//!
//! Three properties, and the third is the one that matters:
//!
//! 1. Every identity string in production source is recorded, in the block that says what it is.
//!    The two blocks are a partition, so "in neither" is a failure rather than a silence.
//! 2. An identity written as an inline literal counts. Seven shipping verify-report documents are
//!    declared that way, so a `const`-only collector would be satisfied by a coding convention
//!    this codebase does not follow.
//! 3. Documents that carry *no* identity are required rows. Measured: coverage, session-state
//!    window, soak, `Baseline`, `HygieneReport`, both `run.json` writers, the sim-run report and
//!    SARIF have zero identity constants between them. A pin built from a source scan alone
//!    satisfies (1) and (2) while omitting every document #2485 has to migrate, and stays green.
//!
//! The inventory is never generated — not in CI and not locally-then-committed. This test reads it
//! and never writes it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const INVENTORY: &str = "docs/architecture/CLI-JSON-IDENTITIES.md";

/// Crates whose production sources are scanned.
///
/// Both crates entirely, not a subdirectory of either. Scoping to `assay-core/src/report` hid
/// `assay.run_report.v1`'s siblings under `mcp/`, `otel/` and `discovery/`, which is the same
/// class of mistake this file exists to catch.
const SCANNED: &[&str] = &["crates/assay-cli/src", "crates/assay-core/src"];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = workspace_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The lines recorded under a `<!-- machine-checked: NAME -->` marker.
///
/// Anchored on an HTML comment rather than a heading, so the prose can be rewritten without
/// silently changing what is checked.
fn recorded(marker: &str) -> Vec<String> {
    let doc = read(INVENTORY);
    let needle = format!("<!-- machine-checked: {marker} -->");
    let after = doc
        .split_once(&needle)
        .unwrap_or_else(|| panic!("{INVENTORY} has no `{needle}` marker"))
        .1;
    let block = after
        .split_once("```text\n")
        .unwrap_or_else(|| panic!("marker `{marker}` is not followed by a ```text block"))
        .1;
    let block = block
        .split_once("\n```")
        .unwrap_or_else(|| panic!("marker `{marker}` block is unterminated"))
        .0;
    block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn recorded_set(marker: &str) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for line in recorded(marker) {
        assert!(
            set.insert(line.clone()),
            "{INVENTORY}: `{marker}` records {line:?} twice"
        );
    }
    set
}

/// Source with `#[cfg(test)] mod … { … }` bodies removed.
///
/// A test module inside a production file is not a producer. It is removed by brace matching
/// rather than by a path or filename rule, because `supply_chain_conformance/tests.rs` is a file
/// named `tests.rs` that sits in `src/`: any name-based rule has to decide it, and deciding it
/// wrongly is silent in both directions.
fn strip_test_modules(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some((attr, body)) = find_test_mod(rest) {
        out.push_str(&rest[..attr]);
        rest = &rest[skip_balanced(rest, body)..];
    }
    out.push_str(rest);
    out
}

/// `(offset of `#[cfg(test)]`, offset just past its `mod … {`)`, if the attribute introduces a
/// module. An attribute on anything else — a `fn`, a `use` — is left alone.
fn find_test_mod(src: &str) -> Option<(usize, usize)> {
    const ATTR: &str = "#[cfg(test)]";
    let mut from = 0usize;
    while let Some(rel) = src[from..].find(ATTR) {
        let attr = from + rel;
        let after = &src[attr + ATTR.len()..];
        let trimmed = after.trim_start();
        if let Some(tail) = trimmed.strip_prefix("mod ") {
            if let Some(brace) = tail.find('{') {
                if tail[..brace]
                    .trim()
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_')
                {
                    let consumed = after.len() - trimmed.len() + "mod ".len() + brace + 1;
                    return Some((attr, attr + ATTR.len() + consumed));
                }
            }
        }
        from = attr + ATTR.len();
    }
    None
}

/// Offset just past the `}` that closes the block whose body starts at `from`.
///
/// Braces inside string literals, char literals and comments are not braces. Counting them was
/// this function's first bug: a production file containing `"{"` made the depth never reach zero,
/// and the failure surfaced as an assertion about unbalanced source rather than as a wrong answer.
fn skip_balanced(src: &str, from: usize) -> usize {
    let bytes = src.as_bytes();
    let mut i = from;
    let mut depth = 1usize;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'"' => i = skip_string(bytes, i),
            b'\'' => i = skip_char(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    assert_eq!(depth, 0, "a `#[cfg(test)] mod` block is never closed");
    i
}

/// Offset of the closing quote of the string literal opening at `open`. Raw strings included:
/// `r#"…"#` bodies may contain unescaped quotes and braces.
fn skip_string(bytes: &[u8], open: usize) -> usize {
    let mut hashes = 0usize;
    let mut back = open;
    while back > 0 && bytes[back - 1] == b'#' {
        hashes += 1;
        back -= 1;
    }
    let raw = hashes > 0 && back > 0 && bytes[back - 1] == b'r';
    let mut i = open + 1;
    if raw {
        while i < bytes.len() {
            if bytes[i] == b'"' && bytes[i + 1..].iter().take(hashes).all(|b| *b == b'#') {
                return i + hashes;
            }
            i += 1;
        }
        return bytes.len();
    }
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'"' => return i,
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// Offset of the closing quote of a char literal, or `open` itself when the quote is a lifetime.
fn skip_char(bytes: &[u8], open: usize) -> usize {
    let mut i = open + 1;
    if bytes.get(i) == Some(&b'\\') {
        i += 1;
    }
    match bytes.get(i + 1) {
        Some(b'\'') => i + 1,
        _ => open,
    }
}

/// Every `"assay.<segments>.vN"` string literal in production source, with one citing site.
///
/// Both `const NAME: &str = "…"` and inline literals, because the codebase writes both and the
/// difference is a style choice rather than a property of the document.
fn identities_in_source() -> BTreeMap<String, String> {
    let root = workspace_root();
    let mut found: BTreeMap<String, String> = BTreeMap::new();
    for dir in SCANNED {
        let base = root.join(dir);
        let mut stack = vec![base.clone()];
        while let Some(path) = stack.pop() {
            let entries = std::fs::read_dir(&path)
                .unwrap_or_else(|e| panic!("read_dir {}: {e}", path.display()));
            for entry in entries {
                let entry = entry.expect("dir entry");
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
                    let src = std::fs::read_to_string(&p)
                        .unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
                    let src = strip_test_modules(&src);
                    for (lineno, line) in src.lines().enumerate() {
                        if line.trim_start().starts_with("//") {
                            continue;
                        }
                        for ident in identities_in_line(line) {
                            let rel = p.strip_prefix(&root).unwrap_or(&p).display();
                            found
                                .entry(ident)
                                .or_insert_with(|| format!("{rel}:{}", lineno + 1));
                        }
                    }
                }
            }
        }
    }
    assert!(
        !found.is_empty(),
        "collected no identities; the scan shape moved"
    );
    found
}

/// `"assay.…vN"` literals on one line.
///
/// A trailing `.v<digits>` segment is required, so a prefix constant such as `"assay.receipt."`
/// is not mistaken for an identity.
fn identities_in_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let Some(rel_end) = line[start..].find('"') else {
            break;
        };
        let literal = &line[start..start + rel_end];
        i = start + rel_end + 1;
        if !literal.starts_with("assay.") {
            continue;
        }
        let Some(last) = literal.rsplit('.').next() else {
            continue;
        };
        let is_generation = last.len() > 1
            && last.starts_with('v')
            && last[1..].chars().all(|c| c.is_ascii_digit());
        if !is_generation {
            continue;
        }
        if literal
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'))
        {
            continue;
        }
        out.push(literal.to_string());
    }
    out
}

/// Every identity in source is recorded as a document or as not-a-document, and nothing else is.
#[test]
fn every_production_identity_is_classified() {
    let documents = recorded_set("cli-documents");
    let others = recorded_set("not-cli-documents");

    let overlap: Vec<_> = documents.intersection(&others).cloned().collect();
    assert!(
        overlap.is_empty(),
        "{INVENTORY}: these are recorded as both a document and not a document: {overlap:?}"
    );

    let in_source = identities_in_source();
    let recorded: BTreeSet<String> = documents.union(&others).cloned().collect();
    let collected: BTreeSet<String> = in_source.keys().cloned().collect();

    let unrecorded: Vec<String> = collected
        .difference(&recorded)
        .map(|id| format!("{id} ({})", in_source[id]))
        .collect();
    assert!(
        unrecorded.is_empty(),
        "these identities ship but are in neither block of {INVENTORY}. Add each one to the block \
         that says what it is — a document a CLI command emits, or an event / nested object / \
         input / digest domain:\n  {}",
        unrecorded.join("\n  ")
    );

    let stale: Vec<_> = recorded.difference(&collected).cloned().collect();
    assert!(
        stale.is_empty(),
        "{INVENTORY} records identities that no longer appear in production source: {stale:?}"
    );
}

/// Documents with no identity string are required rows, and each row still names something real.
///
/// Without this, the guard is satisfied by a source scan — and a source scan cannot see any of the
/// documents #2485 migrates, because not one of them has an identity constant.
#[test]
fn documents_without_an_identity_are_recorded_and_still_exist() {
    const REQUIRED: &[&str] = &[
        "baseline",
        "baseline_diff",
        "coverage_report",
        "discover_inventory",
        "hygiene_report",
        "run_json_extended",
        "run_json_minimal",
        "sarif",
        "session_state_window",
        "sim_run_report",
        "soak_report",
        "trust_basis_generate",
    ];

    let mut rows: BTreeMap<String, (String, String)> = BTreeMap::new();
    for line in recorded("unnamed-documents") {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        assert_eq!(
            parts.len(),
            3,
            "{INVENTORY}: `unnamed-documents` row is not `key | producer | token`: {line:?}"
        );
        assert!(
            rows.insert(
                parts[0].to_string(),
                (parts[1].to_string(), parts[2].to_string())
            )
            .is_none(),
            "{INVENTORY}: duplicate `unnamed-documents` key {:?}",
            parts[0]
        );
    }

    let recorded_keys: BTreeSet<&str> = rows.keys().map(String::as_str).collect();
    let required: BTreeSet<&str> = REQUIRED.iter().copied().collect();

    let missing: Vec<_> = required.difference(&recorded_keys).collect();
    assert!(
        missing.is_empty(),
        "{INVENTORY} is missing required rows for documents that carry no identity: {missing:?}. \
         These cannot be collected from source, so dropping the row would leave no trace."
    );
    let unexpected: Vec<_> = recorded_keys.difference(&required).collect();
    assert!(
        unexpected.is_empty(),
        "{INVENTORY} records unnamed documents this test does not require: {unexpected:?}. Add \
         them to REQUIRED in the same commit, so the row cannot be dropped later without failing."
    );

    for (key, (producer, token)) in &rows {
        let path = workspace_root().join(producer);
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{INVENTORY}: row `{key}` names a producer that cannot be read: {producer} ({e})"
            )
        });
        assert!(
            src.contains(token.as_str()),
            "{INVENTORY}: row `{key}` says {producer} carries {token:?}, and it does not. The \
             producer moved; move the row with it."
        );
    }
}

/// `assay describe` may only bind identities the inventory records as documents.
///
/// One direction on purpose. `BINDING_ROWS` is a projection of seven clap paths, not the
/// inventory, and it omits the run report and run summary. That gap is recorded in the inventory
/// prose rather than enforced here, because forcing `describe` to grow is a product decision and
/// this test is a bookkeeping guard.
#[test]
fn describe_binds_only_recorded_documents() {
    let documents = recorded_set("cli-documents");
    let src = read("crates/assay-cli/src/cli/commands/describe/bindings.rs");

    let mut constants = BTreeSet::new();
    for line in src.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("identity: ") {
            constants.insert(rest.trim_end_matches(',').to_string());
        }
    }
    assert!(
        !constants.is_empty(),
        "parsed no `identity:` bindings; the BINDING_ROWS shape moved"
    );

    // Resolve each binding constant to its value through the whole scanned source.
    let mut values = BTreeSet::new();
    for dir in SCANNED {
        let base = workspace_root().join(dir);
        let mut stack = vec![base];
        while let Some(path) = stack.pop() {
            for entry in std::fs::read_dir(&path).expect("read_dir") {
                let p = entry.expect("dir entry").path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
                    let text = std::fs::read_to_string(&p).expect("read");
                    for name in &constants {
                        for line in text.lines() {
                            if line.contains(&format!("const {name}:")) {
                                if let Some(v) = identities_in_line(line).into_iter().next() {
                                    values.insert(v);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    assert_eq!(
        values.len(),
        constants.len(),
        "could not resolve every `BINDING_ROWS` identity constant to a literal: {constants:?} \
         resolved to {values:?}"
    );
    let unrecorded: Vec<_> = values.difference(&documents).cloned().collect();
    assert!(
        unrecorded.is_empty(),
        "`assay describe` binds identities the inventory does not record as documents: \
         {unrecorded:?}"
    );
}
