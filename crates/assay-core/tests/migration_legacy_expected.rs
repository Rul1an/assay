//! Which configs `assay migrate` reports as carrying an older `expected` form, and which it refuses.
//!
//! `load_config_for_migration` decides `legacy_expected`: whether any test's `expected` is not the
//! current tagged mapping. Each case is checked against the loader and, where the loader accepts
//! the file, against the second reader the decision was first made with: the `expected` values read
//! back from the YAML on their own and tried as a tagged `Expected`.

use assay_core::config::load_config_for_migration;
use assay_core::model::Expected;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Outcome {
    Refused,
    Legacy(bool),
}
use Outcome::{Legacy, Refused};

const HEAD: &str = "suite: s\nmodel: trace\ntests:\n";

/// The decision as it was made before the loader returned it: every `expected` read on its own.
fn second_reader(text: &str) -> bool {
    #[derive(serde::Deserialize)]
    struct Spellings {
        tests: Vec<Spelling>,
    }
    #[derive(serde::Deserialize)]
    struct Spelling {
        expected: Option<serde_json::Value>,
    }
    let spellings: Spellings = serde_yaml::from_str(text).expect("the loader accepted it");
    spellings
        .tests
        .into_iter()
        .filter_map(|test| test.expected)
        .any(|value| serde_json::from_value::<Expected>(value).is_err())
}

fn case(expected: &str) -> String {
    format!("{HEAD}  - id: t\n    input: hello\n{expected}")
}

#[test]
fn the_older_expected_forms_and_their_refusals() {
    let cases: Vec<(&str, String, Outcome)> = vec![
        (
            "current",
            case("    expected: {type: must_contain, must_contain: [hello]}\n"),
            Legacy(false),
        ),
        (
            "legacy mapping",
            case("    expected: {must_contain: [hello]}\n"),
            Legacy(true),
        ),
        (
            "list of one current",
            case("    expected: [{type: must_contain, must_contain: [hello]}]\n"),
            Legacy(true),
        ),
        (
            "list of one legacy",
            case("    expected: [{must_contain: [hello]}]\n"),
            Legacy(true),
        ),
        (
            "tagged scalar shorthand",
            case("    expected: {type: must_contain, must_contain: hello}\n"),
            Legacy(true),
        ),
        (
            "historical sequence tag",
            case("    expected: {type: sequence, sequence: [a, b]}\n"),
            Legacy(true),
        ),
        (
            "external reference",
            case("    expected: {$ref: other.yaml}\n"),
            Legacy(true),
        ),
        ("absent", case(""), Legacy(false)),
        (
            "field alias",
            case("    expected: {type: semantic_similarity_to, text: hello}\n"),
            Legacy(false),
        ),
        (
            "anchored legacy reused by alias",
            format!(
                "{HEAD}  - id: a\n    input: one\n    expected: &e {{must_contain: [hello]}}\n  - id: b\n    input: two\n    expected: *e\n"
            ),
            Legacy(true),
        ),
        (
            "anchored current reused by alias",
            format!(
                "{HEAD}  - id: a\n    input: one\n    expected: &e {{type: must_contain, must_contain: [hello]}}\n  - id: b\n    input: two\n    expected: *e\n"
            ),
            Legacy(false),
        ),
        (
            "current and legacy",
            format!(
                "{HEAD}  - id: a\n    input: one\n    expected: {{type: must_contain, must_contain: [hello]}}\n  - id: b\n    input: two\n    expected: {{must_contain: [hello]}}\n"
            ),
            Legacy(true),
        ),
        (
            "open JSON",
            case("    expected: {type: tool_output_valid, schemas: {Search: {type: object, required: [results]}}}\n"),
            Legacy(false),
        ),
        (
            "open JSON with a NaN",
            case("    expected: {type: tool_output_valid, schemas: {Search: {type: object, required: [results], x-nan: .nan}}}\n"),
            Legacy(false),
        ),
        (
            "open JSON with a 2^64",
            case("    expected: {type: tool_output_valid, schemas: {Search: {type: object, required: [results], x-big: 18446744073709551616}}}\n"),
            Refused,
        ),
        (
            "integer keys in place of field names",
            format!("{HEAD}  - 0: t\n    1: hello\n    2: {{type: must_contain, must_contain: [hello]}}\n"),
            Refused,
        ),
        ("null", case("    expected: null\n"), Refused),
        ("empty list", case("    expected: []\n"), Refused),
        (
            "two in a list",
            case("    expected: [{must_contain: [a]}, {must_contain: [b]}]\n"),
            Refused,
        ),
        (
            "unknown legacy key",
            case("    expected: {must_include: [hello]}\n"),
            Refused,
        ),
        (
            "duplicate test field",
            format!("{HEAD}  - id: t\n    id: u\n    input: hello\n"),
            Refused,
        ),
        (
            "a core YAML tag inside a current value",
            case("    expected: {type: must_contain, must_contain: [!!str 012]}\n"),
            Legacy(false),
        ),
        (
            "tagged expected",
            case("    expected: !x {type: must_contain, must_contain: [hello]}\n"),
            Refused,
        ),
    ];

    let dir = tempfile::tempdir().expect("tempdir");
    let mut differences = Vec::new();
    for (name, text, expected) in cases {
        let path = dir.path().join("eval.yaml");
        std::fs::write(&path, &text).expect("write");
        let actual = match load_config_for_migration(&path) {
            Ok(input) => {
                assert_eq!(
                    input.legacy_expected,
                    second_reader(&text),
                    "{name}: the loader and the second reader disagree"
                );
                Legacy(input.legacy_expected)
            }
            Err(_) => Refused,
        };
        if actual != expected {
            differences.push(format!("{name}: expected {expected:?}, got {actual:?}"));
        }
    }
    assert!(differences.is_empty(), "{differences:#?}");
}
