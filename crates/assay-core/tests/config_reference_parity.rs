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
    serde_renames_in(&read(source_rel), enum_name)
}

/// The parsing half, separated so it can be tested against fixture text.
///
/// Adding a variant to a real enum is not a usable mutation: it breaks every exhaustive match in
/// the crate, so nothing compiles and nothing runs. The shapes below are therefore checked here,
/// where a variant can exist without the rest of the world having to handle it.
fn serde_renames_in(src: &str, enum_name: &str) -> BTreeSet<String> {
    let start = src
        .find(&format!("pub enum {enum_name}"))
        .unwrap_or_else(|| panic!("{enum_name} not found in the source read for it"));
    // The enum body ends at the first line that closes it at column 0.
    let body = &src[start..];
    let end = body.find("\n}").expect("enum body is unterminated");
    let body = &body[..end];

    // The union of both spellings, never one or the other. An earlier version returned the
    // explicit set whenever it was non-empty, which made a new variant invisible on exactly the
    // enum most likely to grow one: `TraceAssertion` carries `rename_all` *and* redundant
    // per-variant renames, so a contributor who adds a variant and omits the redundant attribute
    // drops out of the check silently. Confirmed by mutation, not reasoning.
    let explicit: BTreeSet<String> = body
        .match_indices("#[serde(rename = \"")
        .map(|(i, m)| {
            let rest = &body[i + m.len()..];
            rest[..rest.find('"').expect("unterminated serde rename")].to_string()
        })
        .collect();

    let derived: BTreeSet<String> = body
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            let ident: String = t
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if ident.is_empty() || !ident.starts_with(|c: char| c.is_ascii_uppercase()) {
                return None;
            }
            // Struct (`Foo {`), unit (`Foo` / `Foo,`), tuple (`Foo(`) and discriminant (`Foo =`).
            // Tuple variants were missed by an earlier version: a `Teleport(String)` added to
            // `SequenceRule` survived the entire suite.
            let rest = t[ident.len()..].trim_start();
            let looks_like_variant = rest.is_empty()
                || rest.starts_with('{')
                || rest.starts_with('(')
                || rest.starts_with(',')
                || rest.starts_with('=');
            looks_like_variant.then(|| snake_case(&ident))
        })
        .collect();

    explicit.union(&derived).cloned().collect()
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
    documented_types_in(&read(doc_rel), heading)
}

/// The parsing half, separated so it can be tested against fixture text.
fn documented_types_in(md: &str, heading: &str) -> BTreeSet<String> {
    md.lines()
        .filter_map(|l| l.strip_prefix(heading))
        .filter_map(|rest| {
            let rest = rest.trim_start();
            let inner = rest.strip_prefix('`')?;
            let name = &inner[..inner.find('`')?];
            // Headings are `name` or `name` — Human Title; both carry the type in backticks.
            // Digits allowed. Restricting to `[a-z_]` silently skipped any heading whose type
            // carried one, making the phantom direction vacuous for it.
            (!name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
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
    // Both halves are computed before either is asserted, so a document broken in both
    // directions still reports the undocumented one after the phantom assertion is fixed.
    let phantom: Vec<_> = documented.difference(real).cloned().collect();
    let undocumented: Vec<_> = real.difference(documented).cloned().collect();
    assert!(
        phantom.is_empty(),
        "{doc_rel} documents {what} the parser rejects: {phantom:?}\n\
         A reader who follows the reference writes one of these and gets a parse error, with the \
         reference being the thing they would consult to find out why. Remove the section, or add \
         the variant."
    );

    assert!(
        undocumented.is_empty(),
        "{doc_rel} has no section for {what} a user can write: {undocumented:?}\n\
         Each of these carries a serde rename on a tagged enum, so it parses today."
    );
}

#[test]
fn sequence_rule_types_and_their_reference_agree() {
    let real = serde_renames("crates/assay-core/src/model/types.rs", "SequenceRule");
    // Non-empty, not a pinned count. A pinned count fails on a legitimate variant removal with a
    // message blaming the extractor, and aborts before reporting the stale section that removal
    // actually leaves behind.
    assert!(
        !real.is_empty(),
        "extracted no variant names from SequenceRule, so every assertion below would be vacuous"
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
        !real.is_empty(),
        "extracted no variant names from TraceAssertion, so every assertion below would be vacuous"
    );
    let documented = documented_types("docs/reference/config/eval-yaml.md", "#### ");
    assert_parity(
        "assertion types",
        "docs/reference/config/eval-yaml.md",
        &documented,
        &real,
    );
}

#[cfg(test)]
mod extractor {
    use super::serde_renames_in;

    /// A variant with no explicit rename, on an enum that has them elsewhere. This is the shape
    /// that survived before: the extractor returned the explicit set whenever it was non-empty,
    /// and `TraceAssertion` carries `rename_all` plus redundant per-variant renames, so the next
    /// variant someone adds without the redundant attribute would have gone unchecked.
    #[test]
    fn a_variant_without_an_explicit_rename_is_still_found() {
        let src = r#"
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Thing {
    #[serde(rename = "already_named")]
    AlreadyNamed { a: String },
    NewCheck { thing: String },
}
"#;
        let got = serde_renames_in(src, "Thing");
        assert!(got.contains("already_named"), "got {got:?}");
        assert!(
            got.contains("new_check"),
            "the unrenamed variant was dropped: {got:?}"
        );
    }

    /// Tuple, unit and discriminant variants, none of which the first version recognised.
    #[test]
    fn every_variant_shape_is_found() {
        let src = r#"
#[serde(rename_all = "snake_case")]
pub enum Thing {
    Structy { a: String },
    Tuply(String),
    Unity,
    Trailing,
    Discriminated = 1,
}
"#;
        let got = serde_renames_in(src, "Thing");
        for want in ["structy", "tuply", "unity", "trailing", "discriminated"] {
            assert!(got.contains(want), "{want} missing from {got:?}");
        }
    }

    /// A heading whose type carries a digit. Filtering names to `[a-z_]` skipped these, which
    /// made the phantom direction vacuous for any such name -- a `teleport2` section sat in the
    /// document and no assertion saw it.
    #[test]
    fn a_documented_type_with_a_digit_is_read() {
        let md = "### `sequence_v2` — Something\n### `plain` — Other\n";
        let got = super::documented_types_in(md, "### ");
        assert!(
            got.contains("sequence_v2"),
            "digit-bearing name dropped: {got:?}"
        );
        assert!(got.contains("plain"), "got {got:?}");
    }

    /// Headings that are prose, not types, must not be read as types.
    #[test]
    fn prose_headings_are_not_types() {
        let md = "### 1. Start Simple\n### E-commerce: Payment Flow\n### `real_one`\n";
        let got = super::documented_types_in(md, "### ");
        assert_eq!(got.len(), 1, "got {got:?}");
        assert!(got.contains("real_one"));
    }

    /// Doc comments, attributes and struct fields are not variants.
    #[test]
    fn non_variants_are_not_mistaken_for_variants() {
        let src = r#"
pub enum Thing {
    /// Doc comment mentioning Something.
    #[serde(default)]
    Real { field_name: String, Another: u8 },
}
"#;
        let got = serde_renames_in(src, "Thing");
        assert!(got.contains("real"), "got {got:?}");
        assert!(
            !got.contains("doc"),
            "doc comment read as a variant: {got:?}"
        );
    }
}
