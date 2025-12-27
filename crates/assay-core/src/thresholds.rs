use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThresholdConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_on_warn: Option<bool>,
}
