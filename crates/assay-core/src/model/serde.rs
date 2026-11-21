mod test_case;
use test_case::read_test_case;

use crate::on_error::ErrorPolicy;
use serde::de::Error as _;
use serde::Deserialize;

use super::types::{Expected, TestCase, TestInput};

/// Legacy (pre-`type:`-tagged) keys an `expected:` block may still be written with.
///
/// This surface is frozen: new metrics are only reachable through the tagged form
/// (`type: <metric>`). It is listed here so error messages can name the accepted
/// alternatives instead of leaving the author to guess.
const LEGACY_EXPECTED_KEYS: [&str; 4] = ["$ref", "must_contain", "sequence", "schema"];

#[derive(Debug, Clone, Default, PartialEq)]
enum RawExpected {
    #[default]
    Missing,
    Present(serde_json::Value),
}

impl assay_common::doc::ClosedValue for RawExpected {
    fn __shape() -> assay_common::doc::Shape {
        // The converter can read one assertion wrapped in a list. Runtime decoding still
        // refuses that form before unused fields are reported.
        assay_common::doc::Shape::Json(
            assay_common::doc::SeqOrMap::<Vec<Expected>, Expected>::__shape,
        )
    }
}

impl<'de> Deserialize<'de> for RawExpected {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let checked = assay_common::unique_value::UniqueValue::deserialize(deserializer)?;
        checked
            .into_native_value()
            .map(Self::Present)
            .map_err(D::Error::custom)
    }
}

/// Describe a JSON value's shape for error messages.
fn value_kind(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "a list",
        serde_json::Value::Object(_) => "a mapping",
    }
}

/// A referenced assertion follows the same tagged and legacy grammar as an inline entry.
pub(crate) fn read_referenced_expected(path: &std::path::Path) -> anyhow::Result<Expected> {
    use anyhow::Context;
    let text = super::read_policy_text(path)
        .with_context(|| format!("failed to read policy file: {}", path.display()))?;
    let input = assay_common::doc::JsonInput::parse(&text)
        .map_err(super::policy_file::policy_refusal)
        .with_context(|| format!("failed to parse policy: {}", path.display()))?;
    assay_common::unique_value::reject_raw_value_marker(input.source())
        .with_context(|| format!("failed to parse policy: {}", path.display()))?;
    parse_expected_entry(input.source()).map(|(expected, _)| expected).map_err(|error| anyhow::anyhow!(
        "failed to resolve expected reference '{}': {error}. A referenced args_valid policy must be an Expected block or a tool-name-to-schema map under type: args_valid",
        path.display()
    ))
}

/// Parse a single `expected:` entry into an [`Expected`].
///
/// Resolution order is strict V1 (`type:`-tagged) first, then the frozen legacy
/// heuristics in [`LEGACY_EXPECTED_KEYS`].
///
/// The second value says whether the entry was the tagged form itself.
///
/// An entry matching neither is an error and **never** falls back to
/// `Expected::default()`. That default is an empty `must_contain`, which the
/// `must_contain` metric passes unconditionally, so a silent fallback turns a
/// misspelled key into a test that always reports green.
///
/// The same function serves both the scalar and the list position. Keeping one
/// implementation is deliberate: the two positions previously diverged (the list
/// branch applied legacy heuristics, the scalar branch did not), which made
/// `expected: {must_contain: [...]}` silently vacuous while the list-wrapped form
/// of the same YAML worked.
fn parse_expected_entry(item: &serde_json::Value) -> Result<(Expected, bool), String> {
    // 1. Strict V1 (tagged).
    let strict_err = match serde_json::from_value::<Expected>(item.clone()) {
        Ok(exp) => return reject_for_parse(exp).map(|exp| (exp, true)),
        Err(e) => e,
    };

    let Some(obj) = item.as_object() else {
        return Err(format!(
            "`expected:` must be a mapping (or a list of one mapping), found {}",
            value_kind(item)
        ));
    };

    let matched_keys: Vec<&str> = LEGACY_EXPECTED_KEYS
        .iter()
        .copied()
        .filter(|key| obj.contains_key(*key))
        .collect();

    // A failed tagged parse may enter the legacy decoder only for the two frozen
    // compatibility forms. Establish that before decoding any legacy value: an
    // unrelated malformed value must not replace the original tagged error.
    if let Some(tag) = obj.get("type") {
        let compatibility_key = match tag.as_str() {
            Some("must_contain") => "must_contain",
            Some("sequence") => "sequence",
            _ => return Err(format!("invalid `expected:` block: {}", strict_err)),
        };

        if matched_keys.len() > 1 {
            return Err(format!(
                "ambiguous legacy `expected:` block contains multiple assertions {:?}; \
                 use one tagged assertion or move additional checks to `assertions:`",
                matched_keys
            ));
        }

        let only_compatibility_key = matched_keys.as_slice() == [compatibility_key];
        let has_unknown_key = obj
            .keys()
            .any(|key| key != "type" && key != compatibility_key);
        if !only_compatibility_key || has_unknown_key {
            return Err(format!("invalid `expected:` block: {}", strict_err));
        }
    } else {
        let unknown_keys: Vec<&str> = obj
            .keys()
            .map(String::as_str)
            .filter(|key| !LEGACY_EXPECTED_KEYS.contains(key))
            .collect();
        if !unknown_keys.is_empty() {
            return Err(format!(
                "unrecognized legacy `expected:` key(s) {:?}; supported keys are {:?}",
                unknown_keys, LEGACY_EXPECTED_KEYS
            ));
        }
    }

    // 2. Legacy heuristics.
    //
    // These also recognize two tagged compatibility forms: a scalar value for
    // `type: must_contain`, and the historical `type: sequence`. A failed tagged
    // parse may not fall back through an unrelated legacy key, because that would
    // silently change the metric the author selected.
    let mut parsed = None;

    if let Some(r) = obj.get("$ref") {
        let path = r
            .as_str()
            .ok_or_else(|| format!("`$ref` must be a string, found {}", value_kind(r)))?;
        parsed = Some(Expected::Reference {
            path: path.to_string(),
        });
    }

    // Don't chain else-ifs, check all to detect ambiguity
    if let Some(mc) = obj.get("must_contain") {
        // No `unwrap_or_default()` here: an unparsable value used to collapse to an
        // empty vec, i.e. an assertion that passes for any response — the very bug
        // this module exists to prevent.
        let val: Vec<String> = if let Some(s) = mc.as_str() {
            vec![s.to_string()]
        } else {
            serde_json::from_value(mc.clone()).map_err(|e| {
                format!(
                    "`must_contain` must be a string or a list of strings, found {}: {}",
                    value_kind(mc),
                    e
                )
            })?
        };
        // Last match wins for parsed, but we warn below
        if parsed.is_none() {
            parsed = Some(Expected::MustContain { must_contain: val });
        }
    }

    if let Some(seq) = obj.get("sequence") {
        if parsed.is_none() {
            // Previously `.ok()`, which turned a bad value into `sequence: None`.
            // `sequence_valid` passes unconditionally when it has neither a sequence
            // nor rules, so that silently produced an always-green test.
            let sequence: Vec<String> = serde_json::from_value(seq.clone()).map_err(|e| {
                format!(
                    "`sequence` must be a list of strings, found {}: {}",
                    value_kind(seq),
                    e
                )
            })?;
            parsed = Some(Expected::SequenceValid {
                policy: None,
                sequence: Some(sequence),
                rules: None,
            });
        }
    }

    if obj.get("schema").is_some() && parsed.is_none() {
        parsed = Some(Expected::ArgsValid {
            policy: None,
            schema: obj.get("schema").cloned(),
        });
    }

    if matched_keys.len() > 1 {
        return Err(format!(
            "ambiguous legacy `expected:` block contains multiple assertions {:?}; \
             use one tagged assertion or move additional checks to `assertions:`",
            matched_keys
        ));
    }

    if let Some(p) = parsed {
        return reject_for_parse(p).map(|p| (p, false));
    }

    // 3. Nothing matched. A block that carries `type:` was asking for the tagged
    // form, so report why that parse failed rather than listing legacy keys it
    // never wanted.
    if obj.contains_key("type") {
        return Err(format!("invalid `expected:` block: {}", strict_err));
    }

    let found: Vec<&str> = obj.keys().map(String::as_str).collect();
    Err(format!(
        "unrecognized `expected:` block, found key(s) {:?}. Use the tagged form \
         (e.g. `type: must_contain` with `must_contain: [...]`) or one of the legacy \
         keys {:?}",
        found, LEGACY_EXPECTED_KEYS
    ))
}

/// Reject an `expected:` block that was written out in full but asserts nothing.
///
/// An empty `must_contain` / `must_not_contain` gives the metric no substring to
/// look for, so it passes for any response. Catching it here rather than only in
/// `assay validate` matters: this path runs for every command that loads a config,
/// including `assay run` and `assay ci`, which are the gates that decide outcomes.
///
/// This applies only to an assertion the author actually wrote. Omitting `expected:`
/// altogether stays permissive — see the note in the `TestCase` deserializer — and
/// is reported as a warning by the `W_CFG_VACUOUS_EXPECTED` rule instead.
fn reject_vacuous(exp: Expected) -> Result<Expected, String> {
    let Some(field) = super::validation::vacuous_expected_field(&exp) else {
        return Ok(exp);
    };

    Err(format!(
        "`{}` asserts nothing, so this test would pass for any response. \
         Give it at least one entry, or remove the `expected:` block and put the \
         test's checks in `assertions:`.",
        field
    ))
}

fn reject_for_parse(exp: Expected) -> Result<Expected, String> {
    if let Expected::ArgsValid {
        schema: Some(schema),
        ..
    } = &exp
    {
        super::validation::validate_inline_args_controls(schema).map_err(|e| e.to_string())?;
    }
    let exp = reject_vacuous(exp)?;
    if let Some(reason) = super::validation::non_executable_expected_reason(&exp) {
        return Err(format!("expected block is not executable: {reason}"));
    }
    if let Some(reason) = super::validation::ineffective_expected_reason(&exp) {
        return Err(reason.to_string());
    }
    Ok(exp)
}

/// Parse the whole `expected:` value (scalar or list form) for one test case.
///
/// Multi-element lists are rejected. `TestCase::expected` holds exactly one
/// [`Expected`]; the previous code kept element 0 and dropped the rest without a
/// word, so a two-assertion block enforced half of what it claimed. Supporting
/// them properly would mean making `expected` a collection, which changes the
/// metric-dispatch contract everywhere it is matched on; until that happens the
/// honest behaviour is to refuse the input and name the fix. Single-element lists
/// stay accepted for legacy compatibility.
///
/// The second value says whether the value is in an older form: a list, or an entry that is not
/// the tagged form itself.
fn parse_expected_value(
    test_id: &str,
    val: &serde_json::Value,
) -> Result<(Expected, bool), String> {
    let Some(arr) = val.as_array() else {
        return parse_expected_entry(val)
            .map(|(expected, tagged)| (expected, !tagged))
            .map_err(|e| format!("test '{}': {}", test_id, e));
    };

    match arr.len() {
        0 => Err(format!(
            "test '{}': `expected:` is an empty list, which asserts nothing. \
             Remove the key or give it an assertion.",
            test_id
        )),
        1 => parse_expected_entry(&arr[0])
            .map(|(expected, _)| (expected, true))
            .map_err(|e| format!("test '{}': `expected:` entry 0 is invalid: {}", test_id, e)),
        n => Err(format!(
            "test '{}': `expected:` has {} entries but only one is supported \
             (earlier versions silently dropped all but the first). \
             Split them into separate tests, or move the extra checks to `assertions:`.",
            test_id, n
        )),
    }
}

/// A test case as the converter reads it, and whether its `expected` is in an older form.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MigrationTestCase(pub(crate) TestCase, pub(crate) bool);

impl assay_common::doc::ClosedValue for MigrationTestCase {
    fn __shape() -> assay_common::doc::Shape {
        RawTestCase::__shape()
    }
}

impl<'de> Deserialize<'de> for MigrationTestCase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut older = false;
        let case = read_test_case(deserializer, |id, value| {
            let (expected, form) = parse_expected_value(id, &value)?;
            older = form;
            Ok(expected)
        })?;
        Ok(Self(case, older))
    }
}

impl<'de> Deserialize<'de> for TestCase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_test_case(deserializer, |id, value| {
            let fields = value.as_object().ok_or_else(|| format!(
                "test '{id}': `expected:` must be a mapping with a type; use assay migrate for a legacy list"
            ))?;
            if !fields.contains_key("type") {
                let keys: Vec<_> = fields.keys().collect();
                return Err(format!("test '{id}': `expected:` has no type (keys: {keys:?}); run assay migrate for legacy assertions"));
            }
            let expected: Expected = serde_json::from_value(value)
                .map_err(|err| format!("test '{id}': invalid `expected:` block: {err}. Use a tagged assertion or run assay migrate on an older config."))?;
            if expected.get_policy_path().is_some()
                || matches!(expected, Expected::Reference { .. })
            {
                return Err(format!(
                    "test '{id}': external expected policies must be inlined; run assay migrate"
                ));
            }
            let expected =
                reject_for_parse(expected).map_err(|err| format!("test '{id}': {err}"))?;
            if let Expected::ArgsValid {
                schema: Some(schema),
                ..
            } = &expected
            {
                if !super::has_structured_args_policy_shape(schema) {
                    return Err(format!(
                        "test '{id}': args_valid.schema requires a structured argument policy; run assay migrate for an older tool-name map"
                    ));
                }
            }
            Ok(expected)
        })
    }
}

assay_common::closed! {
    strict legacy_positional struct RawTestCase {
        id: String,
        input: TestInput,
        #[default]
        expected: RawExpected,
        assertions: Option<Vec<crate::agent_assertions::model::TraceAssertion>>,
        #[default]
        on_error: Option<ErrorPolicy>,
        #[default]
        tags: Vec<String>,
        metadata: Option<assay_common::doc::Opaque<assay_common::unique_value::UniqueValue>>,
    }
}

impl assay_common::doc::ClosedValue for TestCase {
    fn __shape() -> assay_common::doc::Shape {
        RawTestCase::__shape()
    }
}

assay_common::closed! {
    /// The mapping form of a test's input.
    struct TestInputFields {
        prompt: String,
        context: Option<Vec<String>>,
    }
}

/// A test's input is a prompt string or the mapping form, read through the shared grammar's
/// name-or-mapping union.
type TestInputWire = assay_common::doc::NameOr<TestInputFields>;

impl assay_common::doc::ClosedValue for TestInput {
    fn __shape() -> assay_common::doc::Shape {
        <TestInputWire as assay_common::doc::ClosedValue>::__shape()
    }
}

impl<'de> Deserialize<'de> for TestInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match TestInputWire::deserialize(deserializer)? {
            assay_common::doc::NameOr::Name(prompt) => TestInput {
                prompt,
                context: None,
            },
            assay_common::doc::NameOr::Detailed(TestInputFields { prompt, context }) => {
                TestInput { prompt, context }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referenced_schema_keeps_literal_number_marker_objects() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metric.yaml");
        std::fs::write(
            &path,
            "schema: {Search: {const: {'$serde_json::private::Number': '7'}}}",
        )
        .unwrap();
        let Expected::ArgsValid {
            schema: Some(schema),
            ..
        } = read_referenced_expected(&path).unwrap()
        else {
            panic!("expected an argument schema");
        };
        assert_eq!(
            schema["Search"]["const"]["$serde_json::private::Number"],
            "7"
        );
    }
}
