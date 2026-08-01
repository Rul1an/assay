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
        Ok(exp) => return Ok(exp),
        Err(e) => e,
    };

    let Some(obj) = item.as_object() else {
        return Err(format!(
            "`expected:` must be a mapping (or a list of one mapping), found {}",
            value_kind(item)
        ));
    };

    // A block that carries `type:` is asking for the tagged form; report why the
    // tagged parse failed rather than the generic "unrecognized keys" message.
    if obj.contains_key("type") {
        return Err(format!("invalid `expected:` block: {}", strict_err));
    }

    // 2. Legacy heuristics.
    let mut parsed = None;
    let mut matched_keys = Vec::new();

    if let Some(r) = obj.get("$ref") {
        parsed = Some(Expected::Reference {
            path: r.as_str().unwrap_or("").to_string(),
        });
        matched_keys.push("$ref");
    }

    // Don't chain else-ifs, check all to detect ambiguity
    if let Some(mc) = obj.get("must_contain") {
        let val = if mc.is_string() {
            vec![mc.as_str().unwrap().to_string()]
        } else {
            serde_json::from_value(mc.clone()).unwrap_or_default()
        };
        // Last match wins for parsed, but we warn below
        if parsed.is_none() {
            parsed = Some(Expected::MustContain { must_contain: val });
        }
        matched_keys.push("must_contain");
    }

    if obj.get("sequence").is_some() {
        if parsed.is_none() {
            parsed = Some(Expected::SequenceValid {
                policy: None,
                sequence: serde_json::from_value(obj.get("sequence").unwrap().clone()).ok(),
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
        eprintln!(
            "WARN: Ambiguous legacy expected block. Found keys: {:?}. Using first match.",
            matched_keys
        );
    }

    parsed.ok_or_else(|| {
        let found: Vec<&str> = obj.keys().map(String::as_str).collect();
        format!(
            "unrecognized `expected:` block, found key(s) {:?}. Use the tagged form \
             (e.g. `type: must_contain` with `must_contain: [...]`) or one of the legacy \
             keys {:?}",
            found, LEGACY_EXPECTED_KEYS
        )
    })
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
        // `E_CFG_VACUOUS_EXPECTED` rule in `assay validate` reports when the test has
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
