pub mod report;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Baseline {
    pub schema_version: u32,
    pub suite: String,
    pub assay_version: String,
    pub created_at: String,
    pub config_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_info: Option<GitInfo>,
    pub entries: Vec<BaselineEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitInfo {
    pub commit: String,
    pub branch: Option<String>,
    pub dirty: bool,
    pub author: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BaselineEntry {
    pub test_id: String,
    pub metric: String,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineDiff {
    pub regressions: Vec<Regression>,
    pub improvements: Vec<Improvement>,
    pub new_tests: Vec<String>,
    pub missing_tests: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Regression {
    pub test_id: String,
    pub metric: String,
    pub baseline_score: f64,
    pub candidate_score: f64,
    pub delta: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Improvement {
    pub test_id: String,
    pub metric: String,
    pub baseline_score: f64,
    pub candidate_score: f64,
    pub delta: f64,
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

        // Note: Suite mismatch and assay version checks are handled in `validate()` to separate structural loading from semantic validation.

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
        if self.assay_version != current_ver {
            eprintln!(
                "warning: baseline generated with assay v{} (current: v{})",
                self.assay_version, current_ver
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

        // Create a sorted clone for deterministic output
        let mut sorted = self.clone();
        sorted.entries.sort_by(|a, b| {
            a.test_id
                .cmp(&b.test_id)
                .then_with(|| a.metric.cmp(&b.metric))
        });

        // Use pretty print for git diffability
        serde_json::to_writer_pretty(file, &sorted).context("failed to write baseline JSON")?;
        Ok(())
    }

    // Helper to get score for a test+metric
    pub fn get_score(&self, test_id: &str, metric: &str) -> Option<f64> {
        self.entries
            .iter()
            .find(|e| e.test_id == test_id && e.metric == metric)
            .map(|e| e.score)
    }

    /// Whether the baseline run actually evaluated this metric.
    ///
    /// `None` means the baseline predates the `exercised` dimension (#1949 layer 2) and says
    /// nothing either way — which is not the same as saying the metric was not exercised, and is
    /// why the coverage check treats it as no evidence rather than as a drop.
    pub fn was_exercised(&self, test_id: &str, metric: &str) -> Option<bool> {
        self.entries
            .iter()
            .find(|e| e.test_id == test_id && e.metric == metric)
            .and_then(|e| e.meta.as_ref())
            .and_then(|m| m.get("exercised"))
            .and_then(|v| v.as_str())
            .map(|s| s == "exercised")
    }

    pub fn diff(&self, candidate: &Baseline) -> BaselineDiff {
        let mut regressions = Vec::new();
        let mut improvements = Vec::new();
        let mut new_tests = Vec::new();
        let mut missing_tests = Vec::new();

        // Map baseline entries by (test_id, metric) for quick lookup
        let mut baseline_map = std::collections::HashMap::new();
        for entry in &self.entries {
            baseline_map.insert((entry.test_id.clone(), entry.metric.clone()), entry.score);
        }

        let mut candidate_seen = std::collections::HashSet::new();

        for entry in &candidate.entries {
            candidate_seen.insert((entry.test_id.clone(), entry.metric.clone()));

            if let Some(baseline_score) =
                baseline_map.get(&(entry.test_id.clone(), entry.metric.clone()))
            {
                let delta = entry.score - baseline_score;
                // Floating point comparison with epsilon?
                // For now, exact logic, but maybe ignore tiny deltas.
                if delta < -0.000001 {
                    regressions.push(Regression {
                        test_id: entry.test_id.clone(),
                        metric: entry.metric.clone(),
                        baseline_score: *baseline_score,
                        candidate_score: entry.score,
                        delta,
                    });
                } else if delta > 0.000001 {
                    improvements.push(Improvement {
                        test_id: entry.test_id.clone(),
                        metric: entry.metric.clone(),
                        baseline_score: *baseline_score,
                        candidate_score: entry.score,
                        delta,
                    });
                }
            } else {
                new_tests.push(format!("{} (metric: {})", entry.test_id, entry.metric));
            }
        }

        // Identify missing
        for (test_id, metric) in baseline_map.keys() {
            if !candidate_seen.contains(&(test_id.clone(), metric.clone())) {
                missing_tests.push(format!("{} (metric: {})", test_id, metric));
            }
        }

        // Sort results for stability
        regressions.sort_by(|a, b| a.test_id.cmp(&b.test_id).then(a.metric.cmp(&b.metric)));
        improvements.sort_by(|a, b| a.test_id.cmp(&b.test_id).then(a.metric.cmp(&b.metric)));
        new_tests.sort();
        missing_tests.sort();

        BaselineDiff {
            regressions,
            improvements,
            new_tests,
            missing_tests,
        }
    }

    pub fn from_coverage_report(
        report: &crate::coverage::CoverageReport,
        suite: String,
        config_fingerprint: String,
        git_info: Option<GitInfo>,
    ) -> Self {
        let entries = vec![
            BaselineEntry {
                test_id: "coverage".to_string(),
                metric: "overall".to_string(),
                score: report.overall_coverage_pct,
                meta: None,
            },
            BaselineEntry {
                test_id: "coverage".to_string(),
                metric: "tool".to_string(),
                score: report.tool_coverage.coverage_pct,
                meta: None,
            },
            BaselineEntry {
                test_id: "coverage".to_string(),
                metric: "rule".to_string(),
                score: report.rule_coverage.coverage_pct,
                meta: None,
            },
        ];

        Self {
            schema_version: 1,
            suite,
            assay_version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            config_fingerprint,
            git_info,
            entries,
        }
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

#[cfg(test)]
mod was_exercised_tests {
    use super::*;

    fn baseline_with(meta: Option<serde_json::Value>) -> Baseline {
        Baseline {
            schema_version: 1,
            suite: "s".into(),
            assay_version: "test".into(),
            created_at: "2026-08-06T00:00:00Z".into(),
            config_fingerprint: "fp".into(),
            git_info: None,
            entries: vec![BaselineEntry {
                test_id: "t1".into(),
                metric: "semantic".into(),
                score: 0.87,
                meta,
            }],
        }
    }

    #[test]
    fn an_exercised_baseline_entry_reads_as_exercised() {
        let b = baseline_with(Some(serde_json::json!({"exercised": "exercised"})));
        assert_eq!(b.was_exercised("t1", "semantic"), Some(true));
    }

    #[test]
    fn a_not_applicable_baseline_entry_reads_as_not_exercised() {
        let b = baseline_with(Some(serde_json::json!({"exercised": "not_applicable"})));
        assert_eq!(b.was_exercised("t1", "semantic"), Some(false));
    }

    /// A baseline written before the dimension existed says nothing either way.
    ///
    /// `None` rather than `Some(false)` is the whole point: treating silence as "was not exercised"
    /// would make every pre-existing baseline look like it had no coverage, and the coverage check
    /// would then never fire because there is nothing to have lost.
    #[test]
    fn a_baseline_predating_the_dimension_says_nothing() {
        assert_eq!(baseline_with(None).was_exercised("t1", "semantic"), None);
        let b = baseline_with(Some(serde_json::json!({"other": "field"})));
        assert_eq!(b.was_exercised("t1", "semantic"), None);
    }

    #[test]
    fn an_absent_entry_says_nothing() {
        let b = baseline_with(Some(serde_json::json!({"exercised": "exercised"})));
        assert_eq!(b.was_exercised("t1", "nosuch"), None);
        assert_eq!(b.was_exercised("nosuch", "semantic"), None);
    }
}
