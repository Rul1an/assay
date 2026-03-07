use crate::on_error::ErrorPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalConfig {
    #[serde(default, rename = "configVersion", alias = "version")]
    pub version: u32,
    pub suite: String,
    pub model: String,
    #[serde(
        default,
        skip_serializing_if = "crate::model::validation::is_default_settings"
    )]
    pub settings: Settings,
    #[serde(
        default,
        skip_serializing_if = "crate::model::validation::is_default_thresholds"
    )]
    pub thresholds: crate::thresholds::ThresholdConfig,
    #[serde(
        default,
        skip_serializing_if = "crate::model::validation::is_default_otel"
    )]
    pub otel: crate::config::otel::OtelConfig,
    pub tests: Vec<TestCase>,
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
    #[serde(
        default,
        skip_serializing_if = "crate::model::validation::is_default_error_policy"
    )]
    pub on_error: ErrorPolicy,

    /// Bail on first failure (useful for CI)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bail_on_first_failure: bool,
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

#[derive(Debug, Clone, Default, Serialize)]
pub struct TestInput {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<String>>,
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
        #[serde(
            default = "crate::model::validation::default_min_score",
            alias = "threshold"
        )]
        min_score: f64,

        #[serde(default)]
        thresholding: Option<ThresholdingConfig>,
    },
    JudgeCriteria {
        judge_criteria: serde_json::Value,
    },
    Faithfulness {
        #[serde(default = "crate::model::validation::default_min_score")]
        min_score: f64,
        rubric_version: Option<String>,
        #[serde(default)]
        thresholding: Option<ThresholdingConfig>,
    },
    Relevance {
        #[serde(default = "crate::model::validation::default_min_score")]
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
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: String,
    #[serde(default)]
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
#[serde(deny_unknown_fields)]
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
        #[serde(default = "crate::model::validation::default_one")]
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

// Helper for alias resolution
impl Policy {
    pub fn load<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let policy: Policy = serde_yaml::from_str(&content)?;
        Ok(policy)
    }

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
    #[serde(default)]
    pub reliability: crate::judge::reliability::ReliabilityConfig,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Pass,
    Fail,
    Flaky,
    Warn,
    Error,
    Skipped,
    Unstable,
    /// Action was allowed despite an upstream error (fail-open mode).
    AllowedOnError,
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
