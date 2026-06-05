use anyhow::{bail, Result};
use serde_json::Value;

pub(super) fn bounded_string(
    value: Option<&Value>,
    field_name: &str,
    max_chars: usize,
) -> Result<String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("{field_name} must be a string"))?;
    validate_bounded_string(value, field_name, max_chars)
}

pub(super) fn optional_bounded_string(
    value: Option<&Value>,
    field_name: &str,
    max_chars: usize,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(validate_bounded_string(
        value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{field_name} must be a string or null"))?,
        field_name,
        max_chars,
    )?))
}

pub(super) fn validate_bounded_string(
    value: &str,
    field_name: &str,
    max_chars: usize,
) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field_name} must not be empty");
    }
    if trimmed.chars().count() > max_chars {
        bail!("{field_name} must be at most {max_chars} characters");
    }
    if trimmed.contains('\n')
        || trimmed.contains('\r')
        || trimmed.contains('"')
        || trimmed.contains('`')
        || trimmed.contains('{')
        || trimmed.contains('}')
    {
        bail!("{field_name} is not reviewer-safe for v1");
    }
    Ok(trimmed.to_string())
}
