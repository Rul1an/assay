use super::constants::{INPUT_SCHEMA, SOURCE_SURFACE, SOURCE_SYSTEM};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};

pub(super) fn validate_top_level(record: &Map<String, Value>, line_number: usize) -> Result<()> {
    let allowed = [
        "schema",
        "framework",
        "surface",
        "timestamp",
        "score",
        "target_ref",
        "score_id_ref",
        "scorer_id",
        "scorer_name",
        "scorer_version",
        "score_source",
        "reason",
        "trace_id_ref",
        "span_id_ref",
        "score_trace_id_ref",
        "target_entity_type",
        "metadata_ref",
    ];
    if let Some(key) = record.keys().find(|key| !allowed.contains(&key.as_str())) {
        bail!(
            "line {line_number} contains unsupported top-level key {key:?}; v1 imports reduced score-event artifacts and excludes raw metadata, correlationContext, traces, logs, metrics, and feedback"
        );
    }

    string_equals(record, "schema", INPUT_SCHEMA, line_number)?;
    string_equals(record, "framework", SOURCE_SYSTEM, line_number)?;
    string_equals(record, "surface", SOURCE_SURFACE, line_number)?;
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
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(validate_bounded_string(
        value.as_str().ok_or_else(|| {
            anyhow::anyhow!("line {line_number} {field_name} must be a string or null")
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
