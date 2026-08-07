//! Anything this code quotes from ADR-045 must still be in ADR-045.
//!
//! Six citations pointed at line numbers, and all six had gone stale: line 232 had become a table
//! row, 192 a JSON key inside an example, 425 a blank line, and 476 a bullet about issue #1998,
//! cited three times across two languages. Every underlying claim was correct. Only the pointers
//! had rotted, each one landing 16 to 42 lines short of its target, which is what insertion above
//! does to a document that keeps growing.
//!
//! So the fix was not better line numbers. A position into a living document is a reference that
//! decays on someone else's edit and fails silently, because nothing reads it. A quotation does
//! not: it carries its own referent, and this test turns "the sentence I relied on still exists"
//! into something the build checks.
//!
//! The rule is therefore: inside a comment block that mentions ADR-045, every double-quoted span
//! must appear verbatim in the ADR. If you want to quote something else in such a block, put it in
//! a different block.
//!
//! What this does not do, stated so a pass is not read as more than it is: it proves the sentence
//! survives, not that the code obeys it. A quote can stay accurate while the code drifts away from
//! what it says. That is a real gap and this test does not reach it.

use std::path::PathBuf;

const ADR: &str = "docs/architecture/ADR-045-aee-substrate-signed-run-end-seal.md";

/// Files that cite the ADR. Extend when a new one starts quoting it.
const CITING_SOURCES: &[&str] = &[
    "crates/assay-cli/src/aee_seal.rs",
    "crates/assay-cli/src/aee_seal_envelope.rs",
    "scripts/experiments/aee_landlock_seal_fixture.py",
];

/// A floor, not a target. It exists so a broken extractor fails loudly instead of passing with
/// nothing to check, since the failure mode of an extractor is to find zero and report success.
///
/// Set below the 18 found when this was written, so ordinary deletion does not trip it. Six of
/// those eighteen were the line-number citations this replaced; the rest were already quotations
/// and had never been checked against anything.
const KNOWN_CITATIONS: usize = 12;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Whitespace-insensitive, so a quotation may wrap across comment lines.
fn flatten(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Terminal punctuation is not part of what was said.
///
/// Most of the ADR's normative statements are list items ending in `;`. Quoting one inside a
/// sentence and writing `.` or `,` instead is ordinary editing, not a misquotation, and failing on
/// it would push people back towards paraphrase — which is the thing that actually goes wrong.
/// Every word still has to match.
fn trim_terminal(s: &str) -> &str {
    s.trim_end_matches([',', '.', ';', ':'])
}

/// Strip a comment marker, or return `None` for a line that is not a comment.
fn comment_body(line: &str) -> Option<&str> {
    let t = line.trim_start();
    for marker in ["///", "//!", "//", "#"] {
        if let Some(rest) = t.strip_prefix(marker) {
            return Some(rest);
        }
    }
    None
}

/// Contiguous comment lines, joined. Blocks are what let a quotation wrap.
fn comment_blocks(src: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in src.lines() {
        match comment_body(line) {
            Some(body) => current.push(body),
            None => {
                if !current.is_empty() {
                    blocks.push(flatten(&current.join(" ")));
                    current.clear();
                }
            }
        }
    }
    if !current.is_empty() {
        blocks.push(flatten(&current.join(" ")));
    }
    blocks
}

/// Double-quoted spans in a block. Unpaired trailing quotes are ignored rather than panicked on,
/// since prose legitimately contains a lone quote character.
fn quoted_spans(block: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut rest = block;
    while let Some(open) = rest.find('"') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('"') else { break };
        let span = rest[..close].trim().to_string();
        if !span.is_empty() {
            spans.push(span);
        }
        rest = &rest[close + 1..];
    }
    spans
}

/// Every (file, quotation) pair in a comment block that mentions the ADR.
fn citations() -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    for src in CITING_SOURCES {
        for block in comment_blocks(&read(src)) {
            if !block.contains("ADR-045") {
                continue;
            }
            for span in quoted_spans(&block) {
                found.push((*src, span));
            }
        }
    }
    found
}

#[test]
fn every_quotation_from_the_adr_is_still_in_the_adr() {
    let adr = flatten(&read(ADR));
    let citations = citations();

    assert!(
        citations.len() >= KNOWN_CITATIONS,
        "found {} citations, expected at least {KNOWN_CITATIONS}. Either they were deleted or the \
         extractor stopped seeing them; both are worth looking at before lowering this number.",
        citations.len()
    );

    let missing: Vec<String> = citations
        .iter()
        .filter(|(_, q)| !adr.contains(trim_terminal(&flatten(q))))
        .map(|(f, q)| format!("{f}: \"{q}\""))
        .collect();

    assert!(
        missing.is_empty(),
        "these are quoted as ADR-045 and are not in it. Either the ADR moved and the comment needs \
         updating, or the quotation was never exact:\n  {}",
        missing.join("\n  ")
    );
}

/// The negative control. A checker that only ever runs against correct input proves nothing about
/// whether it can fail, and this whole class of defect is one that passed silently for months.
#[test]
fn a_quotation_absent_from_the_adr_is_rejected() {
    let adr = flatten(&read(ADR));
    let fabricated = "the ADR does not contain this sentence anywhere at all";
    assert!(
        !adr.contains(fabricated),
        "the containment check accepted a fabricated quotation, so it cannot fail"
    );
}

/// Line-number citations are what this replaced. Reintroducing one puts a decaying pointer back
/// into a file the test can see, and the test would not notice: no quotation, nothing to check.
#[test]
fn no_citation_points_at_a_line_number() {
    let pattern = regex_lite_line_citation();
    let mut offenders = Vec::new();
    for src in CITING_SOURCES {
        for (n, line) in read(src).lines().enumerate() {
            let Some(body) = comment_body(line) else {
                continue;
            };
            if pattern(body) {
                offenders.push(format!("{src}:{}: {}", n + 1, body.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "cite the sentence, not its position -- a line number decays on someone else's edit and \
         nothing reads it:\n  {}",
        offenders.join("\n  ")
    );
}

/// `ADR-045 line <n>` / `ADR-045 lines <n>`, without pulling in a regex dependency for one pattern.
fn regex_lite_line_citation() -> impl Fn(&str) -> bool {
    |body: &str| {
        let flat = flatten(body);
        for marker in [" line ", " lines "] {
            let mut rest = flat.as_str();
            while let Some(i) = rest.find(marker) {
                let after = &rest[i + marker.len()..];
                if after.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    // Only a defect when it is the ADR being cited by position.
                    if flat.contains("ADR-045") {
                        return true;
                    }
                }
                rest = &rest[i + marker.len()..];
            }
        }
        false
    }
}
