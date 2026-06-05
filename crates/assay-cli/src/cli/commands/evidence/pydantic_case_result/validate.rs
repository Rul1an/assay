use super::constants::{INPUT_SCHEMA, SOURCE_SURFACE, SOURCE_SYSTEM};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};

pub(super) fn validate_top_level(record: &Map<String, Value>, line_number: usize) -> Result<()> {
    let allowed = [
        "schema",
        "framework",
        "surface",
        "case_name",
        "source_case_name",
        "source_ref",
        "results",
        "timestamp",
    ];
    if let Some(key) = record.keys().find(|key| !allowed.contains(&key.as_str())) {
        bail!(
            "line {line_number} contains unsupported top-level key {key:?}; v1 imports reduced case-result artifacts and excludes raw ReportCase, trace, Logfire, prompt, completion, input, expected-output, and model-output fields"
        );
    }

    string_equals(record, "schema", INPUT_SCHEMA, line_number)?;
    string_equals(record, "framework", SOURCE_SYSTEM, line_number)?;
    string_equals(record, "surface", SOURCE_SURFACE, line_number)?;
    Ok(())
}

pub(super) fn validate_result_keys(
    result: &Map<String, Value>,
    line_number: usize,
    result_number: usize,
) -> Result<()> {
    let allowed = ["kind", "evaluator_name", "passed", "score", "reason"];
    if let Some(key) = result.keys().find(|key| !allowed.contains(&key.as_str())) {
        bail!(
            "line {line_number} results[{result_number}] contains unsupported key {key:?}; v1 keeps only bounded evaluator identity and assertion/score values"
        );
    }
    Ok(())
}

fn string_equals(
    record: &Map<String, Value>,
    key: &str,
    expected: &str,
    line_number: usize,
) -> Result<()> {
    match record.get(key).and_then(Value::as_str) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => bail!("line {line_number} {key} must be {expected:?}, got {actual:?}"),
        None => bail!("line {line_number} missing string {key}"),
    }
}

pub(super) fn bounded_string(
    value: Option<&Value>,
    field_name: &str,
    max_chars: usize,
    line_number: usize,
) -> Result<String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("line {line_number} {field_name} must be a string"))?;
    validate_bounded_string(value, field_name, max_chars, line_number)
}

pub(super) fn optional_bounded_string(
    value: Option<&Value>,
    field_name: &str,
    max_chars: usize,
    line_number: usize,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    Ok(Some(validate_bounded_string(
        value.as_str().ok_or_else(|| {
            anyhow::anyhow!("line {line_number} {field_name} must be a string when present")
        })?,
        field_name,
        max_chars,
        line_number,
    )?))
}

fn validate_bounded_string(
    value: &str,
    field_name: &str,
    max_chars: usize,
    line_number: usize,
) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("line {line_number} {field_name} must not be empty");
    }
    if trimmed.chars().count() > max_chars {
        bail!("line {line_number} {field_name} must be at most {max_chars} characters");
    }
    if trimmed.contains('\n')
        || trimmed.contains('\r')
        || trimmed.contains('"')
        || trimmed.contains('`')
        || trimmed.contains('{')
        || trimmed.contains('}')
    {
        bail!("line {line_number} {field_name} is not reviewer-safe for v1");
    }
    Ok(trimmed.to_string())
}

pub(super) fn normalized_timestamp(
    value: Option<&Value>,
    field_name: &str,
    line_number: usize,
) -> Result<String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("line {line_number} {field_name} must be a string"))?;
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("line {line_number} {field_name} must be RFC3339 with timezone"))?
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, true))
}
