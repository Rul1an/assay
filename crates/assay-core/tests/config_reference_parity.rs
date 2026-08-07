//! The config reference and the enums it describes must name the same types.
//!
//! Both directions, and they are not one property. "Every variant has a section" is the assertion
//! one naturally writes, and it passed on `sequences.md` for as long as that file existed while
//! two sections described rule types the parser rejects — `immediately_before` and `allowlist`,
//! neither ever present in the model, both introduced with `1822a7de4`. A phantom section is not
//! a missing one, so only the converse catches it.
//!
//! The two failures also differ in cost. An undocumented variant is a discoverability gap: the
//! code still tells the truth. A documented phantom is a falsehood in the one place a reader
//! consults to resolve the confusion it causes.
//!
//! Ground truth is the serde rename, not a hand-kept list here, so a new variant is covered the
//! moment it is added. `schemars` is already a dependency and deriving `JsonSchema` on these enums
//! would let the check read a generated schema instead of parsing headings; that is a larger
//! change and this file does not need it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn read(rel: &str) -> String {
    let p = workspace_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// The `type:` names the enum accepts.
///
/// Two spellings, because the two enums use different ones: `TraceAssertion` writes an explicit
/// `#[serde(rename = "...")]` on each variant, `SequenceRule` relies on the container's
/// `rename_all = "snake_case"`. Both are read out of the source rather than from a list here,
/// because a list here would be a second place to forget, which is the defect under test.
///
/// An empty result is treated as a failure by the callers. A parser that silently matches nothing
/// makes every parity assertion below vacuously true, which is the shape this whole file exists
/// to refuse.
fn serde_renames(source_rel: &str, enum_name: &str) -> BTreeSet<String> {
    let src = read(source_rel);
    let start = src
        .find(&format!("pub enum {enum_name}"))
        .unwrap_or_else(|| panic!("{enum_name} not found in {source_rel}"));
    // The enum body ends at the first line that closes it at column 0.
    let body = &src[start..];
    let end = body.find("\n}").expect("enum body is unterminated");
    let body = &body[..end];

    let explicit: BTreeSet<String> = body
        .match_indices("#[serde(rename = \"")
        .map(|(i, m)| {
            let rest = &body[i + m.len()..];
            rest[..rest.find('"').expect("unterminated serde rename")].to_string()
        })
        .collect();
    if !explicit.is_empty() {
        return explicit;
    }

    // No per-variant renames: derive from the variant identifiers under `rename_all`.
    body.lines()
        .filter_map(|l| {
            let t = l.trim();
            let ident: String = t
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            let looks_like_variant = !ident.is_empty()
                && ident.starts_with(|c: char| c.is_ascii_uppercase())
                && (t == ident
                    || t.starts_with(&format!("{ident} {{"))
                    || t == format!("{ident},"));
            looks_like_variant.then(|| snake_case(&ident))
        })
        .collect()
}

fn snake_case(ident: &str) -> String {
    let mut out = String::new();
    for (i, c) in ident.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The types a reference documents, taken from its headings at the given level.
fn documented_types(doc_rel: &str, heading: &str) -> BTreeSet<String> {
    read(doc_rel)
        .lines()
        .filter_map(|l| l.strip_prefix(heading))
        .filter_map(|rest| {
            let rest = rest.trim_start();
            let inner = rest.strip_prefix('`')?;
            let name = &inner[..inner.find('`')?];
            // Headings are `name` or `name` — Human Title; both carry the type in backticks.
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c == '_')
                .then(|| name.to_string())
        })
        .collect()
}

fn assert_parity(
    what: &str,
    doc_rel: &str,
    documented: &BTreeSet<String>,
    real: &BTreeSet<String>,
) {
    let phantom: Vec<_> = documented.difference(real).cloned().collect();
    assert!(
        phantom.is_empty(),
        "{doc_rel} documents {what} the parser rejects: {phantom:?}\n\
         A reader who follows the reference writes one of these and gets a parse error, with the \
         reference being the thing they would consult to find out why. Remove the section, or add \
         the variant."
    );

    let undocumented: Vec<_> = real.difference(documented).cloned().collect();
    assert!(
        undocumented.is_empty(),
        "{doc_rel} has no section for {what} a user can write: {undocumented:?}\n\
         Each of these carries a serde rename on a tagged enum, so it parses today."
    );
}

#[test]
fn sequence_rule_types_and_their_reference_agree() {
    let real = serde_renames("crates/assay-core/src/model/types.rs", "SequenceRule");
    assert!(
        real.len() >= 8,
        "expected the rename set to be populated, got {real:?}"
    );
    let documented = documented_types("docs/reference/config/sequences.md", "### ");
    assert_parity(
        "sequence rule types",
        "docs/reference/config/sequences.md",
        &documented,
        &real,
    );
}

#[test]
fn trace_assertion_types_and_their_reference_agree() {
    let real = serde_renames(
        "crates/assay-core/src/agent_assertions/model.rs",
        "TraceAssertion",
    );
    assert!(
        real.len() >= 7,
        "expected the rename set to be populated, got {real:?}"
    );
    let documented = documented_types("docs/reference/config/eval-yaml.md", "#### ");
    assert_parity(
        "assertion types",
        "docs/reference/config/eval-yaml.md",
        &documented,
        &real,
    );
}
