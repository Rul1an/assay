use crate::on_error::ErrorPolicy;
use serde::Deserialize;

use super::types::{Expected, TestCase, TestInput};

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
        let mut expected_main = Expected::default();
        let extra_assertions = raw.assertions.unwrap_or_default();

        if let Some(val) = raw.expected {
            if let Some(arr) = val.as_array() {
                // Legacy list format
                for (i, item) in arr.iter().enumerate() {
                    // Try to parse as Expected
                    // Try to parse as Expected (Strict V1)
                    if let Ok(exp) = serde_json::from_value::<Expected>(item.clone()) {
                        if i == 0 {
                            expected_main = exp;
                        }
                    } else if let Some(obj) = item.as_object() {
                        // Try Legacy Heuristics
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
                                    sequence: serde_json::from_value(
                                        obj.get("sequence").unwrap().clone(),
                                    )
                                    .ok(),
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

                        if let Some(p) = parsed {
                            if i == 0 {
                                expected_main = p;
                            }
                            // else: drop or move to assertions (out of scope for quick fix, primary policy is priority)
                        }
                    }
                }
            } else {
                // Try V1 single object
                if let Ok(exp) = serde_json::from_value(val.clone()) {
                    expected_main = exp;
                }
            }
        }

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
