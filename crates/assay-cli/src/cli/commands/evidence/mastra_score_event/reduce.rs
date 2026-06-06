use super::constants::{
    MAX_BOUNDARY_STRING_CHARS, MAX_REASON_CHARS, RECEIPT_SCHEMA, REDUCER_VERSION, SOURCE_SURFACE,
    SOURCE_SYSTEM,
};
use super::validate::{
    bounded_string, normalized_timestamp, optional_bounded_string, validate_top_level,
};
use anyhow::{bail, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};

pub(super) fn reduce_score_event(
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

    let timestamp = normalized_timestamp(record.get("timestamp"), "timestamp", line_number)?;
    let target_ref = bounded_string(
        record.get("target_ref"),
        "target_ref",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;
    let score = record
        .get("score")
        .and_then(Value::as_number)
        .ok_or_else(|| anyhow::anyhow!("line {line_number} score must be a number"))?
        .clone();

    let score_id_ref = optional_bounded_string(
        record.get("score_id_ref"),
        "score_id_ref",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;
    let scorer_id = optional_bounded_string(
        record.get("scorer_id"),
        "scorer_id",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;
    let scorer_name = optional_bounded_string(
        record.get("scorer_name"),
        "scorer_name",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;

    if scorer_id.is_none() && scorer_name.is_none() {
        bail!("line {line_number} missing scorer identity; expected scorer_id or scorer_name");
    }

    let mut score_event = Map::new();
    score_event.insert("score".to_string(), Value::Number(score));
    score_event.insert("target_ref".to_string(), Value::String(target_ref));
    score_event.insert("timestamp".to_string(), Value::String(timestamp));

    if let Some(score_id_ref) = score_id_ref {
        score_event.insert("score_id_ref".to_string(), Value::String(score_id_ref));
    }
    if let Some(scorer_id) = scorer_id {
        score_event.insert("scorer_id".to_string(), Value::String(scorer_id));
    }
    if let Some(scorer_name) = scorer_name {
        score_event.insert("scorer_name".to_string(), Value::String(scorer_name));
    }

    for field in [
        "scorer_version",
        "score_source",
        "trace_id_ref",
        "span_id_ref",
        "score_trace_id_ref",
        "target_entity_type",
        "metadata_ref",
    ] {
        if let Some(value) = optional_bounded_string(
            record.get(field),
            field,
            MAX_BOUNDARY_STRING_CHARS,
            line_number,
        )? {
            score_event.insert(field.to_string(), Value::String(value));
        }
    }

    if let Some(reason) = optional_bounded_string(
        record.get("reason"),
        "reason",
        MAX_REASON_CHARS,
        line_number,
    )? {
        score_event.insert("reason".to_string(), Value::String(reason));
    }

    Ok(json!({
        "schema": RECEIPT_SCHEMA,
        "source_system": SOURCE_SYSTEM,
        "source_surface": SOURCE_SURFACE,
        "source_artifact_ref": source_artifact_ref,
        "source_artifact_digest": source_artifact_digest,
        "reducer_version": REDUCER_VERSION,
        "imported_at": import_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "score_event": Value::Object(score_event),
    }))
}
