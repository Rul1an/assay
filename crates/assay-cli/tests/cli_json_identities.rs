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

/// The `not-cli-documents` block, as `identity -> reason`.
///
/// The reason is mandatory. A static check cannot tell a document from an event when the naming and
/// the writing sit in different files, which is true of several of these. What it can do is make a
/// misclassification require someone to write a false sentence instead of deleting a line.
fn non_document_rows() -> BTreeMap<String, String> {
    let mut rows = BTreeMap::new();
    for line in recorded("not-cli-documents") {
        let (ident, reason) = line.split_once('|').unwrap_or_else(|| {
            panic!("{INVENTORY}: `not-cli-documents` row is not `identity | reason`: {line:?}")
        });
        let (ident, reason) = (ident.trim(), reason.trim());
        assert!(
            !reason.is_empty(),
            "{INVENTORY}: {ident:?} is recorded as a non-document with no reason"
        );
        assert!(
            rows.insert(ident.to_string(), reason.to_string()).is_none(),
            "{INVENTORY}: `not-cli-documents` records {ident:?} twice"
        );
    }
    rows
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

/// Files that are test-only because a `#[cfg(test)] mod name;` declaration brings them in.
///
/// The brace-matching stripper only removes `#[cfg(test)] mod name { … }` written inline. Thirty
/// modules in these two crates are declared as a bare `#[cfg(test)] mod name;` instead, and their
/// files sit in `src/` looking exactly like production. Scanning them collected identities that no
/// shipping code writes — which is the failure this whole file exists to stop, found inside the
/// guard itself.
fn test_only_files() -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    let mut queue: Vec<PathBuf> = Vec::new();
    for dir in SCANNED {
        let base = workspace_root().join(dir);
        let mut stack = vec![base];
        while let Some(path) = stack.pop() {
            for entry in std::fs::read_dir(&path).expect("read_dir") {
                let p = entry.expect("dir entry").path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
                    for name in test_mod_declarations(&std::fs::read_to_string(&p).expect("read")) {
                        queue.extend(module_files(&p, &name));
                    }
                }
            }
        }
    }
    // A test-only module's own submodules are test-only too.
    while let Some(file) = queue.pop() {
        if !out.insert(file.clone()) {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&file) else {
            continue;
        };
        for name in declared_modules(&src) {
            queue.extend(module_files(&file, &name));
        }
    }
    out
}

/// `name` for each `#[cfg(test)] mod name;` in this source.
fn test_mod_declarations(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut lines = src.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let Some(next) = lines.peek() else { continue };
        if let Some(name) = bare_mod_name(next) {
            names.push(name);
        }
    }
    names
}

/// `name` for every `mod name;` in this source, cfg-gated or not.
fn declared_modules(src: &str) -> Vec<String> {
    src.lines().filter_map(bare_mod_name).collect()
}

fn bare_mod_name(line: &str) -> Option<String> {
    let line = line.trim();
    let rest = line
        .strip_prefix("mod ")
        .or_else(|| line.strip_prefix("pub mod "))
        .or_else(|| line.strip_prefix("pub(crate) mod "))
        .or_else(|| line.strip_prefix("pub(super) mod "))?;
    let name = rest.strip_suffix(';')?;
    name.chars()
        .all(|c| c.is_alphanumeric() || c == '_')
        .then(|| name.to_string())
}

/// The candidate files a `mod name;` inside `declaring` resolves to.
fn module_files(declaring: &std::path::Path, name: &str) -> Vec<PathBuf> {
    let dir = if declaring.file_name().and_then(|f| f.to_str()) == Some("mod.rs")
        || declaring.file_name().and_then(|f| f.to_str()) == Some("main.rs")
        || declaring.file_name().and_then(|f| f.to_str()) == Some("lib.rs")
    {
        declaring.parent().map(|p| p.to_path_buf())
    } else {
        declaring
            .parent()
            .map(|p| p.join(declaring.file_stem().and_then(|s| s.to_str()).unwrap_or("")))
    };
    let Some(dir) = dir else { return Vec::new() };
    [
        dir.join(format!("{name}.rs")),
        dir.join(name).join("mod.rs"),
    ]
    .into_iter()
    .filter(|p| p.exists())
    .collect()
}

/// Every `"assay.<segments>.vN"` string literal in production source, with one citing site.
///
/// Both `const NAME: &str = "…"` and inline literals, because the codebase writes both and the
/// difference is a style choice rather than a property of the document.
fn identities_in_source() -> BTreeMap<String, String> {
    let root = workspace_root();
    let test_only = test_only_files();
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
                    if test_only.contains(&p) {
                        continue;
                    }
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

/// Writer idioms. A file that contains none of these does not emit anything, so a row naming it as
/// the writer of a document is wrong.
const WRITERS: &[&str] = &[
    "write_stdout_json",
    "to_string_pretty",
    "to_writer",
    "std::fs::write",
    "println!",
];

/// `identity -> writing command file`, from the `cli-documents` block.
fn document_rows() -> BTreeMap<String, (String, String)> {
    let mut rows = BTreeMap::new();
    for line in recorded("cli-documents") {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        assert_eq!(
            parts.len(),
            3,
            "{INVENTORY}: `cli-documents` row is not `identity | writer | namer`: {line:?}"
        );
        let namer = if parts[2] == "-" { parts[1] } else { parts[2] };
        assert!(
            rows.insert(
                parts[0].to_string(),
                (parts[1].to_string(), namer.to_string())
            )
            .is_none(),
            "{INVENTORY}: `cli-documents` records {:?} twice",
            parts[0]
        );
    }
    rows
}

/// Every identity in source is recorded as a document or as not-a-document, and nothing else is.
#[test]
fn every_production_identity_is_classified() {
    let documents: BTreeSet<String> = document_rows().keys().cloned().collect();
    let others: BTreeSet<String> = non_document_rows().keys().cloned().collect();

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

    // A document row may name an identity declared outside the scanned crates — `assay-evidence`
    // declares one that the CLI writes. Following the write rather than the crate is deliberate,
    // and `documents_are_bound_to_a_writer` is what keeps such a row honest. Non-document rows have
    // no write to bind to, so a stale one there is simply stale.
    let stale: Vec<_> = others.difference(&collected).cloned().collect();
    assert!(
        stale.is_empty(),
        "{INVENTORY} records non-documents that no longer appear in production source: {stale:?}"
    );
    let _ = &recorded;
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
    let documents: BTreeSet<String> = document_rows().keys().cloned().collect();
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

/// Every recorded document names a file that both knows the identity and writes something.
///
/// Membership alone let six identities sit in the wrong block because a word in their name —
/// "carrier", "projection", "health artifact" — read as not-a-document. The partition stayed green,
/// since it only asked whether each identity appeared *somewhere*. Binding the row to a write is
/// what makes that class of mistake fail.
#[test]
fn documents_are_bound_to_a_writer() {
    for (identity, (writer, namer)) in document_rows() {
        let naming = read(&namer);
        let names_it = naming.contains(&format!("\"{identity}\""))
            || constant_names_for(&identity)
                .iter()
                .any(|name| naming.contains(name.as_str()));
        assert!(
            names_it,
            "{INVENTORY}: {identity} is recorded as named in {namer}, and that file neither \
             contains the literal nor mentions a constant holding it"
        );
        let writing = read(&writer);
        assert!(
            WRITERS.iter().any(|w| writing.contains(w)),
            "{INVENTORY}: {identity} is recorded as written by {writer}, and that file calls no \
             writer ({WRITERS:?}). Either it is not the writer, or it is not a document"
        );
    }
}

/// Constant names anywhere in the scanned source whose value is this identity.
fn constant_names_for(identity: &str) -> Vec<String> {
    let mut names = Vec::new();
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
                    for line in text.lines() {
                        if !line.contains(&format!("\"{identity}\"")) {
                            continue;
                        }
                        if let Some(rest) = line.trim().split("const ").nth(1) {
                            if let Some(name) = rest.split(':').next() {
                                names.push(name.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    names
}
