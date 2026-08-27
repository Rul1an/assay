//! The CLI JSON identity inventory in `docs/architecture/CLI-JSON-IDENTITIES.md` is checked here.
//!
//! #2167 settled one convention for public CLI JSON documents: the discriminator is `schema`,
//! carrying `assay.<segments>.vN`. Nothing held the *set* of those identities together — an
//! emitter could arrive and no record would notice, which is how two counts of that set were
//! published and withdrawn before this guard existed.
//!
//! Four properties, and the third and fourth are the ones that matter:
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
//! 4. The inventory follows rows to writers *and* writers to rows. A production file under
//!    `cli/commands` that serializes JSON through the issue idioms must be named by a
//!    `cli-documents` writer/namer, an `unnamed-documents` producer, or (once classified) an
//!    explicit opt-out. The older one-way check is how `assay.runner.observation_health.v0`
//!    shipped unrecorded.
//!
//! The inventory is never generated — not in CI and not locally-then-committed. This test reads it
//! and never writes it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const INVENTORY: &str = "docs/architecture/CLI-JSON-IDENTITIES.md";

/// Crates whose production sources are scanned.
///
/// Both crates entirely, not a subdirectory of either. Scoping to `assay-core/src/report` hid
/// `assay.run_report.v1`'s siblings under `mcp/`, `otel/` and `discovery/`, which is the same
/// class of mistake this file exists to catch.
const SCANNED: &[&str] = &["crates/assay-cli/src", "crates/assay-core/src"];

/// Crates `assay-cli` depends on. Their `pub const` identities must be classified too.
///
/// Following each write by hand does not notice the next one. `trust-basis.diff` was found that
/// way, and then `runner.observation_health` shipped unrecorded anyway: its string appears in
/// `assay-cli` only inside a `//!` comment and a `#[cfg(test)]` assertion, so nothing here could
/// see it. Requiring every published identity in the import graph to be classified turns the next
/// such crate into a red instead of a review comment.
///
/// Only `pub const` in these crates, never every literal: scanning their full source would flood
/// the partition with kernel events and mandate events that no command has any relationship to.
const DEPENDENCY_CRATES: &[&str] = &[
    "crates/assay-canonical/src",
    "crates/assay-common/src",
    "crates/assay-evidence/src",
    "crates/assay-mcp-server/src",
    "crates/assay-metrics/src",
    "crates/assay-monitor/src",
    "crates/assay-policy/src",
    "crates/assay-registry/src",
    "crates/assay-runner-core/src",
    "crates/assay-runner-linux/src",
    "crates/assay-runner-schema/src",
    "crates/assay-sim/src",
];

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
fn strip_test_modules_at(src: &str, whose: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some((attr, body)) = find_test_mod(rest) {
        out.push_str(&rest[..attr]);
        rest = &rest[skip_balanced(rest, body, whose)..];
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
        // Visibility prefixes are the same ones `bare_mod_name` already accepts for `mod name;`.
        // `#[cfg(test)] pub(crate) mod tests { … }` is still a test module; matching only
        // `mod ` left `skill_supply_chain.rs` looking like a production serializer.
        if let Some(tail) = module_after_visibility(trimmed) {
            if let Some(brace) = tail.find('{') {
                if tail[..brace]
                    .trim()
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_')
                {
                    let prefix_len = trimmed.len() - tail.len();
                    let consumed = after.len() - trimmed.len() + prefix_len + brace + 1;
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
fn skip_balanced(src: &str, from: usize, whose: &str) -> usize {
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
    assert_eq!(
        depth, 0,
        "a `#[cfg(test)] mod` block is never closed in {whose}"
    );
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
    // `r"…"` with zero hashes is raw too. Treating it as escaped was this scanner's second bug:
    // `assay-evidence`'s glob tests contain `r"test\\"`, a raw string ending in a backslash, and
    // reading that backslash as an escape ran the scan past the closing quote and off the end of
    // the module. It surfaced as "a `#[cfg(test)] mod` block is never closed", which is at least a
    // loud failure rather than a wrong answer — the same luck as the first brace bug.
    let raw = back > 0 && bytes[back - 1] == b'r';
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

/// Remainder after an optional visibility prefix and `mod `, if this text introduces a module.
fn module_after_visibility(src: &str) -> Option<&str> {
    src.strip_prefix("mod ")
        .or_else(|| src.strip_prefix("pub mod "))
        .or_else(|| src.strip_prefix("pub(crate) mod "))
        .or_else(|| src.strip_prefix("pub(super) mod "))
}

fn bare_mod_name(line: &str) -> Option<String> {
    let line = line.trim();
    let rest = module_after_visibility(line)?;
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
                    let src = strip_test_modules_at(&src, &p.display().to_string());
                    for (lineno, line) in src.lines().enumerate() {
                        // Trailing comments too, not only whole-line ones. `let x = 1; //
                        // "assay.foo.v0"` would otherwise be collected as a producer. No instance
                        // exists on this head; the hole was pointed out before one did, which is
                        // the only time it is cheap to close.
                        let line = code_before_comment(line);
                        if line.trim().is_empty() {
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
    for (identity, site) in published_identities_in_dependencies() {
        found.entry(identity).or_insert(site);
    }
    assert!(
        !found.is_empty(),
        "collected no identities; the scan shape moved"
    );
    found
}

/// `pub const NAME: &str = "assay.…vN"` in every crate `assay-cli` depends on.
fn published_identities_in_dependencies() -> BTreeMap<String, String> {
    let root = workspace_root();
    let mut found = BTreeMap::new();
    for dir in DEPENDENCY_CRATES {
        let base = root.join(dir);
        if !base.exists() {
            continue;
        }
        let mut stack = vec![base];
        while let Some(path) = stack.pop() {
            for entry in std::fs::read_dir(&path).expect("read_dir") {
                let p = entry.expect("dir entry").path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let src = strip_test_modules_at(
                    &std::fs::read_to_string(&p).expect("read"),
                    &p.display().to_string(),
                );
                for (lineno, line) in src.lines().enumerate() {
                    let line = code_before_comment(line);
                    if !line.trim_start().starts_with("pub const ") {
                        continue;
                    }
                    for identity in identities_in_line(line) {
                        let rel = p.strip_prefix(&root).unwrap_or(&p).display();
                        found
                            .entry(identity)
                            .or_insert_with(|| format!("{rel}:{}", lineno + 1));
                    }
                }
            }
        }
    }
    found
}

/// The part of a line before a `//` that is not inside a string literal.
fn code_before_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut in_string = false;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'\\' if in_string => i += 1,
            b'"' => in_string = !in_string,
            b'/' if !in_string && bytes[i + 1] == b'/' => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
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
    "to_vec_pretty",
    "to_writer",
    "serde_json::to_string(",
    "serde_json::to_vec(",
    "std::fs::write",
    "tokio::fs::write",
    "fs::write",
    "write_document",
    "write_all",
    "writeln!",
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

/// Both artifacts created by `assay mcp manifest` are caller-named CLI documents.
///
/// The total-partition guard cannot infer this semantic distinction: recording an identity in the
/// non-document block still satisfies totality. Pin these two command outputs explicitly so a
/// future reason cannot claim that either one is only an input or nested carrier.
#[test]
fn manifest_promotion_outputs_are_cli_documents() {
    let documents = document_rows();
    let non_documents = non_document_rows();

    for identity in [
        "assay.mcp_manifest_candidate.v0",
        "assay.declared_mcp_manifest.v0",
    ] {
        assert!(
            documents.contains_key(identity),
            "{identity} is written by `assay mcp manifest` to a caller-named path and must be in the cli-documents block"
        );
        assert!(
            !non_documents.contains_key(identity),
            "{identity} cannot also be classified as a non-document"
        );
    }
}

/// Documents with no identity string are required rows, and each row still names something real.
///
/// Without this, the guard is satisfied by a source scan — and a source scan cannot see any of the
/// documents #2485 migrates, because not one of them has an identity constant.
#[test]
fn documents_without_an_identity_are_recorded_and_still_exist() {
    const REQUIRED: &[&str] = &[
        "aee_landlock_seal",
        "baseline",
        "baseline_diff",
        "calibration_report",
        "coverage_legacy",
        "coverage_report",
        "discover_inventory",
        "evidence_attest_dsse",
        "evidence_diff",
        "evidence_lint",
        "evidence_lint_sarif",
        "evidence_list",
        "evidence_list_for_run",
        "evidence_show",
        "evidence_store_status",
        "explain_report",
        "generated_policy",
        "hygiene_report",
        "mcp_config_path",
        "profile_file",
        "profile_perf",
        "profile_show",
        "run_json_extended",
        "run_json_minimal",
        "sarif",
        "session_state_window",
        "signed_tool",
        "sim_run_report",
        "skill_supply_chain_cdx",
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
        assert_writer_is_about_this_identity(&identity, &writer, &namer);
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

/// `DEPENDENCY_CRATES` is the `assay-*` dependency list of `assay-cli`, not a list someone typed.
///
/// The const-in-dependencies rule is only as tight as this array. Adding `assay-new` to the
/// manifest and forgetting it here is the sixth crate, invisible again, with every other test
/// green — the same hole the rule was written to close, one level up. A review pointed that out
/// before it happened, which is the only time it costs one function.
#[test]
fn dependency_crates_match_the_manifest() {
    let manifest = read("crates/assay-cli/Cargo.toml");
    let deps = manifest
        .split_once("\n[dependencies]")
        .expect("assay-cli manifest has no [dependencies] section")
        .1;
    let deps = deps.split_once("\n[").map(|(head, _)| head).unwrap_or(deps);

    let mut declared = BTreeSet::new();
    for line in deps.lines() {
        // `assay-core.workspace = true` and `assay-policy = { … }` are both dependency lines, and
        // a crate name never contains a dot, so the key ends at the first `.`, `=` or space.
        let name = line.split(['.', '=', ' ']).next().unwrap_or("").trim();
        if name.starts_with("assay-") {
            declared.insert(name.to_string());
        }
    }
    assert!(
        !declared.is_empty(),
        "parsed no assay-* dependencies; the manifest shape moved"
    );

    // `assay-core` is scanned in full by SCANNED, so it is not in the pub-const-only list.
    let separately_scanned: BTreeSet<String> = SCANNED
        .iter()
        .filter_map(|dir| dir.strip_prefix("crates/"))
        .filter_map(|rest| rest.strip_suffix("/src"))
        .map(str::to_string)
        .collect();

    let listed: BTreeSet<String> = DEPENDENCY_CRATES
        .iter()
        .filter_map(|dir| dir.strip_prefix("crates/"))
        .filter_map(|rest| rest.strip_suffix("/src"))
        .map(str::to_string)
        .collect();

    let expected: BTreeSet<String> = declared.difference(&separately_scanned).cloned().collect();
    assert_eq!(
        listed, expected,
        "DEPENDENCY_CRATES has drifted from assay-cli/Cargo.toml. Every assay-* dependency is \
         either scanned in full (SCANNED) or scanned for pub const identities (DEPENDENCY_CRATES); \
         a dependency in neither is a crate whose published identities nothing classifies"
    );
}

/// The writer file must be about *this* identity, not merely about writing.
///
/// `documents_are_bound_to_a_writer` asks whether the named file contains any writer idiom. That
/// leaves `assay.foo.v0` bindable to a file that writes `assay.bar.v0` and happens to `println!`.
/// A review measured the gap: six rows do not contain their identity in the writer at all, and all
/// six are the write/name splits.
///
/// Two cases, no dataflow:
///
/// - the writer also names the identity (`namer` is `-`): it must contain the literal, or a
///   constant whose value is the literal. Almost every row.
/// - the naming file is separate: the writer must mention a symbol that file publishes. A writer
///   that touches none of the namer's types is not the writer of its document.
fn assert_writer_is_about_this_identity(identity: &str, writer: &str, namer: &str) {
    // Comments are stripped on both sides. `published_symbols` already ignored them and the writer
    // side did not, so the two halves of the tie spoke different languages: at one point
    // `supply_chain_conformance.rs` was tied to its namer by a single `//!` line naming
    // `verify_supply_chain`, and rewriting that sentence would have dissolved the tie without
    // touching a line of code.
    let writing = strip_comments(&writer_module_source(writer));
    if writer == namer {
        let named = writing.contains(&format!("\"{identity}\""))
            || constant_names_for(identity)
                .iter()
                .any(|name| mentions_token(&writing, name));
        assert!(
            named,
            "{INVENTORY}: {identity} names {writer} as both writer and namer, and that file does \
             not mention the identity. Either it is not the writer, or the naming column should \
             point at the file that sets the schema"
        );
        return;
    }
    let published = published_symbols(namer);
    assert!(
        !published.is_empty(),
        "{INVENTORY}: {identity} names {namer} as its naming file and that file publishes no \
         symbols, so nothing can tie the writer to it"
    );
    let touched: Vec<&String> = published
        .iter()
        .filter(|symbol| mentions_token(&writing, symbol))
        .collect();
    assert!(
        !touched.is_empty(),
        "{INVENTORY}: {identity} is recorded as written by {writer} and named in {namer}, and the \
         writer mentions none of that file's public symbols ({published:?}). A file that writes \
         some other document and happens to call a writer would look identical"
    );
}

/// A writer file plus the non-test modules it declares.
///
/// A command is its module tree, not one file. `assay registry supply-chain-conformance` writes in
/// `supply_chain_conformance.rs` and builds the carrier in its own `descriptor` module, so the
/// registry symbols that prove the tie live one file down. Reading only the named file made that
/// row green on a single `//!` line and red as soon as comments were stripped from both sides —
/// the row was never wrong, the reading was.
fn writer_module_source(writer: &str) -> String {
    let root = workspace_root();
    let path = root.join(writer);
    let mut out = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {writer}: {e}"));
    let head = strip_comments(&out);
    let test_only: BTreeSet<String> = test_mod_declarations(&head).into_iter().collect();
    for name in declared_modules(&head) {
        if test_only.contains(&name) {
            continue;
        }
        for file in module_files(&path, &name) {
            if let Ok(src) = std::fs::read_to_string(&file) {
                out.push('\n');
                out.push_str(&src);
            }
        }
    }
    out
}

/// Source with every `//` comment removed, line by line.
fn strip_comments(src: &str) -> String {
    src.lines()
        .map(code_before_comment)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Published names too generic to tie a writer to a document.
///
/// Identifier boundaries stop `SCHEMA` matching `INIT_REPORT_SCHEMA`. They do not stop `new`
/// matching `Sha256::new()`: `assay-runner-schema`'s health module publishes `pub fn new`, and the
/// observation-health writer calls `Sha256::new()`, so that row would stay tied with every
/// meaningful symbol removed.
const TOO_GENERIC: &[&str] = &[
    "new",
    "default",
    "from",
    "try_from",
    "into",
    "parse",
    "fmt",
    "next",
    "len",
    "is_empty",
    "build",
    "builder",
    "as_str",
    "to_string",
    "get",
    "insert",
    "push",
    "run",
];

/// Whether `src` uses `token` as a whole identifier rather than as a substring of a longer one.
///
/// `contains` was wrong here and a mutation caught it: `mcp/preflight.rs` declares its identity as
/// `const SCHEMA`, and `init_report.rs` contains `INIT_REPORT_SCHEMA`, so binding the preflight
/// document to the init writer passed. A substring match knows nothing about identifiers — the same
/// defect that made a `git grep` for a constant look empty earlier in this file's history.
fn mentions_token(src: &str, token: &str) -> bool {
    let mut from = 0usize;
    while let Some(rel) = src[from..].find(token) {
        let start = from + rel;
        let end = start + token.len();
        let before_ok = start == 0
            || !src.as_bytes()[start - 1].is_ascii_alphanumeric()
                && src.as_bytes()[start - 1] != b'_';
        let after_ok = end >= src.len()
            || !src.as_bytes()[end].is_ascii_alphanumeric() && src.as_bytes()[end] != b'_';
        if before_ok && after_ok {
            return true;
        }
        from = start + token.len();
    }
    false
}

/// `pub` item names declared in a file: consts, structs, enums, type aliases and functions.
fn published_symbols(path: &str) -> BTreeSet<String> {
    let src = read(path);
    let mut names = BTreeSet::new();
    for line in src.lines() {
        let line = code_before_comment(line).trim();
        // `pub(crate)` and `pub(super)` publish to the writer just as well as bare `pub`; the
        // schema-report constants are `pub(crate)` and this parser saw none of them at first.
        let Some(rest) = line.strip_prefix("pub") else {
            continue;
        };
        let rest = match rest.strip_prefix('(') {
            Some(restricted) => match restricted.split_once(')') {
                Some((_, after)) => after,
                None => continue,
            },
            None => rest,
        };
        let Some(rest) = rest.strip_prefix(' ') else {
            continue;
        };
        let rest = rest
            .strip_prefix("const ")
            .or_else(|| rest.strip_prefix("struct "))
            .or_else(|| rest.strip_prefix("enum "))
            .or_else(|| rest.strip_prefix("type "))
            .or_else(|| rest.strip_prefix("fn "));
        let Some(rest) = rest else { continue };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && !TOO_GENERIC.contains(&name.as_str()) {
            names.insert(name);
        }
    }
    names
}

/// Command files whose JSON emit is the converse of `documents_are_bound_to_a_writer`.
const COMMANDS_DIR: &str = "crates/assay-cli/src/cli/commands";

/// Idioms that mean a command file serializes JSON.
///
/// Deliberately not `WRITERS`: that list includes `println!` and filesystem helpers that 110
/// command files hit without emitting a document. This converse detector is the issue's
/// six serializers, matched as those exact substrings after test-module bodies are gone.
const JSON_SERIALIZER_IDIOMS: &[&str] = &[
    "write_stdout_json",
    "to_string_pretty",
    "to_vec_pretty",
    "to_writer",
    "serde_json::to_string(",
    "serde_json::to_vec(",
];

/// `true` for a file whose path is an external `tests.rs` module under `COMMANDS_DIR`.
///
/// Path-suffix, not basename: `attest.rs` and `livekit_tool_action.rs` must not match, and a
/// `tests.rs` sitting anywhere else in the crate is out of scope.
fn is_external_tests_rs(rel: &str) -> bool {
    rel.starts_with(COMMANDS_DIR)
        && rel.ends_with("/tests.rs")
        && rel.as_bytes().get(COMMANDS_DIR.len()) == Some(&b'/')
}

fn posix_rel(path: &std::path::Path, root: &std::path::Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_str()
        .unwrap_or_else(|| panic!("{} is not utf-8", path.display()))
        .replace('\\', "/")
}

/// One path component, admitted only if it is an ordinary name.
///
/// `read_dir` entries are untrusted input to a static analyser, and a symlink away from being
/// untrusted in fact. The first fix would canonicalise `DirEntry::path()` and then require
/// containment; CodeQL still flags that, which is fair: it is a check applied after an arbitrary
/// path exists. This admits the component instead, so a traversal cannot be spelled. `.`, `..`,
/// separators, and anything not in the allowed set are refused, and every path below is built
/// from the walk root plus admitted names. Same rule `error_enums_non_exhaustive.rs` and
/// `validate_baseline_key()` apply.
fn safe_component(name: &std::ffi::OsStr) -> Option<String> {
    let s = name.to_str()?;
    let ordinary = !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    ordinary.then(|| s.to_string())
}

/// Rust files under `root`, walked by admitting one component at a time.
///
/// Uses `DirEntry::file_type()` (does not follow) and never `DirEntry::path()`. A symlink is a
/// hard failure, not a skip: skipping would leave a serializer on the other side of the link
/// unaccounted without anyone noticing. The walk root is checked with `symlink_metadata` before
/// the first `read_dir`, because `read_dir` follows a symlink root and would inventory its target.
fn collect_rs_under(root: &Path) -> Vec<PathBuf> {
    let meta = std::fs::symlink_metadata(root)
        .unwrap_or_else(|e| panic!("unreadable command tree root {}: {e}", root.display()));
    assert!(
        !meta.file_type().is_symlink(),
        "command tree root must not be a symlink: {}",
        root.display()
    );
    assert!(
        meta.is_dir(),
        "command tree root must be a real directory: {}",
        root.display()
    );
    let mut stack = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("unreadable command directory {}: {e}", dir.display()));
        for entry in entries {
            let entry = entry.expect("dir entry");
            let file_type = entry
                .file_type()
                .unwrap_or_else(|e| panic!("unreadable file type in {}: {e}", dir.display()));
            assert!(
                !file_type.is_symlink(),
                "command tree must not contain symlinks: {} / {:?}",
                dir.display(),
                entry.file_name()
            );
            let Some(name) = safe_component(&entry.file_name()) else {
                panic!(
                    "command tree path component is not an ordinary ASCII name: {:?}",
                    entry.file_name()
                );
            };
            let p = dir.join(&name);
            assert!(
                p.starts_with(root),
                "joined path {} escaped walk root {}",
                p.display(),
                root.display()
            );
            if file_type.is_dir() {
                stack.push(p);
            } else if file_type.is_file() && name.ends_with(".rs") {
                files.push(p);
            } else if !file_type.is_file() {
                panic!("command tree entry is not a regular file or directory: {name}");
            }
        }
    }
    files.sort();
    files
}

fn command_rs_files() -> Vec<PathBuf> {
    collect_rs_under(&workspace_root().join(COMMANDS_DIR))
}

fn source_has_json_serializer(src: &str) -> bool {
    JSON_SERIALIZER_IDIOMS
        .iter()
        .any(|idiom| src.contains(idiom))
}

/// Production command files that serialize JSON, as exact workspace-relative POSIX paths.
fn json_serializing_command_files() -> BTreeSet<String> {
    let root = workspace_root();
    let test_only = test_only_files();
    let mut found = BTreeSet::new();
    for path in command_rs_files() {
        let rel = posix_rel(&path, &root);
        if is_external_tests_rs(&rel) {
            continue;
        }
        if test_only.contains(&path) {
            continue;
        }
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("unreadable production command file {rel}: {e}"));
        let src = strip_comments(&strip_test_modules_at(&src, &rel));
        if source_has_json_serializer(&src) {
            found.insert(rel);
        }
    }
    found
}

/// Exact command-tree paths named by a `cli-documents` writer/namer or an `unnamed-documents`
/// producer. Basename matching is not a fallback: a row that says `lint.rs` does not cover
/// `crates/assay-cli/src/cli/commands/evidence/lint.rs`.
fn is_command_tree_path(path: &str) -> bool {
    path.starts_with(COMMANDS_DIR) && path.as_bytes().get(COMMANDS_DIR.len()) == Some(&b'/')
}

fn command_paths_named_by_inventory_rows() -> BTreeSet<String> {
    let mut named = BTreeSet::new();
    for (writer, namer) in document_rows().into_values() {
        if is_command_tree_path(&writer) {
            named.insert(writer);
        }
        if is_command_tree_path(&namer) {
            named.insert(namer);
        }
    }
    for line in recorded("unnamed-documents") {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        assert_eq!(
            parts.len(),
            3,
            "{INVENTORY}: `unnamed-documents` row is not `key | producer | token`: {line:?}"
        );
        let producer = parts[1];
        if is_command_tree_path(producer) {
            named.insert(producer.to_string());
        }
    }
    named
}

/// Exact command-tree paths opted out of being document producers, with a non-empty motive.
///
/// The motive is a reviewable declaration of the emit or helper role, not proof. An unreadable
/// inventory or an unparsable `json-writer-opt-outs` block fails in `recorded` before this runs.
fn json_writer_opt_outs() -> BTreeMap<String, String> {
    let mut rows = BTreeMap::new();
    for line in recorded("json-writer-opt-outs") {
        let (path, motive) = line.split_once('|').unwrap_or_else(|| {
            panic!("{INVENTORY}: `json-writer-opt-outs` row is not `path | motive`: {line:?}")
        });
        let (path, motive) = (path.trim(), motive.trim());
        assert!(
            is_command_tree_path(path),
            "{INVENTORY}: opt-out path must be the exact command-tree path, not a basename: {path:?}"
        );
        assert!(
            !motive.is_empty(),
            "{INVENTORY}: opt-out {path:?} has an empty motive; name the emit or helper role"
        );
        assert!(
            rows.insert(path.to_string(), motive.to_string()).is_none(),
            "{INVENTORY}: `json-writer-opt-outs` records {path:?} twice"
        );
    }
    rows
}

/// Every production command file that serializes JSON is accounted for: a document row or an opt-out.
///
/// The older guard only followed rows to writers. A file that writes JSON without being named
/// stayed green — which is how `assay.runner.observation_health.v0` shipped unrecorded.
#[test]
fn json_serializing_command_files_are_accounted_for() {
    let writers = json_serializing_command_files();
    assert!(
        !writers.is_empty(),
        "collected no JSON-serializing command files; the scan shape moved"
    );
    let named = command_paths_named_by_inventory_rows();
    let opt_outs = json_writer_opt_outs();
    let opted: BTreeSet<String> = opt_outs.keys().cloned().collect();

    let also_named: Vec<_> = opted.intersection(&named).cloned().collect();
    assert!(
        also_named.is_empty(),
        "{INVENTORY}: these opt-outs are already named by a `cli-documents` writer/namer or \
         `unnamed-documents` producer, so the opt-out is stale:\n  {}",
        also_named.join("\n  ")
    );

    let missing: Vec<_> = opted
        .iter()
        .filter(|path| !workspace_root().join(path).is_file())
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "{INVENTORY}: these opt-out paths do not exist:\n  {}",
        missing.join("\n  ")
    );

    let no_longer_serialize: Vec<_> = opted.difference(&writers).cloned().collect();
    assert!(
        no_longer_serialize.is_empty(),
        "{INVENTORY}: these opt-outs no longer serialize JSON through the converse idioms:\n  {}",
        no_longer_serialize.join("\n  ")
    );

    let accounted: BTreeSet<String> = named.union(&opted).cloned().collect();
    let unaccounted: Vec<String> = writers.difference(&accounted).cloned().collect();
    assert!(
        unaccounted.is_empty(),
        "these production command files serialize JSON and are named by no `cli-documents` \
         writer/namer, no `unnamed-documents` producer, and no `json-writer-opt-outs` row:\n  {}",
        unaccounted.join("\n  ")
    );
}

/// `*/tests.rs` under the command tree is excluded by a path rule, not because those files
/// happen to lack serializers. Several of them serialize today; leaking them into the candidate
/// set is a failure of the rule, not of the inventory.
#[test]
fn external_tests_rs_serializers_are_not_candidates() {
    let root = workspace_root();
    let mut tests_rs_with_idioms = Vec::new();
    for path in command_rs_files() {
        let rel = posix_rel(&path, &root);
        if !is_external_tests_rs(&rel) {
            continue;
        }
        let src =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("unreadable {rel}: {e}"));
        if source_has_json_serializer(&src) {
            tests_rs_with_idioms.push(rel);
        }
    }
    assert!(
        !tests_rs_with_idioms.is_empty(),
        "no `*/tests.rs` under {COMMANDS_DIR} currently contains a serializer idiom; the \
         exclusion would be an omission rather than a tested rule"
    );
    let candidates = json_serializing_command_files();
    let leaked: Vec<_> = tests_rs_with_idioms
        .into_iter()
        .filter(|rel| candidates.contains(rel))
        .collect();
    assert!(
        leaked.is_empty(),
        "external `*/tests.rs` files leaked into writer candidates: {leaked:?}"
    );
}

/// Inline `#[cfg(test)] mod` bodies are stripped before the idiom scan. Files whose only
/// serializer sits in that body are not candidates; if the stripper stopped running they would
/// appear as unaccounted production writers.
#[test]
fn inline_cfg_test_module_serializers_are_not_candidates() {
    let root = workspace_root();
    let test_only = test_only_files();
    let mut inline_only = Vec::new();
    for path in command_rs_files() {
        let rel = posix_rel(&path, &root);
        if is_external_tests_rs(&rel) || test_only.contains(&path) {
            continue;
        }
        let src =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("unreadable {rel}: {e}"));
        let with_tests = strip_comments(&src);
        let without_tests = strip_comments(&strip_test_modules_at(&src, &rel));
        if source_has_json_serializer(&with_tests) && !source_has_json_serializer(&without_tests) {
            inline_only.push(rel);
        }
    }
    assert!(
        !inline_only.is_empty(),
        "no production command file currently keeps its serializer only inside an inline \
         `#[cfg(test)] mod`; the exclusion would be an omission rather than a tested rule"
    );
    let candidates = json_serializing_command_files();
    let leaked: Vec<_> = inline_only
        .into_iter()
        .filter(|rel| candidates.contains(rel))
        .collect();
    assert!(
        leaked.is_empty(),
        "inline `#[cfg(test)]` serializers leaked into writer candidates: {leaked:?}"
    );
}

#[test]
fn external_tests_rs_rule_is_a_commands_path_suffix() {
    assert!(is_external_tests_rs(
        "crates/assay-cli/src/cli/commands/replay/tests.rs"
    ));
    assert!(!is_external_tests_rs(
        "crates/assay-cli/src/cli/commands/evidence/attest.rs"
    ));
    assert!(!is_external_tests_rs(
        "crates/assay-core/src/report/tests.rs"
    ));
    assert!(!is_external_tests_rs(
        "crates/assay-cli/src/cli/commands/tests_helpers.rs"
    ));
}

#[test]
fn strip_test_modules_removes_pub_crate_mod() {
    let src = "fn prod() {}\n#[cfg(test)]\npub(crate) mod tests {\n    let _ = serde_json::to_string(&1);\n}\n";
    let stripped = strip_test_modules_at(src, "fixture");
    assert!(
        !stripped.contains("serde_json::to_string("),
        "visibility-prefixed test modules must strip like bare `mod tests`"
    );
    assert!(stripped.contains("fn prod"));
}

/// The component allowlist, which is the load-bearing half of the command-tree walk.
#[test]
fn command_tree_component_rule_refuses_anything_that_could_leave_the_directory() {
    use std::ffi::OsStr;
    for ok in ["mod.rs", "lint.rs", "skill_supply_chain.rs", "a.b-c_1"] {
        assert_eq!(
            safe_component(OsStr::new(ok)).as_deref(),
            Some(ok),
            "{ok} is an ordinary name"
        );
    }
    for bad in ["", ".", "..", "../etc", "a/b", "a\\b", "a b", "naïve"] {
        assert_eq!(
            safe_component(OsStr::new(bad)),
            None,
            "{bad:?} must not be admitted as a component"
        );
    }
}

/// A newly added `.rs` file under the walk root is collected. The converse detector's
/// unaccounted assertion is this property on the live tree: an unreferenced serializer file
/// must appear, not be skipped because the walker only knows inventory paths.
#[test]
fn command_tree_walk_collects_a_new_unreferenced_rs_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir.path().join("evidence");
    std::fs::create_dir(&nested).expect("nested dir");
    std::fs::write(dir.path().join("mod.rs"), "fn g() {}\n").expect("mod.rs");
    std::fs::write(
        nested.join("unreferenced.rs"),
        "fn f() { let _ = serde_json::to_string(&1); }\n",
    )
    .expect("unreferenced.rs");
    let files = collect_rs_under(dir.path());
    let names: Vec<String> = files
        .iter()
        .map(|p| {
            p.strip_prefix(dir.path())
                .expect("under root")
                .to_str()
                .expect("utf-8")
                .replace('\\', "/")
        })
        .collect();
    assert!(
        names.contains(&"mod.rs".to_string()),
        "existing file must remain visible: {names:?}"
    );
    assert!(
        names.contains(&"evidence/unreferenced.rs".to_string()),
        "a newly added command file must be collected so the converse detector can fail it: {names:?}"
    );
    let with_idiom: Vec<_> = files
        .iter()
        .filter(|p| source_has_json_serializer(&std::fs::read_to_string(p).expect("read fixture")))
        .collect();
    assert_eq!(
        with_idiom.len(),
        1,
        "the new file is the serializer the inventory has not named"
    );
}

/// `DirEntry::path()` then `is_dir()` follows a symlink. The walk must refuse the link
/// before a path is constructed, so a link cannot take the scan outside the root.
#[cfg(unix)]
#[test]
fn command_tree_walk_rejects_symlinks_fail_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(dir.path().join("ok.rs"), "fn x() {}\n").expect("ok.rs");
    std::os::unix::fs::symlink(outside.path(), dir.path().join("escape")).expect("symlink");
    let result = std::panic::catch_unwind(|| collect_rs_under(dir.path()));
    assert!(
        result.is_err(),
        "a symlink in the command tree must fail closed, not be followed or skipped"
    );
}

/// `read_dir` follows a symlink root. The walk must refuse before the first listing, so a
/// replaced `cli/commands` link cannot inventory a tree outside that directory.
#[cfg(unix)]
#[test]
fn command_tree_walk_rejects_symlink_root_fail_closed() {
    let holder = tempfile::tempdir().expect("holder");
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("escaped.rs"), "fn x() {}\n").expect("escaped.rs");
    let commands = holder.path().join("commands");
    std::os::unix::fs::symlink(outside.path(), &commands).expect("root symlink");
    let result = std::panic::catch_unwind(|| collect_rs_under(&commands));
    assert!(
        result.is_err(),
        "a symlink supplied as the walk root must fail closed, not be followed"
    );
}
