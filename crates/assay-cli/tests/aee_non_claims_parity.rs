//! The producer's standing non-claims and the checker's floor must be the same list.
//!
//! `payload_non_claims()` in `aee_seal.rs` builds what every seal carries. `MINIMUM_NON_CLAIMS` in
//! `scripts/experiments/aee_landlock_seal_fixture.py` is what a seal must carry to be credited. One
//! contract, two hand-kept copies, no link — the same shape as the `AEE_VERSION` duplication that
//! `aee_version_parity.rs` exists for.
//!
//! Found by biting rather than by reasoning. Removing an entry from the checker's floor left every
//! gate in that file green: the positive fixture derives from the floor, so it lost the entry too,
//! and a producer emitting five non-claims against a floor of four is still credited. So the
//! failure is silent in the direction that matters — the checker stops requiring something the
//! producer still sends, and the day the producer drops it too, nothing objects.
//!
//! Direction matters here and the assertion is equality rather than subset. A floor *below* the
//! producer means a seal that quietly dropped a non-claim would still be credited. A floor *above*
//! it means every real seal is rejected, which is loud and self-correcting. Only the first is
//! dangerous, but equality is the honest contract: the checker's floor is not a policy of its own,
//! it is a restatement of what the producer promises.

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

/// The quoted strings inside a named bracketed block, in source order.
///
/// One extractor for both languages: the Rust array and the Python tuple differ only in the
/// bracket, which the caller names. A second extractor would be a second thing to get wrong, in a
/// test whose whole subject is duplication.
fn quoted_items(src: &str, start_marker: &str, close: char) -> Vec<String> {
    let from = src
        .find(start_marker)
        .unwrap_or_else(|| panic!("{start_marker:?} not found"));
    let body = &src[from + start_marker.len()..];

    // Comments come out first, before the closing bracket is looked for. The real
    // `MINIMUM_NON_CLAIMS` carries a citation ending in `#570)`, and searching the raw text for the
    // first `)` stopped the scan inside that comment and silently dropped the entry after it. The
    // fixture case below had no bracket in its comment, so the extractor test passed while the
    // extractor was wrong -- found by running it against the real file.
    let stripped: String = body
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n");

    let end = stripped
        .find(close)
        .unwrap_or_else(|| panic!("unterminated block after {start_marker:?}"));
    let body = &stripped[..end];

    let mut out = Vec::new();
    let mut rest = body;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let Some(close_q) = after.find('"') else {
            break;
        };
        out.push(after[..close_q].to_string());
        rest = &after[close_q + 1..];
    }
    out
}

#[test]
fn the_producer_and_the_checker_agree_on_the_standing_non_claims() {
    let producer = quoted_items(
        &read("crates/assay-cli/src/aee_seal.rs"),
        "fn payload_non_claims() -> Vec<String> {\n    [",
        ']',
    );
    let checker = quoted_items(
        &read("scripts/experiments/aee_landlock_seal_fixture.py"),
        "MINIMUM_NON_CLAIMS = (",
        ')',
    );

    // Non-empty, so a parser that silently matched nothing cannot make this vacuous.
    assert!(
        producer.len() >= 4,
        "extracted {} producer non-claim(s); the extractor stopped matching",
        producer.len()
    );
    assert_eq!(
        producer, checker,
        "the seal producer emits these standing non-claims and the fixture checker requires those. \
         They are one contract: a floor below the producer credits a seal that dropped a non-claim, \
         which is the failure `MINIMUM_NON_CLAIMS` exists to prevent."
    );
}

/// The extractor, against fixture text.
///
/// Editing either real list is not a usable mutation for the test above: it fails the assertion
/// under test, which cannot distinguish "the extractor works" from "the extractor returns something
/// that happens to match on both sides".
#[test]
fn the_extractor_reads_items_and_skips_prose() {
    let rustish = "fn f() -> Vec<String> {\n    [\n        \"one\",\n        // a \"quoted\" aside\n        \"two\",\n    ]\n";
    assert_eq!(
        quoted_items(rustish, "fn f() -> Vec<String> {\n    [", ']'),
        vec!["one", "two"]
    );

    let pyish = "X = (\n    \"alpha\",\n    # a \"quoted\" comment\n    \"beta\",\n)\n";
    assert_eq!(quoted_items(pyish, "X = (", ')'), vec!["alpha", "beta"]);

    // The case the real file has and the one above did not: a closing bracket inside a comment,
    // which truncated the scan and dropped every entry after it.
    let with_bracket =
        "X = (\n    \"alpha\",\n    # a citation (see #570) mid-list\n    \"beta\",\n)\n";
    assert_eq!(
        quoted_items(with_bracket, "X = (", ')'),
        vec!["alpha", "beta"],
        "a bracket inside a comment must not end the block"
    );
}

/// A missing block panics rather than returning nothing.
#[test]
fn a_missing_block_fails_loudly() {
    let got = std::panic::catch_unwind(|| quoted_items("nothing here", "MISSING = (", ')'));
    assert!(
        got.is_err(),
        "a renamed list must fail rather than compare empty vectors"
    );
}
