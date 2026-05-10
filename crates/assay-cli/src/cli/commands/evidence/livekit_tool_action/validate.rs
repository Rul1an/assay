use super::constants::{
    CALL_KEYS, FORBIDDEN_TOP_LEVEL_KEYS, INPUT_SCHEMA, OPTIONAL_TOP_LEVEL_KEYS, OUTPUT_KEYS,
    REQUIRED_TOP_LEVEL_KEYS, SOURCE_SURFACE, SOURCE_SYSTEM,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

pub(super) fn validate_top_level(
    record: &Map<String, Value>,
    document_number: usize,
) -> Result<()> {
    for (key, message) in FORBIDDEN_TOP_LEVEL_KEYS {
        if record.contains_key(*key) {
            bail!("document {document_number} {message}");
        }
    }

    let allowed = REQUIRED_TOP_LEVEL_KEYS
        .iter()
        .chain(OPTIONAL_TOP_LEVEL_KEYS.iter())
        .chain(FORBIDDEN_TOP_LEVEL_KEYS.iter().map(|(key, _)| key));
    let allowed = allowed.copied().collect::<BTreeSet<_>>();
    if let Some(key) = record.keys().find(|key| !allowed.contains(key.as_str())) {
        bail!(
            "document {document_number} contains unsupported top-level key {key:?}; v1 imports reduced LiveKit function tool execution artifacts only"
        );
    }

    for key in REQUIRED_TOP_LEVEL_KEYS {
        if !record.contains_key(*key) {
            bail!("document {document_number} missing required key {key:?}");
        }
    }
    string_equals(record, "schema", INPUT_SCHEMA, document_number)?;
    string_equals(record, "framework", SOURCE_SYSTEM, document_number)?;
    string_equals(record, "surface", SOURCE_SURFACE, document_number)?;
    string_equals(record, "runtime_mode", "agent_session", document_number)?;
    if let Some(value) = record.get("type") {
        match value.as_str() {
            Some("function_tools_executed") => {}
            Some(actual) => bail!(
                "document {document_number} type must be \"function_tools_executed\", got {actual:?}"
            ),
            None => bail!("document {document_number} type must be a string when present"),
        }
    }
    Ok(())
}

pub(super) fn validate_call_keys(
    call: &Map<String, Value>,
    document_number: usize,
    call_index: usize,
) -> Result<()> {
    if let Some(key) = call.keys().find(|key| !CALL_KEYS.contains(&key.as_str())) {
        bail!(
            "document {document_number} function_calls[{call_index}] contains unsupported key {key:?}; v1 keeps only bounded function identity and argument digest/ref"
        );
    }
    Ok(())
}

pub(super) fn validate_output_keys(
    output: &Map<String, Value>,
    document_number: usize,
    output_index: usize,
) -> Result<()> {
    if let Some(key) = output
        .keys()
        .find(|key| !OUTPUT_KEYS.contains(&key.as_str()))
    {
        bail!(
            "document {document_number} function_call_outputs[{output_index}] contains unsupported key {key:?}; v1 keeps only bounded output digest/ref and error status"
        );
    }
    Ok(())
}

fn string_equals(
    record: &Map<String, Value>,
    key: &str,
    expected: &str,
    document_number: usize,
) -> Result<()> {
    match record.get(key).and_then(Value::as_str) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => {
            bail!("document {document_number} {key} must be {expected:?}, got {actual:?}")
        }
        None => bail!("document {document_number} missing string {key}"),
    }
}

pub(super) fn bounded_string(
    value: Option<&Value>,
    field_name: &str,
    max_chars: usize,
    document_number: usize,
) -> Result<String> {
    let value = value.and_then(Value::as_str).ok_or_else(|| {
        anyhow::anyhow!("document {document_number} {field_name} must be a string")
    })?;
    validate_bounded_string(value, field_name, max_chars, document_number)
}

pub(super) fn optional_bounded_string(
    value: Option<&Value>,
    field_name: &str,
    max_chars: usize,
    document_number: usize,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    Ok(Some(validate_bounded_string(
        value.as_str().ok_or_else(|| {
            anyhow::anyhow!("document {document_number} {field_name} must be a string when present")
        })?,
        field_name,
        max_chars,
        document_number,
    )?))
}

pub(super) fn optional_nullable_bounded_string(
    value: Option<&Value>,
    field_name: &str,
    max_chars: usize,
    document_number: usize,
) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(validate_bounded_string(
            value.as_str().ok_or_else(|| {
                anyhow::anyhow!(
                    "document {document_number} {field_name} must be a string when present"
                )
            })?,
            field_name,
            max_chars,
            document_number,
        )?)),
    }
}

fn validate_bounded_string(
    value: &str,
    field_name: &str,
    max_chars: usize,
    document_number: usize,
) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("document {document_number} {field_name} must not be empty");
    }
    if trimmed.chars().count() > max_chars {
        bail!("document {document_number} {field_name} must be at most {max_chars} characters");
    }
    if trimmed.contains('\n')
        || trimmed.contains('\r')
        || trimmed.contains('"')
        || trimmed.contains('`')
        || trimmed.contains('{')
        || trimmed.contains('}')
    {
        bail!("document {document_number} {field_name} is not reviewer-safe for v1");
    }
    Ok(trimmed.to_string())
}

pub(super) fn optional_bool(
    value: Option<&Value>,
    field_name: &str,
    document_number: usize,
) -> Result<Option<bool>> {
    match value {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => bail!("document {document_number} {field_name} must be a boolean when present"),
    }
}

pub(super) fn required_bool(
    value: Option<&Value>,
    field_name: &str,
    document_number: usize,
) -> Result<bool> {
    value
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("document {document_number} {field_name} must be a boolean"))
}

pub(super) fn optional_timestamp(
    value: Option<&Value>,
    field_name: &str,
    document_number: usize,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(normalized_timestamp(
        Some(value),
        field_name,
        document_number,
    )?))
}

pub(super) fn normalized_timestamp(
    value: Option<&Value>,
    field_name: &str,
    document_number: usize,
) -> Result<String> {
    let value = value
        .ok_or_else(|| anyhow::anyhow!("document {document_number} {field_name} is required"))?;
    if let Some(value) = value.as_str() {
        return Ok(DateTime::parse_from_rfc3339(value)
            .with_context(|| {
                format!("document {document_number} {field_name} must be RFC3339 with timezone")
            })?
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Millis, true));
    }

    let Some(seconds) = value.as_f64() else {
        bail!("document {document_number} {field_name} must be RFC3339 string or unix seconds");
    };
    if !seconds.is_finite() {
        bail!("document {document_number} {field_name} must be finite unix seconds");
    }
    let millis = (seconds * 1000.0).round();
    if !(i64::MIN as f64..=i64::MAX as f64).contains(&millis) {
        bail!("document {document_number} {field_name} is outside supported timestamp range");
    }
    let millis = millis as i64;
    Ok(DateTime::<Utc>::from_timestamp_millis(millis)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "document {document_number} {field_name} is outside supported timestamp range"
            )
        })?
        .to_rfc3339_opts(SecondsFormat::Millis, true))
}
