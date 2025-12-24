use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalConfig {
    pub version: u32,
    pub suite: String,
    pub model: String,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub thresholds: crate::thresholds::ThresholdConfig,
    pub tests: Vec<TestCase>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub parallel: Option<usize>,
    pub timeout_seconds: Option<u64>,
    pub cache: Option<bool>,
    pub seed: Option<u64>,
    pub judge: Option<JudgeConfig>,
    pub thresholding: Option<ThresholdingSettings>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThresholdingSettings {
    pub mode: Option<String>,
    pub max_drop: Option<f64>,
    pub min_floor: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub id: String,
    pub input: TestInput,
    pub expected: Expected,
    #[serde(default)]
    pub assertions: Option<Vec<crate::agent_assertions::model::TraceAssertion>>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestInput {
    pub prompt: String,
    #[serde(default)]
    pub context: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Expected {
    MustContain {
        must_contain: Vec<String>,
    },
    MustNotContain {
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

    // v0.3 hooks
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThresholdingConfig {
    pub max_drop: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    // v0.3.0
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
    // v0.3.0
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
