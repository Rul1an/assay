//! Every `assertions:` example in the documentation must load (#1960).
//!
//! `docs/metrics/index.md` and `docs/architecture/agents.md` document the inline assertion
//! surface. Before this test, not one of their examples parsed: five of the seven documented
//! types did not exist, the two that did were shown with field names that did not, and the
//! example labelled "must NOT use a forbidden tool" inverted into "must be called at least
//! once" once its field name was corrected.
//!
//! Review cannot keep prose and an enum in agreement — nothing failed when they diverged. This
//! test is the mechanism that does: it extracts every fenced YAML block from those documents
//! and pushes it through the same code a user's config goes through. A documented type or field
//! that stops existing fails here.
//!
//! It also checks the reverse direction. A catalogue that silently omits shipped variants is a
//! subtler version of the same defect, so every variant in the enum must appear in the
//! catalogue, and every type the catalogue lists as unimplemented must in fact be rejected.

use assay_core::agent_assertions::model::TraceAssertion;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

const METRICS_DOC: &str = "docs/metrics/index.md";
const AGENTS_DOC: &str = "docs/architecture/agents.md";

/// Every variant of `TraceAssertion`, by its wire name. Adding a variant without documenting it
/// fails `every_shipped_variant_is_documented` below.
const SHIPPED_TYPES: &[&str] = &[
    "trace_must_call_tool",
    "trace_must_not_call_tool",
    "trace_tool_sequence",
    "trace_max_steps",
    "args_valid",
    "sequence_valid",
    "tool_blocklist",
];

struct Block {
    doc: &'static str,
    line: usize,
    body: String,
}

/// Extracts fenced ```yaml blocks with the line number of the opening fence, so a failure points
/// at the source rather than at an anonymous snippet.
fn yaml_blocks(doc: &'static str) -> Vec<Block> {
    let text = std::fs::read_to_string(repo_root().join(doc))
        .unwrap_or_else(|e| panic!("cannot read {doc}: {e}"));
    let mut out = Vec::new();
    let mut current: Option<(usize, Vec<&str>)> = None;

    for (idx, line) in text.lines().enumerate() {
        match current.take() {
            None => {
                if line.trim_start().starts_with("```yaml") {
                    current = Some((idx + 1, Vec::new()));
                }
            }
            Some((start, mut acc)) => {
                if line.trim_start().starts_with("```") {
                    out.push(Block {
                        doc,
                        line: start,
                        body: acc.join("\n"),
                    });
                } else {
                    acc.push(line);
                    current = Some((start, acc));
                }
            }
        }
    }
    assert!(
        !out.is_empty(),
        "{doc}: no fenced yaml blocks found — the extractor or the document moved"
    );
    out
}

/// A block is either one assertion or a whole config. Both are checked, through the path a user
/// would actually take.
fn check_block(b: &Block) {
    let looks_like_config = b.body.contains("configVersion:") || b.body.contains("\ntests:");

    if looks_like_config {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("eval.yaml");
        std::fs::write(&path, &b.body).expect("write config");
        if let Err(e) = assay_core::config::load_config(&path, false, false) {
            panic!(
                "{}:{} — documented config does not load: {e}\n---\n{}\n---",
                b.doc, b.line, b.body
            );
        }
        return;
    }

    // Otherwise it must be a single assertion. Anything else in a fenced yaml block on these
    // pages is a documentation bug in its own right, so require the tag rather than skipping.
    assert!(
        b.body.contains("type:"),
        "{}:{} — fenced yaml block is neither a config nor an assertion:\n{}",
        b.doc,
        b.line,
        b.body
    );
    if let Err(e) = serde_yaml::from_str::<TraceAssertion>(&b.body) {
        panic!(
            "{}:{} — documented assertion does not parse: {e}\n---\n{}\n---",
            b.doc, b.line, b.body
        );
    }
}

#[test]
fn every_documented_assertion_example_parses() {
    for doc in [METRICS_DOC, AGENTS_DOC] {
        for block in yaml_blocks(doc) {
            check_block(&block);
        }
    }
}

/// The other direction: the catalogue must not quietly omit shipped variants. Five of the seven
/// were missing when this was filed, which is how the documented surface and the implemented one
/// drifted far enough for none of the examples to work.
#[test]
fn every_shipped_variant_is_documented() {
    let text = std::fs::read_to_string(repo_root().join(METRICS_DOC)).expect("read metrics doc");
    for ty in SHIPPED_TYPES {
        assert!(
            text.contains(&format!("### `{ty}`")),
            "{METRICS_DOC}: `{ty}` exists in TraceAssertion but has no section in the catalogue"
        );
    }
}

/// The "Not implemented" table is a claim about the parser, so it is checked against the parser.
/// If one of these is ever built, this test fails and the table has to be updated with it.
#[test]
fn types_documented_as_unimplemented_are_actually_rejected() {
    for ty in [
        "trace_no_tool_call",
        "trace_tool_args_match",
        "trace_tool_args_schema",
        "trace_tool_call_count",
        "trace_no_tool_errors",
    ] {
        let yaml = format!("type: {ty}\n");
        assert!(
            serde_yaml::from_str::<TraceAssertion>(&yaml).is_err(),
            "`{ty}` is listed as not implemented but the parser accepts it"
        );
        let text =
            std::fs::read_to_string(repo_root().join(METRICS_DOC)).expect("read metrics doc");
        assert!(
            text.contains(&format!("`{ty}`")),
            "{METRICS_DOC}: `{ty}` is rejected by the parser but is not listed under \
             \"Not implemented\", so a reader has no way to learn what to write instead"
        );
    }
}
