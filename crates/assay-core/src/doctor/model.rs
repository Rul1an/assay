use serde::{Deserialize, Serialize};

use crate::errors::diagnostic::Diagnostic;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub schema_version: u32,   // 1
    pub generated_at: String,  // rfc3339
    pub assay_version: String, // e.g. "0.3.4"
    pub platform: PlatformInfo,

    pub inputs: DoctorInputs,
    pub config: Option<ConfigSummary>,
    pub trace: Option<TraceSummary>,
    pub baseline: Option<BaselineSummary>,
    pub db: Option<DbSummary>,
    pub caches: CacheSummary,

    pub diagnostics: Vec<Diagnostic>, // from validate + local checks
    pub suggested_actions: Vec<SuggestedAction>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorInputs {
    pub config_path: String,
    pub trace_file: Option<String>,
    pub baseline_file: Option<String>,
    pub db_path: Option<String>,
    pub replay_strict: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSummary {
    pub suite: String,
    pub model: String,
    pub test_count: u32,
    pub metric_counts: std::collections::BTreeMap<String, u32>,
    pub thresholding_mode: Option<String>,
    pub max_drop: Option<f64>,
    pub min_floor: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSummary {
    pub path: String,
    pub entries: u64,
    pub schema_version: Option<u32>,
    pub has_assay_meta: bool,
    pub coverage: TraceCoverage,
    pub approx_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceCoverage {
    pub has_embeddings: bool,
    pub has_judge_faithfulness: bool,
    pub has_judge_relevance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineSummary {
    pub path: String,
    pub suite: String,
    pub schema_version: u32,
    pub assay_version: Option<String>,
    pub entry_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSummary {
    pub path: String,
    pub size_bytes: Option<u64>,
    pub runs: Option<u64>,
    pub results: Option<u64>,
    pub last_run_id: Option<i64>,
    pub last_run_started_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheSummary {
    pub assay_cache_dir: Option<String>,
    pub assay_embeddings_dir: Option<String>,
    pub cache_size_bytes: Option<u64>,
    pub embeddings_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedAction {
    pub title: String,      // "Fix trace miss"
    pub relates_to: String, // "failure_mode_1_trace_miss"
    pub why: String,
    pub steps: Vec<String>, // copy/paste commands
}
