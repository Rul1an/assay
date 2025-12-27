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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestCase {
    pub id: String,
    pub input: TestInput,
    pub expected: Expected,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertions: Option<Vec<crate::agent_assertions::model::TraceAssertion>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
}

impl Default for Expected {
    fn default() -> Self {
        Expected::MustContain {
            must_contain: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SequenceRule {
    Require { tool: String },
    Before { first: String, then: String },
    Blocklist { pattern: String },
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
            _ => TestStatus::Error, // Default fallback
        }
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
