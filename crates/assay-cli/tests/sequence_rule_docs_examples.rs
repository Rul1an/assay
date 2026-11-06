//! Contract: every YAML example in the sequence-rule documentation is a configuration that
//! `assay validate` accepts, and every rule in it uses only fields its rule type has.
//!
//! The examples in these pages used a shape the config model never had (`metric:` and `rules:`
//! directly under a test). One of them also advertised a `when:` key that `sequence_valid` never
//! read, and `assay validate` accepts unknown rule keys silently. So this test checks the shape
//! through the real binary, and the field names by round-tripping each rule through
//! `SequenceRule`.
//!
//! A block that is deliberately not a configuration (the v0 format `assay migrate` reads, a
//! fragment that only shows test ids) is marked with a `<!-- docs-example: not-a-config (why) -->`
//! line right before its fence. The set of such markers is pinned below, so adding one is a
//! decision rather than a way around this test.

use assay_core::model::SequenceRule;
use serde_yaml::{Mapping, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

const PAGES: &[&str] = &[
    "docs/reference/config/sequences.md",
    "docs/metrics/sequence-valid.md",
];

const NOT_A_CONFIG_MARKER: &str = "<!-- docs-example: not-a-config";

/// Every exemption, as (page, the marker's reason). Pinned so a new one is reviewed on purpose.
const EXPECTED_EXEMPTIONS: &[(&str, &str)] = &[
    (
        "docs/reference/config/sequences.md",
        "the v0 format that assay migrate reads",
    ),
    ("docs/reference/config/sequences.md", "shows test ids only"),
];

struct Block {
    page: &'static str,
    line: usize,
    yaml: String,
    exemption: Option<String>,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn blocks(page: &'static str) -> Vec<Block> {
    let text = std::fs::read_to_string(workspace_root().join(page))
        .unwrap_or_else(|e| panic!("read {page}: {e}"));
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_end() == "```yaml" {
            let previous = lines[..i]
                .iter()
                .rev()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim());
            let exemption = previous
                .filter(|l| l.starts_with(NOT_A_CONFIG_MARKER))
                .map(|l| {
                    let inner = l
                        .trim_start_matches(NOT_A_CONFIG_MARKER)
                        .trim_end_matches("-->")
                        .trim();
                    inner
                        .strip_prefix('(')
                        .and_then(|s| s.strip_suffix(')'))
                        .unwrap_or_else(|| panic!("{page}:{}: marker needs a (reason)", i + 1))
                        .to_string()
                });
            let start = i + 1;
            let end = lines[start..]
                .iter()
                .position(|l| l.trim_end() == "```")
                .map(|p| start + p)
                .unwrap_or_else(|| panic!("{page}:{}: unterminated yaml block", i + 1));
            out.push(Block {
                page,
                line: i + 1,
                yaml: lines[start..end].join("\n"),
                exemption,
            });
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn mapping(pairs: Vec<(&str, Value)>) -> Value {
    let mut m = Mapping::new();
    for (k, v) in pairs {
        m.insert(Value::String(k.to_string()), v);
    }
    Value::Mapping(m)
}

fn config_around(expected: Value) -> Value {
    mapping(vec![
        ("configVersion", Value::Number(1.into())),
        ("suite", Value::String("docs_example".into())),
        ("model", Value::String("trace".into())),
        (
            "tests",
            Value::Sequence(vec![mapping(vec![
                ("id", Value::String("docs_example".into())),
                (
                    "input",
                    mapping(vec![("prompt", Value::String("docs".into()))]),
                ),
                ("expected", expected),
            ])]),
        ),
    ])
}

/// Turn one example into a whole configuration, plus the rules it declares.
fn as_config(block: &Block, doc: &Value) -> Value {
    let where_ = format!("{}:{}", block.page, block.line);
    let Value::Mapping(top) = doc else {
        panic!("{where_}: example is not a mapping; mark it not-a-config with a reason");
    };
    let keys: BTreeSet<&str> = top.keys().filter_map(Value::as_str).collect();
    if keys.contains("tests") {
        // The loader ignores keys it does not know and an omitted `expected:` defaults to an
        // empty check, so a test that puts `rules:` beside `id:` validates while asserting
        // nothing. On these pages every example test must carry the expectation itself.
        for test in doc["tests"].as_sequence().into_iter().flatten() {
            let expected = &test["expected"];
            assert!(
                expected["type"].as_str() == Some("sequence_valid")
                    && (expected["rules"].is_sequence() || expected["policy"].is_string()),
                "{where_}: example test {:?} has no `expected: {{ type: sequence_valid, rules | policy }}`",
                test["id"]
            );
        }
        return doc.clone();
    }
    if keys == BTreeSet::from(["rules"]) {
        return config_around(mapping(vec![
            ("type", Value::String("sequence_valid".into())),
            ("rules", top[&Value::String("rules".into())].clone()),
        ]));
    }
    if keys == BTreeSet::from(["expected"]) {
        return config_around(top[&Value::String("expected".into())].clone());
    }
    panic!(
        "{where_}: example has top-level keys {keys:?}; it is neither a configuration, a `rules:` \
         list nor an `expected:` block. Fix it, or mark it not-a-config with a reason."
    );
}

/// Every rule mapping in a configuration's `sequence_valid` expectations.
fn rules_in(config: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for test in config["tests"].as_sequence().into_iter().flatten() {
        if let Some(rules) = test["expected"]["rules"].as_sequence() {
            out.extend(rules.iter().cloned());
        }
    }
    out
}

/// A rule's fields must all be fields of its type, at every depth. `SequenceRule` and the untagged
/// `CallSelector` ignore unknown keys, so a typo or an invented option (on the rule, or inside a
/// `{ tool, args_match }` selector) would otherwise parse into a rule that does not say what the
/// page says.
fn assert_rule_fields_known(where_: &str, rule: &Value) {
    let parsed: SequenceRule = serde_yaml::from_value(rule.clone())
        .unwrap_or_else(|e| panic!("{where_}: rule does not parse: {e}\n{rule:?}"));
    let known = serde_yaml::to_value(&parsed).expect("serialize rule");
    assert!(
        rule.is_mapping(),
        "{where_}: rule is not a mapping: {rule:?}"
    );
    assert_keys_known(where_, "rule", rule, &known);
}

/// Every key written in `written` must exist in `known` (the value re-serialized from the parsed
/// type), recursing into nested mappings and sequences.
fn assert_keys_known(where_: &str, path: &str, written: &Value, known: &Value) {
    match (written, known) {
        (Value::Mapping(w), Value::Mapping(k)) => {
            for (key, value) in w {
                let name = key.as_str().unwrap_or("<non-string key>");
                let Some(known_value) = k.get(key) else {
                    panic!("{where_}: {path} has a field its type does not read: `{name}`");
                };
                assert_keys_known(where_, &format!("{path}.{name}"), value, known_value);
            }
        }
        (Value::Sequence(w), Value::Sequence(k)) => {
            assert_eq!(w.len(), k.len(), "{where_}: {path} changed length on parse");
            for (i, (wv, kv)) in w.iter().zip(k).enumerate() {
                assert_keys_known(where_, &format!("{path}[{i}]"), wv, kv);
            }
        }
        (Value::Mapping(_), other) => {
            panic!("{where_}: {path} is written as a mapping but parses as {other:?}")
        }
        // A selector written as a bare string re-serializes as a string; scalars carry no keys.
        _ => {}
    }
}

fn validate(where_: &str, config: &Value) {
    let dir = tempfile::tempdir().expect("tempdir");
    // A `policy:` file named by an example is not part of the example; supply a minimal one.
    for test in config["tests"].as_sequence().into_iter().flatten() {
        if let Some(policy) = test["expected"]["policy"].as_str() {
            std::fs::write(dir.path().join(policy), "- { type: require, tool: A }\n")
                .expect("policy file");
        }
    }
    let text = serde_yaml::to_string(config).expect("serialize config");
    std::fs::write(dir.path().join("eval.yaml"), &text).expect("eval.yaml");
    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .args(["validate", "--config", "eval.yaml"])
        .output()
        .expect("run assay validate");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{where_}: assay validate refused the example\n--- config ---\n{text}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn every_sequence_rule_docs_example_validates() {
    let mut exemptions = Vec::new();
    for &page in PAGES {
        let found = blocks(page);
        let fences = std::fs::read_to_string(workspace_root().join(page))
            .expect("read page")
            .lines()
            .filter(|l| l.trim_end() == "```yaml")
            .count();
        assert_eq!(found.len(), fences, "{page}: extractor missed a yaml block");

        let mut full_configs = 0;
        for block in &found {
            let where_ = format!("{}:{}", block.page, block.line);
            if let Some(reason) = &block.exemption {
                exemptions.push((block.page, reason.clone()));
                continue;
            }
            let doc: Value = serde_yaml::from_str(&block.yaml)
                .unwrap_or_else(|e| panic!("{where_}: not YAML: {e}"));
            if doc.get("tests").is_some() {
                full_configs += 1;
            }
            let config = as_config(block, &doc);
            for rule in rules_in(&config) {
                assert_rule_fields_known(&where_, &rule);
            }
            validate(&where_, &config);
        }
        assert!(
            full_configs > 0,
            "{page}: no complete configuration example was checked"
        );
    }
    let expected: Vec<(&str, String)> = EXPECTED_EXEMPTIONS
        .iter()
        .map(|(p, r)| (*p, (*r).to_string()))
        .collect();
    assert_eq!(exemptions, expected, "not-a-config markers changed");
}
