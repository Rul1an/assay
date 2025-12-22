pub mod report;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Baseline {
    pub schema_version: u32,
    pub suite: String,
    pub verdict_version: String,
    pub created_at: String,
    pub config_fingerprint: String,
    pub entries: Vec<BaselineEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BaselineEntry {
    pub test_id: String,
    pub metric: String,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

impl Baseline {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)
            .with_context(|| format!("failed to open baseline file: {}", path.display()))?;
        let baseline: Baseline =
            serde_json::from_reader(file).context("failed to parse baseline JSON")?;

        if baseline.schema_version != 1 {
            anyhow::bail!(
                "config error: unsupported baseline schema version {}",
                baseline.schema_version
            );
        }

        // Note: Suite mismatch and verdict version checks are handled in `validate()` to separate structural loading from semantic validation.

        Ok(baseline)
    }

    pub fn validate(&self, current_suite: &str, current_fingerprint: &str) -> Result<()> {
        if self.suite != current_suite {
            anyhow::bail!(
                "config error: baseline suite mismatch (expected '{}', found '{}')",
                current_suite,
                self.suite
            );
        }

        let current_ver = env!("CARGO_PKG_VERSION");
        if self.verdict_version != current_ver {
            eprintln!(
                "warning: baseline generated with verdict v{} (current: v{})",
                self.verdict_version, current_ver
            );
        }

        if self.config_fingerprint != current_fingerprint {
            eprintln!(
                "warning: config fingerprint mismatch (baseline config differs from current runtime config).\n\
                 hint: run with --export-baseline to update the baseline if config changes are intentional."
            );
        }

        Ok(())
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(path)
            .with_context(|| format!("failed to create baseline file: {}", path.display()))?;
        // Use pretty print for git diffability
        serde_json::to_writer_pretty(file, self).context("failed to write baseline JSON")?;
        Ok(())
    }

    // Helper to get score for a test+metric
    pub fn get_score(&self, test_id: &str, metric: &str) -> Option<f64> {
        self.entries
            .iter()
            .find(|e| e.test_id == test_id && e.metric == metric)
            .map(|e| e.score)
    }
}

// Fingerprint logic could go here or in util
pub fn compute_config_fingerprint(config_path: &Path) -> String {
    // For MVP, just hash the config file content if it exists.
    // In future, canonicalize logic.
    if let Ok(content) = std::fs::read(config_path) {
        let digest = md5::compute(content);
        format!("md5:{:x}", digest)
    } else {
        "md5:unknown".to_string()
    }
}
