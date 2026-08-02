use crate::on_error::ErrorPolicy;

use super::types::{
    EvalConfig, Expected, SequenceRule, Settings, TestCase, TestStatus, ThresholdingConfig,
};

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

/// The field or field group that leaves `expected` without an effective check.
///
/// This is the shared definition used by parsing, serialization, and validation.
/// Keeping it here prevents those three surfaces from disagreeing about a shape
/// that would otherwise pass without evaluating any constraint.
pub(crate) fn vacuous_expected_field(e: &Expected) -> Option<&'static str> {
    match e {
        Expected::MustContain { must_contain } if must_contain.iter().all(String::is_empty) => {
            Some("must_contain")
        }
        Expected::MustNotContain { must_not_contain } if must_not_contain.is_empty() => {
            Some("must_not_contain")
        }
        Expected::RegexMatch { pattern, .. } if pattern.is_empty() => Some("pattern"),
        Expected::SemanticSimilarityTo { min_score, .. } if *min_score <= -1.0 => Some("min_score"),
        Expected::ArgsValid { policy, schema }
            if schema.as_ref().is_some_and(schema_map_asserts_nothing)
                || (policy.is_none() && schema.is_none()) =>
        {
            Some("policy/schema")
        }
        Expected::SequenceValid {
            policy,
            sequence,
            rules,
        } if policy.is_none()
            && sequence.as_ref().is_none_or(Vec::is_empty)
            && rules.as_ref().is_none_or(Vec::is_empty) =>
        {
            Some("policy/sequence/rules")
        }
        Expected::ToolOutputValid { schemas }
            if schemas.as_ref().is_none_or(schema_map_asserts_nothing) =>
        {
            Some("schemas")
        }
        Expected::ToolBlocklist { blocked } if blocked.is_empty() => Some("blocked"),
        _ => None,
    }
}

fn schema_map_asserts_nothing(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|schemas| {
        schemas.values().all(|schema| {
            schema == &serde_json::Value::Bool(true)
                || schema.as_object().is_some_and(serde_json::Map::is_empty)
        })
    })
}

/// Explain an `Expected` shape that the current metric set cannot execute as written.
pub(crate) fn non_executable_expected_reason(e: &Expected) -> Option<&'static str> {
    match e {
        Expected::JudgeCriteria { .. } => Some("judge_criteria has no registered evaluator"),
        Expected::SequenceValid {
            rules: Some(rules), ..
        } => rules.iter().find_map(|rule| match rule {
            SequenceRule::Require { .. }
            | SequenceRule::Blocklist { .. }
            | SequenceRule::Before { .. } => None,
            SequenceRule::Eventually { .. } => {
                Some("sequence rule eventually is not executable by sequence_valid")
            }
            SequenceRule::MaxCalls { .. } => {
                Some("sequence rule max_calls is not executable by sequence_valid")
            }
            SequenceRule::After { .. } => {
                Some("sequence rule after is not executable by sequence_valid")
            }
            SequenceRule::NeverAfter { .. } => {
                Some("sequence rule never_after is not executable by sequence_valid")
            }
            SequenceRule::Sequence { .. } => {
                Some("sequence rule sequence is not executable by sequence_valid")
            }
        }),
        _ => None,
    }
}

pub(crate) fn ineffective_expected_reason(e: &Expected) -> Option<&'static str> {
    match e {
        Expected::SequenceValid {
            rules: Some(rules), ..
        } if rules.iter().any(|rule| {
            matches!(
                rule,
                SequenceRule::Before { first, then } if first == then
            )
        }) =>
        {
            Some("a before rule with identical tools cannot constrain a trace")
        }
        _ => None,
    }
}

/// Reject any expected value that cannot safely reach metric dispatch.
pub(crate) fn validate_expected_for_execution(e: &Expected) -> anyhow::Result<()> {
    if matches!(e, Expected::Reference { .. }) {
        anyhow::bail!("unresolved `$ref` cannot be executed; resolve or migrate it first");
    }
    if let Some(field) = vacuous_expected_field(e) {
        anyhow::bail!("`{field}` asserts nothing");
    }
    if let Some(reason) = non_executable_expected_reason(e) {
        anyhow::bail!("expected block is not executable: {reason}");
    }
    if let Some(reason) = ineffective_expected_reason(e) {
        anyhow::bail!("{reason}");
    }
    Ok(())
}

/// Validate the execution contract while preserving omitted-`expected` compatibility.
pub(crate) fn validate_test_case_for_execution(test: &TestCase) -> anyhow::Result<()> {
    // Deserialization represents an omitted `expected:` key with this exact default.
    // A written empty block never reaches here because the parser rejects it. Keep
    // the historical warning-only behavior until the assertion contract is tightened.
    if matches!(
        &test.expected,
        Expected::MustContain { must_contain } if must_contain.is_empty()
    ) {
        return Ok(());
    }
    validate_expected_for_execution(&test.expected)
}

/// True when `expected` asserts nothing.
///
/// Used to skip serializing the vacuous default. Without this, a writer such as
/// `assay migrate` would materialise an assertion that the parser rejects.
pub(crate) fn is_vacuous_expected(e: &Expected) -> bool {
    vacuous_expected_field(e).is_some()
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
