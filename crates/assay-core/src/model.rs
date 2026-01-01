use crate::on_error::ErrorPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalConfig {
    #[serde(default, rename = "configVersion", alias = "version")]
    pub version: u32,
    pub suite: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "is_default_settings")]
    pub settings: Settings,
    #[serde(default, skip_serializing_if = "is_default_thresholds")]
    pub thresholds: crate::thresholds::ThresholdConfig,
    pub tests: Vec<TestCase>,
}

impl EvalConfig {
    pub fn is_legacy(&self) -> bool {
        self.version == 0
    }

    pub fn has_legacy_usage(&self) -> bool {
        self.tests
            .iter()
            .any(|t| t.expected.get_policy_path().is_some())
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version >= 1 {
            for test in &self.tests {
                if matches!(test.expected, Expected::Reference { .. }) {
                    anyhow::bail!("$ref in expected block is not allowed in configVersion >= 1. Run `assay migrate` to inline policies.");
                }
            }
        }
        Ok(())
    }

    /// Get the effective error policy for a test.
    /// Test-level on_error overrides suite-level settings.
    pub fn effective_error_policy(&self, test: &TestCase) -> ErrorPolicy {
        test.on_error.unwrap_or(self.settings.on_error)
    }
}

fn is_default_thresholds(t: &crate::thresholds::ThresholdConfig) -> bool {
    t == &crate::thresholds::ThresholdConfig::default()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge: Option<JudgeConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thresholding: Option<ThresholdingSettings>,

    /// Global error handling policy (default: block)
    /// Can be overridden per-test
    #[serde(default, skip_serializing_if = "is_default_error_policy")]
    pub on_error: ErrorPolicy,

    /// Bail on first failure (useful for CI)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bail_on_first_failure: bool,
}

fn is_default_error_policy(p: &ErrorPolicy) -> bool {
    *p == ErrorPolicy::default()
}

fn is_default_settings(s: &Settings) -> bool {
    s == &Settings::default()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThresholdingSettings {
    pub mode: Option<String>,
    pub max_drop: Option<f64>,
    pub min_floor: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TestCase {
    pub id: String,
    pub input: TestInput,
    pub expected: Expected,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertions: Option<Vec<crate::agent_assertions::model::TraceAssertion>>,
    /// Per-test error handling policy override
    /// If None, uses settings.on_error
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<ErrorPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl<'de> Deserialize<'de> for TestCase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
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
                            eprintln!("WARN: Ambiguous legacy expected block. Found keys: {:?}. Using first match.", matched_keys);
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

#[derive(Debug, Clone, Default, Serialize)]
pub struct TestInput {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<String>>,
}

impl<'de> Deserialize<'de> for TestInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TestInputVisitor;

        impl<'de> serde::de::Visitor<'de> for TestInputVisitor {
            type Value = TestInput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Expected {
    MustContain {
        #[serde(default)]
        must_contain: Vec<String>,
    },
    MustNotContain {
        #[serde(default)]
        must_not_contain: Vec<String>,
    },

    RegexMatch {
        pattern: String,
        #[serde(default)]
        flags: Vec<String>,
    },
    RegexNotMatch {
        pattern: String,
        #[serde(default)]
        flags: Vec<String>,
    },

    JsonSchema {
        json_schema: String,
        #[serde(default)]
        schema_file: Option<String>,
    },
    SemanticSimilarityTo {
        // canonical field
        #[serde(alias = "text")]
        semantic_similarity_to: String,

        // canonical field
        #[serde(default = "default_min_score", alias = "threshold")]
        min_score: f64,

        #[serde(default)]
        thresholding: Option<ThresholdingConfig>,
    },
    JudgeCriteria {
        judge_criteria: serde_json::Value,
    },
    Faithfulness {
        #[serde(default = "default_min_score")]
        min_score: f64,
        rubric_version: Option<String>,
        #[serde(default)]
        thresholding: Option<ThresholdingConfig>,
    },
    Relevance {
        #[serde(default = "default_min_score")]
        min_score: f64,
        rubric_version: Option<String>,
        #[serde(default)]
        thresholding: Option<ThresholdingConfig>,
    },

    ArgsValid {
        #[serde(skip_serializing_if = "Option::is_none")]
        policy: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema: Option<serde_json::Value>,
    },
    SequenceValid {
        #[serde(skip_serializing_if = "Option::is_none")]
        policy: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sequence: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rules: Option<Vec<SequenceRule>>,
    },
    ToolBlocklist {
        blocked: Vec<String>,
    },
    // For migration/legacy support
    #[serde(rename = "$ref")]
    Reference {
        path: String,
    },
}

impl Default for Expected {
    fn default() -> Self {
        Expected::MustContain {
            must_contain: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub tools: ToolsPolicy,
    #[serde(default)]
    pub sequences: Vec<SequenceRule>,
    #[serde(default)]
    pub aliases: std::collections::HashMap<String, Vec<String>>,
    #[serde(default)]
    pub on_error: ErrorPolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsPolicy {
    #[serde(default)]
    pub allow: Option<Vec<String>>,
    #[serde(default)]
    pub deny: Option<Vec<String>>,
    #[serde(default)]
    pub require_args: Option<std::collections::HashMap<String, Vec<String>>>,
    #[serde(default)]
    pub arg_constraints: Option<
        std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
    >,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SequenceRule {
    Require {
        tool: String,
    },
    Eventually {
        tool: String,
        within: u32,
    },
    MaxCalls {
        tool: String,
        max: u32,
    },
    Before {
        first: String,
        then: String,
    },
    After {
        trigger: String,
        then: String,
        #[serde(default = "default_one")]
        within: u32,
    },
    NeverAfter {
        trigger: String,
        forbidden: String,
    },
    Sequence {
        tools: Vec<String>,
        #[serde(default)]
        strict: bool,
    },
    Blocklist {
        pattern: String,
    },
}

fn default_one() -> u32 {
    1
}

// Helper for alias resolution
impl Policy {
    pub fn resolve_alias(&self, tool_name: &str) -> Vec<String> {
        if let Some(members) = self.aliases.get(tool_name) {
            members.clone()
        } else {
            // If not an alias, return strict singleton if no alias found?
            // RFC says: "Matches SearchKnowledgeBase OR SearchWeb".
            // "Alias can be used anywhere a tool name is expected".
            // If we rely on resolve_alias to return all matches for a "rule target",
            // AND we want to support literals:
            // If 'Search' is in aliases, satisfy if match any alias member.
            // If 'Search' is NOT in aliases, it's a literal.
            vec![tool_name.to_string()]
        }
    }
}

impl Expected {
    pub fn get_policy_path(&self) -> Option<&str> {
        match self {
            Expected::ArgsValid { policy, .. } => policy.as_deref(),
            Expected::SequenceValid { policy, .. } => policy.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
    pub index: usize,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThresholdingConfig {
    pub max_drop: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JudgeConfig {
    pub rubric_version: Option<String>,
    pub samples: Option<u32>,
}

fn default_min_score() -> f64 {
    0.80
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub cached: bool,
    #[serde(default)]
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Pass,
    Fail,
    Flaky,
    Warn,
    Error,
    Skipped,
    Unstable,
    /// New: Action was allowed despite error (fail-open mode)
    AllowedOnError,
}

impl TestStatus {
    pub fn parse(s: &str) -> Self {
        match s {
            "pass" => TestStatus::Pass,
            "fail" => TestStatus::Fail,
            "flaky" => TestStatus::Flaky,
            "warn" => TestStatus::Warn,
            "error" => TestStatus::Error,
            "skipped" => TestStatus::Skipped,
            "unstable" => TestStatus::Unstable,
            "allowed_on_error" => TestStatus::AllowedOnError,
            _ => TestStatus::Error,
        }
    }

    /// Returns true if this status should be treated as passing for CI purposes
    pub fn is_passing(&self) -> bool {
        matches!(
            self,
            TestStatus::Pass | TestStatus::AllowedOnError | TestStatus::Warn
        )
    }

    /// Returns true if this status should block CI
    pub fn is_blocking(&self) -> bool {
        matches!(self, TestStatus::Fail | TestStatus::Error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResultRow {
    pub test_id: String,
    pub status: TestStatus,
    pub score: Option<f64>,
    pub cached: bool,
    pub message: String,
    #[serde(default)]
    pub details: serde_json::Value,
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub skip_reason: Option<String>,
    #[serde(default)]
    pub attempts: Option<Vec<AttemptRow>>,
    /// Error policy that was applied (if error occurred)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_policy_applied: Option<ErrorPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptRow {
    pub attempt_no: u32,
    pub status: TestStatus,
    pub message: String,
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub details: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_input_deserialize() {
        let yaml = r#"
            id: test1
            input: "simple string"
            expected:
              type: must_contain
              must_contain: ["foo"]
        "#;
        let tc: TestCase = serde_yaml::from_str(yaml).expect("failed to parse");
        assert_eq!(tc.input.prompt, "simple string");
    }

    #[test]
    fn test_legacy_list_expected() {
        let yaml = r#"
            id: test1
            input: "test"
            expected:
              - must_contain: "Paris"
              - must_not_contain: "London"
        "#;
        let tc: TestCase = serde_yaml::from_str(yaml).expect("failed to parse");
        if let Expected::MustContain { must_contain } = tc.expected {
            assert_eq!(must_contain, vec!["Paris"]);
        } else {
            panic!("Expected MustContain, got {:?}", tc.expected);
        }
    }

    #[test]
    fn test_scalar_must_contain_promotion() {
        let yaml = r#"
            id: test1
            input: "test"
            expected:
              - must_contain: "single value"
        "#;
        let tc: TestCase = serde_yaml::from_str(yaml).unwrap();
        if let Expected::MustContain { must_contain } = tc.expected {
            assert_eq!(must_contain, vec!["single value"]);
        } else {
            panic!("Expected MustContain");
        }
    }

    #[test]
    fn test_validate_ref_in_v1() {
        let config = EvalConfig {
            version: 1,
            suite: "test".into(),
            model: "test".into(),
            settings: Settings::default(),
            thresholds: Default::default(),
            tests: vec![TestCase {
                id: "t1".into(),
                input: TestInput {
                    prompt: "hi".into(),
                    context: None,
                },
                expected: Expected::Reference {
                    path: "foo.yaml".into(),
                },
                assertions: None,
                tags: vec![],
                metadata: None,
                on_error: None,
            }],
        };
        assert!(config.validate().is_err());
    }
}
