//! Every public error enum carries `#[non_exhaustive]`, so a new failure mode is not a major.
//!
//! #2140 measured the whole public surface: 697 types, 9 marked, 609 that could not grow without a
//! major version paid by every published crate. The decision recorded there is deliberately narrow.
//! Error enums get the attribute and almost nothing else does, for two reasons that pull the same
//! way.
//!
//! The first is that a new failure mode is the most ordinary change a library makes, and every
//! source agrees this is the canonical case. The second is measured rather than argued: marking all
//! 27 public error enums produced **zero** compile errors across the workspace, because error types
//! are consumed through `?` and `Display` rather than matched exhaustively. Marking every at-risk
//! enum instead produced 19, and the twelve `E0004`s named `ClaimSupport` and
//! `NetworkProtocolCoverageStatus` -- the claim and coverage vocabularies, where an exhaustive match
//! is the enforcement and a wildcard arm would remove the thing the parity tests exist to hold.
//!
//! So this test guards the boundary the decision drew. It is not a general "mark everything" rule
//! and must not become one.
//!
//! **Why the rule is `pub enum *Error` and not "public error enums":** deciding what is truly public
//! needs module-visibility resolution through `pub mod` chains and `pub use` re-exports, and #2140
//! got that wrong twice before rustdoc JSON settled it -- a `pub` type in a private module is still
//! public API when re-exported, and a grep for the attribute counted fifteen doc-comment mentions as
//! code. A check that needs a fragile analysis is a check that reports clean when it breaks. The
//! name-shaped rule over-includes instead: an error enum in a private module gets an attribute that
//! is a no-op there, which costs nothing and cannot silently under-report.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// One path component, admitted only if it is an ordinary name.
///
/// Paths here start at a `read_dir` entry, which is untrusted input to a static analyser and is a
/// symlink away from being untrusted in fact: a link under `crates/` pointing outside the workspace
/// would have this test read, and report on, a file that is not ours.
///
/// The first fix canonicalised and then required containment under the root. That is a correct
/// check and CodeQL still flagged the read, which is fair: it is a check applied *after* an
/// arbitrary path exists. This admits the component instead, so a traversal cannot be spelled in
/// the first place. `.`, `..`, separators, and anything not in the allowed set are refused, and
/// every path below is then built from `root` plus fixed segments plus admitted names.
///
/// Same rule the bundle reader and `validate_baseline_key()` apply to archive members. Being a
/// test is not a reason for an exemption.
fn safe_component(name: &std::ffi::OsStr) -> Option<String> {
    let s = name.to_str()?;
    let ordinary = !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    ordinary.then(|| s.to_string())
}

/// True when `p` resolves to something inside `root`, so a symlink cannot lead the walk out.
///
/// Kept alongside the component rule rather than instead of it: the allowlist stops a traversal
/// being written, and this stops one being followed.
fn resolves_inside(root: &Path, p: &Path) -> bool {
    p.canonicalize().is_ok_and(|r| r.starts_with(root))
}

/// Crates that can break a downstream consumer: published, and carrying a library target.
///
/// `publish = false` crates have no downstream. `assay-cli` is published but is a binary with no
/// `lib.rs`, so it has no public API surface at all, which is why its own error enums are outside
/// this rule rather than exceptions to it.
fn crates_with_public_api(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let dir = root.join("crates");
    for entry in fs::read_dir(&dir).expect("crates/ is readable") {
        let Some(name) = safe_component(&entry.expect("dir entry").file_name()) else {
            continue;
        };
        // Rebuilt from the root and an admitted name, so this path cannot express a traversal.
        let path = dir.join(&name);
        let manifest = path.join("Cargo.toml");
        if !resolves_inside(root, &manifest) || !resolves_inside(root, &path.join("src/lib.rs")) {
            continue;
        }
        let text = fs::read_to_string(&manifest).expect("manifest is readable");
        if text
            .lines()
            .any(|l| l.trim_start().starts_with("publish") && l.contains("false"))
        {
            continue;
        }
        out.push(path);
    }
    out.sort();
    assert!(
        out.len() >= 10,
        "found only {} crates with a public API; the walk is broken, not the workspace",
        out.len()
    );
    out
}

fn rust_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(name) = safe_component(&entry.file_name()) else {
            continue;
        };
        let p = dir.join(&name);
        if !resolves_inside(root, &p) {
            continue;
        }
        if p.is_dir() {
            rust_files(root, &p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// `pub enum <Name>Error` declarations in `text` that lack `#[non_exhaustive]`, by line number.
///
/// The attribute block is read by walking *up* from the declaration over contiguous attribute and
/// doc-comment lines, stopping at the first line that is neither. A fixed-distance lookback was
/// tried first and produced a false positive by catching the attribute of the item above, which is
/// why this walks lines rather than characters.
fn unmarked_error_enums(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("pub enum ") else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.ends_with("Error") {
            continue;
        }
        let mut marked = false;
        let mut j = i;
        while j > 0 {
            j -= 1;
            let a = lines[j].trim_start();
            if a.starts_with("#[") {
                if a.contains("non_exhaustive") {
                    marked = true;
                    break;
                }
            } else if !(a.starts_with("///") || a.starts_with("//!") || a.starts_with("//")) {
                break;
            }
        }
        if !marked {
            out.push((i + 1, name));
        }
    }
    out
}

#[test]
fn every_public_error_enum_is_non_exhaustive() {
    let root = workspace_root();
    let mut offenders = Vec::new();
    let mut checked = 0usize;

    for krate in crates_with_public_api(&root) {
        let mut files = Vec::new();
        rust_files(&root, &krate.join("src"), &mut files);
        for f in files {
            let text = fs::read_to_string(&f).expect("source is readable");
            checked += text
                .lines()
                .filter(|l| {
                    l.trim_start().strip_prefix("pub enum ").is_some_and(|r| {
                        r.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                            .next()
                            .is_some_and(|n| n.ends_with("Error"))
                    })
                })
                .count();
            for (line, name) in unmarked_error_enums(&text) {
                offenders.push(format!(
                    "{}:{line} {name}",
                    f.strip_prefix(&root).unwrap_or(&f).display()
                ));
            }
        }
    }

    // Non-vacuous: if the walk stops finding declarations, this test would pass by finding nothing.
    assert!(
        checked >= 20,
        "only {checked} error enum declaration(s) seen; the walk is broken, so a pass here means nothing"
    );
    assert!(
        offenders.is_empty(),
        "these public error enums can only grow a variant at a major version. Add \
         `#[non_exhaustive]` at the declaration, per the decision in #2140:\n  {}",
        offenders.join("\n  ")
    );
}

/// The component allowlist, which is now the load-bearing half of the path handling.
#[test]
fn the_component_rule_refuses_anything_that_could_leave_the_directory() {
    use std::ffi::OsStr;
    for ok in ["assay-core", "assay_evidence", "mod.rs", "a.b-c_1"] {
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

/// The detector, against fixture text.
///
/// Editing a real enum is not a usable mutation for the test above: it fails the assertion under
/// test, which cannot tell "the detector works" from "the detector found nothing".
#[test]
fn the_detector_reads_the_attribute_block_and_not_the_neighbourhood() {
    assert_eq!(
        unmarked_error_enums("#[non_exhaustive]\npub enum FooError {\n}\n").len(),
        0,
        "an attribute directly above must count"
    );
    assert_eq!(
        unmarked_error_enums(
            "#[non_exhaustive]\n#[derive(Debug)]\n/// doc\npub enum FooError {\n}\n"
        )
        .len(),
        0,
        "the attribute must still count through derives and docs"
    );
    assert_eq!(
        unmarked_error_enums("pub enum FooError {\n}\n").len(),
        1,
        "a bare declaration must be reported"
    );

    // The false positive that a fixed-distance lookback produced: the attribute belongs to the item
    // above, and a character-window search attributes it to the enum below.
    let neighbour = "#[non_exhaustive]\npub enum Other {\n    A,\n}\n\npub enum FooError {\n}\n";
    assert_eq!(
        unmarked_error_enums(neighbour),
        vec![(6, "FooError".to_string())],
        "an attribute on a preceding item must not be read as this one's"
    );

    // Names that merely contain `Error` are out of scope; the rule is on the suffix.
    assert_eq!(
        unmarked_error_enums("pub enum ErrorKind {\n}\n").len(),
        0,
        "the rule keys on the Error suffix, not on the substring"
    );
}
