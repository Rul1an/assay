use super::canonical::{hash_or_ref, HashOrRef};
use super::constants::{
    MAX_NAME_CHARS, MAX_REF_CHARS, RECEIPT_SCHEMA, REDUCER_VERSION, SOURCE_SURFACE, SOURCE_SYSTEM,
};
use super::validate::{
    bounded_string, normalized_timestamp, optional_bool, optional_nullable_bounded_string,
    optional_timestamp, required_bool, validate_call_keys, validate_output_keys,
    validate_top_level,
};
use anyhow::{bail, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};

struct ReceiptContext<'a> {
    source_artifact_ref: &'a str,
    source_artifact_digest: &'a str,
    import_time: DateTime<Utc>,
    document_number: usize,
    event_ref: &'a str,
    event_created_at: &'a str,
    has_tool_reply: Option<bool>,
    has_agent_handoff: Option<bool>,
}

pub(super) fn reduce_tool_action_event(
    row: &Value,
    source_artifact_ref: &str,
    source_artifact_digest: &str,
    import_time: DateTime<Utc>,
    document_number: usize,
) -> Result<Vec<Value>> {
    let record = row
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("document {document_number} must be a JSON object"))?;
    validate_top_level(record, document_number)?;

    let event_ref = bounded_string(
        record.get("event_ref"),
        "event_ref",
        MAX_REF_CHARS,
        document_number,
    )?;
    let event_created_at =
        normalized_timestamp(record.get("created_at"), "created_at", document_number)?;
    let has_tool_reply = optional_bool(
        record.get("has_tool_reply"),
        "has_tool_reply",
        document_number,
    )?;
    let has_agent_handoff = optional_bool(
        record.get("has_agent_handoff"),
        "has_agent_handoff",
        document_number,
    )?;

    let calls = record
        .get("function_calls")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow::anyhow!("document {document_number} function_calls must be an array")
        })?;
    let outputs = record
        .get("function_call_outputs")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow::anyhow!("document {document_number} function_call_outputs must be an array")
        })?;

    if calls.is_empty() {
        bail!("document {document_number} function_calls must not be empty");
    }
    if calls.len() != outputs.len() {
        bail!(
            "document {document_number} function_calls and function_call_outputs must have the same length"
        );
    }

    let pairs = paired_call_outputs(calls, outputs, document_number)?;
    let mut payloads = Vec::with_capacity(pairs.len());
    let context = ReceiptContext {
        source_artifact_ref,
        source_artifact_digest,
        import_time,
        document_number,
        event_ref: &event_ref,
        event_created_at: &event_created_at,
        has_tool_reply,
        has_agent_handoff,
    };

    for (call_index, output_index) in pairs {
        let call = calls[call_index].as_object().ok_or_else(|| {
            anyhow::anyhow!(
                "document {document_number} function_calls[{call_index}] must be a JSON object"
            )
        })?;
        let output = outputs[output_index].as_object();
        let payload = build_receipt(&context, call_index, output_index, call, output)?;
        payloads.push(payload);
    }

    Ok(payloads)
}

fn build_receipt(
    context: &ReceiptContext<'_>,
    call_index: usize,
    output_index: usize,
    call: &Map<String, Value>,
    output: Option<&Map<String, Value>>,
) -> Result<Value> {
    validate_call_keys(call, context.document_number, call_index)?;

    let call_name = bounded_string(
        call.get("name"),
        "function_calls[].name",
        MAX_NAME_CHARS,
        context.document_number,
    )?;

    let mut function = Map::new();
    function.insert(
        "event_ref".to_string(),
        Value::String(context.event_ref.to_string()),
    );
    function.insert(
        "call_index".to_string(),
        Value::Number((call_index as u64).into()),
    );
    function.insert("name".to_string(), Value::String(call_name.clone()));

    if let Some(call_id) = optional_nullable_bounded_string(
        call.get("call_id"),
        "function_calls[].call_id",
        MAX_REF_CHARS,
        context.document_number,
    )? {
        function.insert("call_id".to_string(), Value::String(call_id));
    }
    if let Some(group_id) = optional_nullable_bounded_string(
        call.get("group_id"),
        "function_calls[].group_id",
        MAX_REF_CHARS,
        context.document_number,
    )? {
        function.insert("group_id".to_string(), Value::String(group_id));
    }
    if let Some(created_at) = optional_timestamp(
        call.get("created_at"),
        "function_calls[].created_at",
        context.document_number,
    )? {
        function.insert("created_at".to_string(), Value::String(created_at));
    }
    match hash_or_ref(
        call,
        "arguments",
        "arguments_ref",
        "function_calls[]",
        context.document_number,
    )? {
        HashOrRef::Hash(value) => {
            function.insert("arguments_hash".to_string(), Value::String(value));
        }
        HashOrRef::Ref(value) => {
            function.insert("arguments_ref".to_string(), Value::String(value));
        }
        HashOrRef::Absent => {}
    }

    let mut outcome = Map::new();
    if let Some(output) = output {
        validate_output_keys(output, context.document_number, output_index)?;
        let output_name = bounded_string(
            output.get("name"),
            "function_call_outputs[].name",
            MAX_NAME_CHARS,
            context.document_number,
        )?;
        if call_name != output_name {
            bail!(
                "document {} paired call/output names differ at call index {call_index}",
                context.document_number
            );
        }

        outcome.insert("completed".to_string(), Value::Bool(true));
        outcome.insert(
            "is_error".to_string(),
            Value::Bool(required_bool(
                output.get("is_error"),
                "function_call_outputs[].is_error",
                context.document_number,
            )?),
        );
        if let Some(received_at) = optional_timestamp(
            output.get("created_at"),
            "function_call_outputs[].created_at",
            context.document_number,
        )? {
            outcome.insert("received_at".to_string(), Value::String(received_at));
        }
        match hash_or_ref(
            output,
            "output",
            "output_ref",
            "function_call_outputs[]",
            context.document_number,
        )? {
            HashOrRef::Hash(value) => {
                outcome.insert("output_hash".to_string(), Value::String(value));
            }
            HashOrRef::Ref(value) => {
                outcome.insert("output_ref".to_string(), Value::String(value));
            }
            HashOrRef::Absent => {}
        }
    } else {
        outcome.insert("completed".to_string(), Value::Bool(false));
    }

    let mut event_context = Map::new();
    event_context.insert(
        "event_created_at".to_string(),
        Value::String(context.event_created_at.to_string()),
    );
    if let Some(value) = context.has_tool_reply {
        event_context.insert("has_tool_reply".to_string(), Value::Bool(value));
    }
    if let Some(value) = context.has_agent_handoff {
        event_context.insert("has_agent_handoff".to_string(), Value::Bool(value));
    }

    Ok(json!({
        "schema": RECEIPT_SCHEMA,
        "source_system": SOURCE_SYSTEM,
        "source_surface": SOURCE_SURFACE,
        "source_artifact_ref": context.source_artifact_ref,
        "source_artifact_digest": context.source_artifact_digest,
        "reducer_version": REDUCER_VERSION,
        "imported_at": context.import_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "function": Value::Object(function),
        "outcome": Value::Object(outcome),
        "event_context": Value::Object(event_context),
    }))
}

fn paired_call_outputs(
    calls: &[Value],
    outputs: &[Value],
    document_number: usize,
) -> Result<Vec<(usize, usize)>> {
    let mut all_pairs_have_call_ids = true;
    let mut pair_call_ids = Vec::with_capacity(calls.len());

    for (index, (call, output)) in calls.iter().zip(outputs.iter()).enumerate() {
        let call = call.as_object().ok_or_else(|| {
            anyhow::anyhow!(
                "document {document_number} function_calls[{index}] must be a JSON object"
            )
        })?;
        validate_call_keys(call, document_number, index)?;
        let call_id = optional_nullable_bounded_string(
            call.get("call_id"),
            "function_calls[].call_id",
            MAX_REF_CHARS,
            document_number,
        )?;

        let output_id = if output.is_null() {
            None
        } else {
            let output = output.as_object().ok_or_else(|| {
                anyhow::anyhow!(
                    "document {document_number} function_call_outputs[{index}] must be a JSON object or null"
                )
            })?;
            validate_output_keys(output, document_number, index)?;
            optional_nullable_bounded_string(
                output.get("call_id"),
                "function_call_outputs[].call_id",
                MAX_REF_CHARS,
                document_number,
            )?
        };

        all_pairs_have_call_ids &= call_id.is_some() && output_id.is_some();
        pair_call_ids.push((call_id, output_id));
    }

    if all_pairs_have_call_ids {
        for (index, (call_id, output_id)) in pair_call_ids.iter().enumerate() {
            if call_id != output_id {
                bail!(
                    "document {document_number} call_id mismatch at index {index}: call has {:?}, output has {:?}",
                    call_id.as_deref(),
                    output_id.as_deref()
                );
            }
        }
    }

    Ok((0..calls.len()).map(|index| (index, index)).collect())
}
