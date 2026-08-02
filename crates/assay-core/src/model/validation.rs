use crate::on_error::ErrorPolicy;

use super::types::{
    EvalConfig, Expected, SequenceRule, Settings, TestCase, TestStatus, ThresholdingConfig,
};

/// Tolerance used by the semantic-similarity evaluator at its pass boundary.
pub const SEMANTIC_SIMILARITY_EPSILON: f64 = 1e-6;

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
/// Parsing and execution share this predicate because both must reject an
/// explicitly inert assertion. Serialization deliberately uses a narrower
/// predicate: only the synthetic omitted-key sentinel may disappear on write.
pub(crate) fn vacuous_expected_field(e: &Expected) -> Option<&'static str> {
    match e {
        Expected::MustContain { must_contain } if must_contain.iter().all(String::is_empty) => {
            Some("must_contain")
        }
        Expected::MustNotContain { must_not_contain } if must_not_contain.is_empty() => {
            Some("must_not_contain")
        }
        Expected::RegexMatch { pattern, .. } if pattern.is_empty() => Some("pattern"),
        Expected::SemanticSimilarityTo { min_score, .. }
            if *min_score <= -1.0 + SEMANTIC_SIMILARITY_EPSILON =>
        {
            Some("min_score")
        }
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
        } if (sequence.is_none() && rules.is_none() && policy.is_none())
            || (sequence.is_none() && rules.as_ref().is_some_and(Vec::is_empty)) =>
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
        Expected::MustNotContain { must_not_contain }
            if must_not_contain.iter().any(String::is_empty) =>
        {
            Some("must_not_contain contains an empty string, so no response can pass")
        }
        Expected::RegexNotMatch { pattern, .. } if pattern.is_empty() => {
            Some("an empty regex_not_match pattern matches every response, so no response can pass")
        }
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
    validate_static_inputs(e)?;
    Ok(())
}

fn validate_static_inputs(e: &Expected) -> anyhow::Result<()> {
    match e {
        Expected::RegexMatch { pattern, flags } | Expected::RegexNotMatch { pattern, flags } => {
            let mut builder = regex::RegexBuilder::new(pattern);
            for flag in flags {
                match flag.as_str() {
                    "i" => {
                        builder.case_insensitive(true);
                    }
                    "m" => {
                        builder.multi_line(true);
                    }
                    "s" => {
                        builder.dot_matches_new_line(true);
                    }
                    _ => {}
                }
            }
            builder
                .build()
                .map_err(|e| anyhow::anyhow!("invalid regex pattern: {e}"))?;
        }
        Expected::JsonSchema {
            json_schema,
            schema_file,
        } => {
            let source = if let Some(path) = schema_file {
                std::fs::read_to_string(path)
                    .map_err(|e| anyhow::anyhow!("failed to read schema_file '{path}': {e}"))?
            } else {
                json_schema.clone()
            };
            let schema: serde_json::Value = serde_json::from_str(&source)
                .map_err(|e| anyhow::anyhow!("invalid JSON schema: {e}"))?;
            jsonschema::validator_for(&schema)
                .map_err(|e| anyhow::anyhow!("schema compile failed: {e}"))?;
        }
        Expected::ArgsValid {
            schema: Some(schema),
            ..
        }
        | Expected::ToolOutputValid {
            schemas: Some(schema),
        } => validate_schema_map(schema)?,
        Expected::ArgsValid {
            policy: Some(path),
            schema: None,
        } => validate_args_policy(path)?,
        Expected::SequenceValid {
            policy: Some(path), ..
        } => validate_sequence_policy(path)?,
        _ => {}
    }
    Ok(())
}

fn validate_args_policy(path: &str) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read args_valid policy '{path}': {e}"))?;
    let policy: serde_json::Value = serde_yaml::from_str(&source)
        .map_err(|e| anyhow::anyhow!("invalid args_valid policy YAML: {e}"))?;

    let structured = [
        "version",
        "name",
        "tools",
        "allow",
        "deny",
        "schemas",
        "constraints",
        "enforcement",
        "limits",
        "signatures",
        "tool_pins",
        "discovery",
        "runtime_monitor",
        "kill_switch",
    ]
    .iter()
    .any(|key| policy.get(key).is_some());

    if structured {
        if let Some(schemas) = policy.get("schemas") {
            let schemas = schemas
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("args_valid policy schemas must be a mapping"))?;
            if !schemas.is_empty() {
                validate_schema_map(&serde_json::Value::Object(schemas.clone()))?;
            }
        }
        return Ok(());
    }

    validate_schema_map(&policy)
}

fn validate_sequence_policy(path: &str) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read sequence_valid policy '{path}': {e}"))?;
    if serde_yaml::from_str::<Vec<String>>(&source).is_ok() {
        return Ok(());
    }

    let rules = if let Ok(policy) = serde_yaml::from_str::<super::types::Policy>(&source) {
        policy.sequences
    } else {
        serde_yaml::from_str::<Vec<SequenceRule>>(&source)
            .map_err(|e| anyhow::anyhow!("invalid sequence_valid policy YAML: {e}"))?
    };
    let expected = Expected::SequenceValid {
        policy: None,
        sequence: None,
        rules: Some(rules),
    };
    if let Some(field) = vacuous_expected_field(&expected) {
        anyhow::bail!("`{field}` asserts nothing");
    }
    if let Some(reason) = non_executable_expected_reason(&expected) {
        anyhow::bail!("expected block is not executable: {reason}");
    }
    if let Some(reason) = ineffective_expected_reason(&expected) {
        anyhow::bail!("{reason}");
    }
    Ok(())
}

fn validate_schema_map(value: &serde_json::Value) -> anyhow::Result<()> {
    let schemas = value
        .as_object()
        .filter(|schemas| !schemas.is_empty())
        .ok_or_else(|| anyhow::anyhow!("schema must be a non-empty tool-name-to-schema map"))?;
    for (tool, schema) in schemas {
        if !schema.is_object() && !schema.is_boolean() {
            anyhow::bail!(
                "schema entry '{tool}' must be a JSON Schema; expected a tool-name-to-schema map"
            );
        }
        jsonschema::validator_for(schema)
            .map_err(|e| anyhow::anyhow!("schema for tool '{tool}' failed to compile: {e}"))?;
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

/// True for the legacy empty-`must_contain` sentinel.
///
/// `TestCase` does not retain whether this shape came from an omitted key or from
/// programmatic construction, so serialization cannot distinguish those origins.
pub(crate) fn is_omitted_expected_sentinel(e: &Expected) -> bool {
    matches!(e, Expected::MustContain { must_contain } if must_contain.is_empty())
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
