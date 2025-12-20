use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThresholdConfig {
    pub min_score: Option<f64>,
    pub block_on_warn: Option<bool>,
}
