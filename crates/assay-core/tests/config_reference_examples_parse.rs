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

/// Every markdown file under `docs/` that shows a sequence rule.
///
/// Derived, not listed. A hand-kept list was a second place to forget in exactly the way the
/// deleted type allowlist was: it omitted `docs/use-cases/ci-gate.md`, a page whose whole purpose
/// is to be copied, which carried a `before` rule passing a list to a field that takes a string.
fn docs_showing_rules(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "md")
                // `docs/archive/` is kept deliberately as a record of superseded reference text.
                // Holding it to the current schema would be asking history to be current.
                && !p.to_string_lossy().contains("/archive/")
            {
                if let Ok(text) = std::fs::read_to_string(&p) {
                    if text.contains("- type:") {
                        out.push(p.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join("docs"), &mut out);
    out.sort();
    out
}

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
fn rules_in(block: &str) -> Result<Vec<serde_yaml::Value>, String> {
    // Not a skip. A block that is not valid YAML was silently dropped, so a typo, a tab or an
    // undefined alias made every rule inside it invisible -- and one such block was already in
    // the corpus (`concepts/metrics.md`, an undefined `*_dangerous` anchor). A reader copies
    // these; if it does not parse as YAML it does not work for them either.
    let v = serde_yaml::from_str::<serde_yaml::Value>(block)
        .map_err(|e| format!("the yaml block itself does not parse: {e}"))?;
    let mut found = Vec::new();
    collect_rules(&v, &mut found);
    Ok(found
        .into_iter()
        // A rule is a mapping carrying `type`. Other list entries in these docs are not rules.
        .filter(|e| {
            e.as_mapping()
                .is_some_and(|m| m.contains_key(serde_yaml::Value::String("type".into())))
        })
        .collect())
}

/// Every `rules:` sequence anywhere in the value.
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

    let docs = docs_showing_rules(&root);
    for doc in &docs {
        let path = std::path::PathBuf::from(doc);
        let rel = doc
            .strip_prefix(&format!("{}/", root.display()))
            .unwrap_or(doc)
            .to_string();
        let md = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

        for (line, block) in yaml_blocks(&md) {
            // Only blocks that purport to show rules. A block without a `- type:` entry is some
            // other schema, and several docs legitimately show two alternative forms in one
            // fence, which is not valid YAML as a single document and is not meant to be.
            if !block.contains("- type:") {
                continue;
            }
            let rules = match rules_in(&block) {
                Ok(r) => r,
                Err(e) => {
                    failures.push(format!("{rel}:{line} — {e}"));
                    continue;
                }
            };
            for rule in rules {
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
                        "{rel}:{line} — {e}\n  the example reads:\n{}",
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
        docs.len() >= 4,
        "found only {} documents showing rules; the walk stopped matching",
        docs.len()
    );
    assert!(
        checked >= 40,
        "found only {checked} documented rules across {} document(s). The corpus carries well \
         over that, so this means the extractor stopped matching rather than that the docs are \
         clean.",
        docs.len()
    );
    assert!(
        failures.is_empty(),
        "{} documented sequence rule(s) do not parse:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[cfg(test)]
mod extractor {
    /// A block that is not valid YAML is a failure, not a skip.
    ///
    /// The corpus is clean, so nothing in the documents exercises this: reverting the guard
    /// leaves every test green. It is pinned here instead, because the case it exists for --
    /// an undefined anchor hiding every rule in its block -- was live in the corpus until this
    /// PR and will be live again the next time someone writes a tab.
    #[test]
    fn an_unparsable_block_is_reported_not_skipped() {
        let block = "rules:\n  - type: blocklist\n    pattern: *undefined_anchor\n";
        let err = super::rules_in(block).expect_err("invalid yaml must not be silently dropped");
        assert!(err.contains("does not parse"), "got {err}");
    }

    /// Rules are found at any depth, because depth is not a property of a rule.
    #[test]
    fn rules_are_found_under_a_nested_key() {
        let block = "tests:\n  - id: x\n    metric: sequence_valid\n    rules:\n      - type: require\n        tool: A\n";
        let found = super::rules_in(block).expect("parses");
        assert_eq!(found.len(), 1, "nested rules were not collected: {found:?}");
    }

    /// A block carrying no rules yields none rather than erroring.
    #[test]
    fn a_block_without_rules_yields_nothing() {
        let block = "suite: x\nversion: \"1\"\n";
        assert!(super::rules_in(block).expect("parses").is_empty());
    }
}
