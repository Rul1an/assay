use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ConsumerResult {
    pub vector_id: String,
    pub condition: String,
    pub realism_class: String,
    pub canonical_classification: String,
    pub consumer_classification: String,
    pub downgrade_occurred: bool,
    pub outcome: ConsumerOutcome,
    pub hypothesis_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerOutcome {
    NoEffect,
    RetainedNoDowngrade,
    DowngradeWithCorrectDetection,
    SilentDowngrade,
    SilentTrustUpgrade,
}
