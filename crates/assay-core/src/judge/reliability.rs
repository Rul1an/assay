use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityConfig {
    /// Minimum score to be considered "borderline" (inclusive). Default 0.4.
    pub borderline_min: f64,
    /// Maximum score to be considered "borderline" (inclusive). Default 0.6.
    pub borderline_max: f64,
    /// If true, borderline results trigger a rerun (if enabled in runner).
    pub retry_borderline: bool,
    /// Strategies: "majority_vote", "consensus", "all_pass"
    pub strategy: String,
}

impl Default for ReliabilityConfig {
    fn default() -> Self {
        Self {
            borderline_min: 0.4,
            borderline_max: 0.6,
            retry_borderline: true,
            strategy: "majority_vote".to_string(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum VerdictStatus {
    Pass,
    Fail,
    Uncertain,
}

impl ReliabilityConfig {
    pub fn assess(&self, score: f64) -> VerdictStatus {
        if score >= self.borderline_min && score <= self.borderline_max {
            VerdictStatus::Uncertain
        } else if score > self.borderline_max {
            VerdictStatus::Pass
        } else {
            VerdictStatus::Fail
        }
    }
}
