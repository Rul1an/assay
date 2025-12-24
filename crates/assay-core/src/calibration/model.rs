use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationReport {
    pub schema_version: u32,
    pub source: String,
    pub generated_at: String,
    pub metrics: Vec<MetricSummary>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct MetricKey {
    pub metric: String,
    /// Optional granularity: if set, these stats apply only to this specific test_id.
    /// If None, stats are aggregated across all tests (global metric performance).
    pub test_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSummary {
    pub key: MetricKey,
    /// Number of data points (runs) included
    pub n: u32,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub std: f64,
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,

    /// The recommended threshold for "pass" (usually p10 or target_tail)
    pub recommended_min_score: f64,

    /// For relative gating: maximum allowed drop from baseline (p50 - p10 logic)
    pub recommended_max_drop: f64,
}
