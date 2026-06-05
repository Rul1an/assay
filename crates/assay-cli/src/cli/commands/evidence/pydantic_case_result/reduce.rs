use super::constants::{
    MAX_BOUNDARY_STRING_CHARS, MAX_REASON_CHARS, MAX_RESULTS, RECEIPT_SCHEMA, REDUCER_VERSION,
    SOURCE_SURFACE, SOURCE_SYSTEM,
};
use super::validate::{
    bounded_string, normalized_timestamp, optional_bounded_string, validate_result_keys,
    validate_top_level,
};
use anyhow::{bail, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Number, Value};

pub(super) fn reduce_case_result(
    row: &Value,
    source_artifact_ref: &str,
    source_artifact_digest: &str,
    import_time: DateTime<Utc>,
    line_number: usize,
) -> Result<Value> {
    let record = row
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("line {line_number} must be a JSON object"))?;
    validate_top_level(record, line_number)?;

    let case_name = bounded_string(
        record.get("case_name"),
        "case_name",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;
    let timestamp = normalized_timestamp(record.get("timestamp"), "timestamp", line_number)?;
    let results = reduced_results(record.get("results"), line_number)?;

    let mut case_result = Map::new();
    case_result.insert("case_name".to_string(), Value::String(case_name));
    case_result.insert("timestamp".to_string(), Value::String(timestamp));
    case_result.insert("results".to_string(), Value::Array(results));

    for field in ["source_case_name", "source_ref"] {
        if let Some(value) = optional_bounded_string(
            record.get(field),
            field,
            MAX_BOUNDARY_STRING_CHARS,
            line_number,
        )? {
            case_result.insert(field.to_string(), Value::String(value));
        }
    }

    Ok(json!({
        "schema": RECEIPT_SCHEMA,
        "source_system": SOURCE_SYSTEM,
        "source_surface": SOURCE_SURFACE,
        "source_artifact_ref": source_artifact_ref,
        "source_artifact_digest": source_artifact_digest,
        "reducer_version": REDUCER_VERSION,
        "imported_at": import_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "case_result": Value::Object(case_result),
    }))
}

fn reduced_results(value: Option<&Value>, line_number: usize) -> Result<Vec<Value>> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("line {line_number} results must be an array"))?;
    if values.is_empty() {
        bail!("line {line_number} results must not be empty");
    }
    if values.len() > MAX_RESULTS {
        bail!("line {line_number} results must contain at most {MAX_RESULTS} entries");
    }

    values
        .iter()
        .enumerate()
        .map(|(index, value)| reduced_result(value, line_number, index + 1))
        .collect()
}

fn reduced_result(value: &Value, line_number: usize, result_number: usize) -> Result<Value> {
    let result = value.as_object().ok_or_else(|| {
        anyhow::anyhow!("line {line_number} results[{result_number}] must be a JSON object")
    })?;
    validate_result_keys(result, line_number, result_number)?;

    let kind = bounded_string(
        result.get("kind"),
        "results[].kind",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;
    let evaluator_name = bounded_string(
        result.get("evaluator_name"),
        "results[].evaluator_name",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;

    let mut reduced = Map::new();
    reduced.insert("kind".to_string(), Value::String(kind.clone()));
    reduced.insert("evaluator_name".to_string(), Value::String(evaluator_name));

    match kind.as_str() {
        "assertion" => {
            if result.contains_key("score") {
                bail!(
                    "line {line_number} results[{result_number}] assertion result must not include score"
                );
            }
            let passed = result
                .get("passed")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "line {line_number} results[{result_number}] assertion passed must be a boolean"
                    )
                })?;
            reduced.insert("passed".to_string(), Value::Bool(passed));
        }
        "score" => {
            if result.contains_key("passed") {
                bail!(
                    "line {line_number} results[{result_number}] score result must not include passed"
                );
            }
            let score = result
                .get("score")
                .and_then(Value::as_number)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "line {line_number} results[{result_number}] score must be a number"
                    )
                })?;
            reduced.insert("score".to_string(), Value::Number(normalize_number(score)?));
        }
        actual => bail!(
            "line {line_number} results[{result_number}] kind must be \"assertion\" or \"score\", got {actual:?}"
        ),
    }

    if let Some(reason) = optional_bounded_string(
        result.get("reason"),
        "results[].reason",
        MAX_REASON_CHARS,
        line_number,
    )? {
        reduced.insert("reason".to_string(), Value::String(reason));
    }

    Ok(Value::Object(reduced))
}

fn normalize_number(number: &Number) -> Result<Number> {
    if number
        .as_f64()
        .map(|value| value.is_finite())
        .unwrap_or(false)
    {
        return Ok(number.clone());
    }
    bail!("score must be finite");
}
