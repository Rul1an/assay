//! Every sequence rule written in the documentation must parse.
//!
//! The parity test beside this one compares headings against variant names, and that is a weaker
//! property than it looks. It passed on a `sequences.md` whose "Real-World Patterns" section still
//! told a reader to write `type: immediately_before`, and on a `sequence-valid.md` whose
//! `blocklist` example named a field the variant does not have — `tools:` where the type takes
//! `pattern:`, which serde rejects with "missing field `pattern`". Neither is a heading, so
//! neither was visible.
//!
//! The stronger property is simply that the examples run. A reader does not copy a heading; they
//! copy the block underneath it. So this deserialises every rule the docs show, through the same
//! type the loader uses, and reports the file and the snippet when one fails.
//!
//! Scope, stated because it bounds the guarantee: this proves a rule is *well-formed*, not that
//! the surrounding prose describes its behaviour correctly. A `before` example that parses but is
//! documented with the wrong pass/fail table is beyond what any parser can catch.

use assay_core::model::SequenceRule;
use std::path::{Path, PathBuf};

/// Documents that show sequence rules a reader is expected to copy.
const DOCS: &[&str] = &[
    "docs/reference/config/sequences.md",
    "docs/metrics/sequence-valid.md",
    "docs/concepts/metrics.md",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Every fenced yaml block in the document.
fn yaml_blocks(md: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut lines = md.lines().enumerate();
    while let Some((i, l)) = lines.next() {
        if l.trim_start().starts_with("```yaml") {
            let mut body = String::new();
            for (_, b) in lines.by_ref() {
                if b.trim_start().starts_with("```") {
                    break;
                }
                body.push_str(b);
                body.push('\n');
            }
            out.push((i + 1, body));
        }
    }
    out
}

/// The rule mappings inside one block, as YAML values, at any depth.
///
/// An earlier version looked only for a top-level `rules:` key or a top-level sequence, and the
/// documents nest: `metric:` → `rules:`, `tests:` → `rules:`. Eight blocks were invisible, two of
/// them carrying a `blocklist` rule written with `tools:` — the exact defect this test was added
/// to catch, sitting in the half it could not see. Depth is not a property of a rule, so it must
/// not be a property of the search.
fn rules_in(block: &str) -> Vec<serde_yaml::Value> {
    let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(block) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    collect_rules(&v, &mut found);
    found
        .into_iter()
        // A rule is a mapping carrying `type`. Other list entries in these docs are not rules.
        .filter(|e| {
            e.as_mapping()
                .is_some_and(|m| m.contains_key(serde_yaml::Value::String("type".into())))
        })
        .collect()
}

/// Every `rules:` sequence anywhere in the value, plus a bare top-level sequence.
fn collect_rules(v: &serde_yaml::Value, out: &mut Vec<serde_yaml::Value>) {
    match v {
        serde_yaml::Value::Mapping(m) => {
            for (k, val) in m {
                if k.as_str() == Some("rules") {
                    if let Some(seq) = val.as_sequence() {
                        out.extend(seq.iter().cloned());
                        continue;
                    }
                }
                collect_rules(val, out);
            }
        }
        serde_yaml::Value::Sequence(s) => {
            for e in s {
                collect_rules(e, out);
            }
        }
        _ => {}
    }
}

#[test]
fn every_documented_sequence_rule_parses() {
    let root = workspace_root();
    let mut failures = Vec::new();
    let mut checked = 0usize;

    for doc in DOCS {
        let path = root.join(doc);
        let md = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

        for (line, block) in yaml_blocks(&md) {
            for rule in rules_in(&block) {
                // Only rules the loader would see: a `type` this crate owns. A block describing a
                // different schema that happens to use `type` is not this test's business.
                let printed = serde_yaml::to_string(&rule).unwrap_or_default();
                // No type allowlist. Everything under a `rules:` key is a sequence rule by
                // construction, and a hand-kept list of "types this crate owns" was a second
                // place to forget: it omitted `count`, so every broken `count` example was
                // skipped as though it belonged to some other schema.
                checked += 1;
                if let Err(e) = serde_yaml::from_value::<SequenceRule>(rule) {
                    failures.push(format!(
                        "{doc}:{line} — {e}\n  the example reads:\n{}",
                        printed
                            .lines()
                            .map(|l| format!("    {l}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ));
                }
            }
        }
    }

    assert!(
        checked >= 40,
        "found only {checked} documented rules to check. The four documents carry far more than \
         that, so this means the extractor stopped matching rather than that the docs are clean."
    );
    assert!(
        failures.is_empty(),
        "{} documented sequence rule(s) do not parse:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
