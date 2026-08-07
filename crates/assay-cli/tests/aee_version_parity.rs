//! The producer and the fixture checker must name the same AEE draft version.
//!
//! `AEE_VERSION` exists twice: once in `crates/assay-cli/src/aee_seal.rs` and once in
//! `scripts/experiments/aee_landlock_seal_fixture.py`. One version, two literals, no link — so a
//! bump that reaches only one side leaves a producer emitting a version its own checker rejects,
//! and the failure surfaces as an unrelated-looking `payload-aee-version-unsupported` on a payload
//! that is otherwise correct.
//!
//! `CLAUDE.md` prefers one rule with one implementation and allows a parity test where that is not
//! possible. It is not possible here: the checker is a standalone Python script by design, so it
//! cannot read a Rust constant, and duplicating the version is the price of that separation. This
//! test is the sanctioned fallback rather than a substitute for a fix.
//!
//! Both sides are read from source text, following `adr045_prefix_table.rs` — `assay-cli` is a
//! binary crate with no lib target, so a test cannot import the constant.
//!
//! #2093 decided the value stays `0.7` while in-toto/attestation#570 is open at v0.7. This test
//! does not encode that decision: a deliberate bump should edit both sides and stay green. It
//! encodes only that the two sides agree.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn read(rel: &str) -> String {
    let path = workspace_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The value assigned to `AEE_VERSION`, from either language.
///
/// One extractor for both because the two declarations differ only in punctuation this already
/// discards: `pub const AEE_VERSION: &str = "0.7";` and `AEE_VERSION = "0.7"`. A second extractor
/// would be a second thing to get wrong, on a test whose subject is duplication.
fn declared_version(src: &str) -> String {
    let line = src
        .lines()
        .find(|l| {
            let t = l.trim_start();
            t.starts_with("AEE_VERSION") || t.starts_with("pub const AEE_VERSION")
        })
        .expect("AEE_VERSION is declared in this file");
    let after = line
        .split_once('=')
        .expect("AEE_VERSION has a value")
        .1
        .trim();
    // Take what is inside the first pair of quotes, so a trailing comment or `;` cannot enter.
    let quote = after
        .chars()
        .next()
        .filter(|c| *c == '"' || *c == '\'')
        .expect("the value is quoted");
    let rest = &after[1..];
    rest[..rest.find(quote).expect("the value's quote is closed")].to_string()
}

#[test]
fn the_producer_and_the_fixture_checker_agree_on_the_aee_version() {
    let producer = declared_version(&read("crates/assay-cli/src/aee_seal.rs"));
    let checker = declared_version(&read("scripts/experiments/aee_landlock_seal_fixture.py"));

    // Non-empty, so a parser that silently matches nothing cannot make the comparison vacuous.
    assert!(
        !producer.is_empty() && !checker.is_empty(),
        "extracted producer={producer:?} checker={checker:?}; an empty side makes this test vacuous"
    );
    assert_eq!(
        producer, checker,
        "the producer emits aeeVersion {producer:?} and the fixture checker accepts {checker:?}. \
         A bump has to reach both sides; see the decision recorded on AEE_VERSION in aee_seal.rs."
    );
}

/// The extractor, against fixture text.
///
/// Editing either real constant is not a usable mutation for the test above: it would fail the
/// assertion under test, which cannot distinguish "the extractor works" from "the extractor
/// returns something that happens to match on both sides".
#[test]
fn the_version_extractor_reads_the_value_and_not_the_line() {
    assert_eq!(declared_version("AEE_VERSION = \"0.9\"\n"), "0.9");
    assert_eq!(declared_version("  AEE_VERSION = '1.2'  \n"), "1.2");
    assert_eq!(
        declared_version("pub const AEE_VERSION: &str = \"0.7\";\n"),
        "0.7",
        "the Rust form's trailing semicolon must not be read as part of the version"
    );
    assert_eq!(
        declared_version("OTHER = \"x\"\nAEE_VERSION = \"0.7\"  # why\n"),
        "0.7",
        "a trailing comment must not be read as part of the version"
    );
}
