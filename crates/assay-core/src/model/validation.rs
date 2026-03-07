use crate::on_error::ErrorPolicy;

use super::types::{EvalConfig, Expected, Settings, TestCase, TestStatus, ThresholdingConfig};

pub(crate) fn is_default_otel(o: &crate::config::otel::OtelConfig) -> bool {
    o == &crate::config::otel::OtelConfig::default()
}

pub(crate) fn is_default_thresholds(t: &crate::thresholds::ThresholdConfig) -> bool {
    t == &crate::thresholds::ThresholdConfig::default()
}

pub(crate) fn is_default_error_policy(p: &ErrorPolicy) -> bool {
    *p == ErrorPolicy::default()
}

pub(crate) fn is_default_settings(s: &Settings) -> bool {
    s == &Settings::default()
}

pub(crate) fn default_one() -> u32 {
    1
}

pub(crate) fn default_min_score() -> f64 {
    0.80
}

impl EvalConfig {
    pub fn is_legacy(&self) -> bool {
        self.version == 0
    }

    pub fn has_legacy_usage(&self) -> bool {
        self.tests
            .iter()
            .any(|t: &TestCase| t.expected.get_policy_path().is_some())
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version >= 1 {
            for test in &self.tests {
                if matches!(test.expected, Expected::Reference { .. }) {
                    anyhow::bail!("$ref in expected block is not allowed in configVersion >= 1. Run `assay migrate` to inline policies.");
                }
            }
        }
        Ok(())
    }

    /// Get the effective error policy for a test.
    /// Test-level on_error overrides suite-level settings.
    pub fn effective_error_policy(&self, test: &TestCase) -> ErrorPolicy {
        test.on_error.unwrap_or(self.settings.on_error)
    }
}

impl Expected {
    pub fn get_policy_path(&self) -> Option<&str> {
        match self {
            Expected::ArgsValid { policy, .. } => policy.as_deref(),
            Expected::SequenceValid { policy, .. } => policy.as_deref(),
            _ => None,
        }
    }

    /// Per-test thresholding for baseline regression (mode/max_drop) when this Expected variant matches the metric.
    pub fn thresholding_for_metric(&self, metric_name: &str) -> Option<&ThresholdingConfig> {
        match (metric_name, self) {
            ("semantic_similarity_to", Expected::SemanticSimilarityTo { thresholding, .. }) => {
                thresholding.as_ref()
            }
            ("faithfulness", Expected::Faithfulness { thresholding, .. }) => thresholding.as_ref(),
            ("relevance", Expected::Relevance { thresholding, .. }) => thresholding.as_ref(),
            _ => None,
        }
    }
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
            "allowed_on_error" => TestStatus::AllowedOnError,
            _ => TestStatus::Error,
        }
    }

    /// Returns true if this status should be treated as passing for CI purposes
    pub fn is_passing(&self) -> bool {
        matches!(
            self,
            TestStatus::Pass | TestStatus::AllowedOnError | TestStatus::Warn
        )
    }

    /// Returns true if this status should block CI
    pub fn is_blocking(&self) -> bool {
        matches!(self, TestStatus::Fail | TestStatus::Error)
    }
}
