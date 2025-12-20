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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub id: String,
    pub input: TestInput,
    pub expected: Expected,
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

    // v0.3 hooks (placeholders)
    JsonSchema {
        json_schema: String,
    },
    SemanticSimilarityTo {
        semantic_similarity_to: String,
        min_score: Option<f64>,
    },
    JudgeCriteria {
        judge_criteria: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub cached: bool,
    #[serde(default)]
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Pass,
    Fail,
    Flaky,
    Warn,
    Error,
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
