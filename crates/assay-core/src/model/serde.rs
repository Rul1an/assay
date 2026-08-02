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

/// Parse a single `expected:` entry into an [`Expected`].
///
/// Resolution order is strict V1 (`type:`-tagged) first, then the frozen legacy
/// heuristics in [`LEGACY_EXPECTED_KEYS`].
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
fn parse_expected_entry(item: &serde_json::Value) -> Result<Expected, String> {
    // 1. Strict V1 (tagged).
    let strict_err = match serde_json::from_value::<Expected>(item.clone()) {
        Ok(exp) => return reject_vacuous(exp),
        Err(e) => e,
    };

    let Some(obj) = item.as_object() else {
        return Err(format!(
            "`expected:` must be a mapping (or a list of one mapping), found {}",
            value_kind(item)
        ));
    };

    // 2. Legacy heuristics.
    //
    // These also recognize two tagged compatibility forms: a scalar value for
    // `type: must_contain`, and the historical `type: sequence`. A failed tagged
    // parse may not fall back through an unrelated legacy key, because that would
    // silently change the metric the author selected.
    let mut parsed = None;
    let mut matched_keys = Vec::new();

    if let Some(r) = obj.get("$ref") {
        let path = r
            .as_str()
            .ok_or_else(|| format!("`$ref` must be a string, found {}", value_kind(r)))?;
        parsed = Some(Expected::Reference {
            path: path.to_string(),
        });
        matched_keys.push("$ref");
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
        matched_keys.push("must_contain");
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
        matched_keys.push("sequence");
    }

    if obj.get("schema").is_some() {
        if parsed.is_none() {
            parsed = Some(Expected::ArgsValid {
                policy: None,
                schema: obj.get("schema").cloned(),
            });
        }
        matched_keys.push("schema");
    }

    if matched_keys.len() > 1 {
        return Err(format!(
            "ambiguous legacy `expected:` block contains multiple assertions {:?}; \
             use one tagged assertion or move additional checks to `assertions:`",
            matched_keys
        ));
    }

    if let Some(p) = parsed {
        if let Some(tag) = obj.get("type") {
            let compatible_legacy_form = tag.as_str().is_some_and(|tag| {
                matches!(
                    (tag, matched_keys[0]),
                    ("must_contain", "must_contain") | ("sequence", "sequence")
                )
            });
            if !compatible_legacy_form {
                return Err(format!("invalid `expected:` block: {}", strict_err));
            }
        }
        return reject_vacuous(p);
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
    let field = match &exp {
        Expected::MustContain { must_contain } if must_contain.is_empty() => "must_contain",
        Expected::MustNotContain { must_not_contain } if must_not_contain.is_empty() => {
            "must_not_contain"
        }
        _ => return Ok(exp),
    };

    Err(format!(
        "`{}` is empty, so this test would pass for any response. \
         Give it at least one entry, or remove the `expected:` block and put the \
         test's checks in `assertions:`.",
        field
    ))
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
fn parse_expected_value(test_id: &str, val: &serde_json::Value) -> Result<Expected, String> {
    let Some(arr) = val.as_array() else {
        return parse_expected_entry(val).map_err(|e| format!("test '{}': {}", test_id, e));
    };

    match arr.len() {
        0 => Err(format!(
            "test '{}': `expected:` is an empty list, which asserts nothing. \
             Remove the key or give it an assertion.",
            test_id
        )),
        1 => parse_expected_entry(&arr[0])
            .map_err(|e| format!("test '{}': `expected:` entry 0 is invalid: {}", test_id, e)),
        n => Err(format!(
            "test '{}': `expected:` has {} entries but only one is supported \
             (earlier versions silently dropped all but the first). \
             Split them into separate tests, or move the extra checks to `assertions:`.",
            test_id, n
        )),
    }
}

impl<'de> Deserialize<'de> for TestCase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawTestCase {
            id: String,
            input: TestInput,
            #[serde(default)]
            expected: Option<serde_json::Value>,
            assertions: Option<Vec<crate::agent_assertions::model::TraceAssertion>>,
            #[serde(default)]
            on_error: Option<ErrorPolicy>,
            #[serde(default)]
            tags: Vec<String>,
            metadata: Option<serde_json::Value>,
        }

        let raw = RawTestCase::deserialize(deserializer)?;
        let extra_assertions = raw.assertions.unwrap_or_default();

        // A missing `expected:` key stays permissive: a test may carry its checks in
        // `assertions:` instead. It resolves to the vacuous default, which the
        // `W_CFG_VACUOUS_EXPECTED` rule in `assay validate` reports when the test has
        // no assertions either. A present-but-unparsable key is a different matter and
        // is a hard error below.
        let expected_main = match &raw.expected {
            Some(val) => parse_expected_value(&raw.id, val).map_err(D::Error::custom)?,
            None => Expected::default(),
        };

        Ok(TestCase {
            id: raw.id,
            input: raw.input,
            expected: expected_main,
            assertions: if extra_assertions.is_empty() {
                None
            } else {
                Some(extra_assertions)
            },
            on_error: raw.on_error,
            tags: raw.tags,
            metadata: raw.metadata,
        })
    }
}

impl<'de> Deserialize<'de> for TestInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TestInputVisitor;

        impl<'de> serde::de::Visitor<'de> for TestInputVisitor {
            type Value = TestInput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("string or struct TestInput")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(TestInput {
                    prompt: value.to_owned(),
                    context: None,
                })
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                // Default derivation logic manually implemented or use intermediate struct
                // Using intermediate struct is easier to avoid massive boilerplate
                #[derive(Deserialize)]
                struct Helper {
                    prompt: String,
                    #[serde(default)]
                    context: Option<Vec<String>>,
                }
                let helper =
                    Helper::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(TestInput {
                    prompt: helper.prompt,
                    context: helper.context,
                })
            }
        }

        deserializer.deserialize_any(TestInputVisitor)
    }
}
