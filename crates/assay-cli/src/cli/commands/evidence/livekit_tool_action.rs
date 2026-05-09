use crate::exit_codes;
use anyhow::{bail, Context, Result};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Args;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

const EVENT_TYPE: &str = "assay.receipt.livekit.tool_action.v1";
const EVENT_SOURCE: &str = "urn:assay:external:livekit:function-tool-call";
const RECEIPT_SCHEMA: &str = "assay.receipt.livekit.tool-action.v1";
const SOURCE_SYSTEM: &str = "livekit_agents";
const SOURCE_SURFACE: &str = "function_tools_executed";
const REDUCER_VERSION: &str = "assay-livekit-function-tools-executed@0.1.0";
const INPUT_SCHEMA: &str = "livekit.function-tools-executed.export.v1";
const DEFAULT_RUN_ID: &str = "import-livekit-tool-action";
const MAX_NAME_CHARS: usize = 160;
const MAX_REF_CHARS: usize = 240;

const REQUIRED_TOP_LEVEL_KEYS: &[&str] = &[
    "schema",
    "framework",
    "surface",
    "runtime_mode",
    "event_ref",
    "created_at",
    "function_calls",
    "function_call_outputs",
];

const OPTIONAL_TOP_LEVEL_KEYS: &[&str] = &["type", "has_tool_reply", "has_agent_handoff"];

const CALL_KEYS: &[&str] = &[
    "id",
    "type",
    "call_id",
    "name",
    "arguments",
    "arguments_ref",
    "created_at",
    "group_id",
];

const OUTPUT_KEYS: &[&str] = &[
    "id",
    "type",
    "call_id",
    "name",
    "output",
    "output_ref",
    "is_error",
    "created_at",
];

const FORBIDDEN_TOP_LEVEL_KEYS: &[(&str, &str)] = &[
    (
        "transcript",
        "artifact: transcript import is out of scope for LiveKit tool-action v1",
    ),
    (
        "audio",
        "artifact: audio import is out of scope for LiveKit tool-action v1",
    ),
    (
        "user_input",
        "artifact: raw user input is out of scope for LiveKit tool-action v1",
    ),
    (
        "model_output",
        "artifact: raw model output is out of scope for LiveKit tool-action v1",
    ),
    (
        "usage",
        "artifact: usage telemetry is out of scope for LiveKit tool-action v1",
    ),
    (
        "latency",
        "artifact: latency telemetry is out of scope for LiveKit tool-action v1",
    ),
    (
        "room_state",
        "artifact: room state is out of scope for LiveKit tool-action v1",
    ),
    (
        "participant_identity",
        "artifact: participant identity is out of scope for LiveKit tool-action v1",
    ),
    (
        "capture_context",
        "artifact: capture context and session identity are out of scope for LiveKit tool-action v1",
    ),
    (
        "trace",
        "artifact: full trace payloads are out of scope for LiveKit tool-action v1",
    ),
    (
        "spans",
        "artifact: full span payloads are out of scope for LiveKit tool-action v1",
    ),
];

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

#[derive(Debug, Args, Clone)]
pub struct LiveKitToolActionArgs {
    /// LiveKit FunctionToolsExecutedEvent reduced artifact file
    #[arg(long, value_name = "PATH")]
    pub input: PathBuf,

    /// Output Assay evidence bundle path (.tar.gz)
    #[arg(long, alias = "out", value_name = "PATH")]
    pub bundle_out: PathBuf,

    /// Reviewer-safe source artifact reference stored in receipts
    #[arg(long)]
    pub source_artifact_ref: Option<String>,

    /// Assay import run id used for receipt provenance and event ids
    #[arg(long, default_value = DEFAULT_RUN_ID)]
    pub run_id: String,

    /// Import timestamp for deterministic fixtures (RFC3339 UTC recommended)
    #[arg(long)]
    pub import_time: Option<String>,
}

pub fn cmd_livekit_tool_action(args: LiveKitToolActionArgs) -> Result<i32> {
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let source_artifact_ref = args
        .source_artifact_ref
        .unwrap_or_else(|| default_source_artifact_ref(&args.input));
    let source_artifact_digest = sha256_file(&args.input)
        .with_context(|| format!("failed to digest input {}", args.input.display()))?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let events = read_livekit_tool_actions(
        &args.input,
        &source_artifact_ref,
        &source_artifact_digest,
        &args.run_id,
        import_time,
        &producer,
    )?;

    let out_file = File::create(&args.bundle_out)
        .with_context(|| format!("failed to create bundle {}", args.bundle_out.display()))?;
    let mut writer = BundleWriter::new(out_file).with_producer(producer);
    for event in events {
        writer.add_event(event);
    }
    writer
        .finish()
        .with_context(|| format!("failed to write bundle {}", args.bundle_out.display()))?;

    eprintln!(
        "Imported LiveKit tool-action receipts to {}",
        args.bundle_out.display()
    );

    Ok(exit_codes::OK)
}

fn read_livekit_tool_actions(
    input: &Path,
    source_artifact_ref: &str,
    source_artifact_digest: &str,
    run_id: &str,
    import_time: DateTime<Utc>,
    producer: &ProducerMeta,
) -> Result<Vec<EvidenceEvent>> {
    if run_id.contains(':') {
        bail!("run_id cannot contain ':' because event ids use run_id:seq");
    }

    let input_text = std::fs::read_to_string(input)
        .with_context(|| format!("failed to read input {}", input.display()))?;
    let rows = parse_input_documents(&input_text)?;
    let mut events = Vec::new();

    for (document_index, row) in rows.iter().enumerate() {
        let document_number = document_index + 1;
        let payloads = reduce_tool_action_event(
            row,
            source_artifact_ref,
            source_artifact_digest,
            import_time,
            document_number,
        )?;
        for payload in payloads {
            let seq = events.len() as u64;
            let event = EvidenceEvent::new(EVENT_TYPE, EVENT_SOURCE, run_id, seq, payload)
                .with_time(import_time)
                .with_producer(producer);
            events.push(event);
        }
    }

    if events.is_empty() {
        bail!("input produced no LiveKit tool-action receipts");
    }

    Ok(events)
}

fn parse_input_documents(input_text: &str) -> Result<Vec<Value>> {
    let trimmed = input_text.trim();
    if trimmed.is_empty() {
        bail!("input contains no JSON documents");
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        match value {
            Value::Object(_) => return Ok(vec![value]),
            Value::Array(values) => {
                if values.is_empty() {
                    bail!("input JSON array contains no documents");
                }
                return Ok(values);
            }
            _ => bail!("input JSON document must be an object, array of objects, or JSONL rows"),
        }
    }

    let mut rows = Vec::new();
    for (line_index, line) in input_text.lines().enumerate() {
        let line_number = line_index + 1;
        if line.trim().is_empty() {
            continue;
        }
        rows.push(
            serde_json::from_str(line)
                .with_context(|| format!("invalid JSONL object at line {line_number}"))?,
        );
    }

    if rows.is_empty() {
        bail!("input contains no JSONL rows");
    }

    Ok(rows)
}

fn reduce_tool_action_event(
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
        let output = outputs[output_index].as_object().ok_or_else(|| {
            anyhow::anyhow!(
                "document {document_number} function_call_outputs[{output_index}] must be a JSON object"
            )
        })?;
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
    output: &Map<String, Value>,
) -> Result<Value> {
    validate_call_keys(call, context.document_number, call_index)?;
    validate_output_keys(output, context.document_number, output_index)?;

    let call_name = bounded_string(
        call.get("name"),
        "function_calls[].name",
        MAX_NAME_CHARS,
        context.document_number,
    )?;
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

    let mut function = Map::new();
    function.insert(
        "event_ref".to_string(),
        Value::String(context.event_ref.to_string()),
    );
    function.insert(
        "call_index".to_string(),
        Value::Number((call_index as u64).into()),
    );
    function.insert("name".to_string(), Value::String(call_name));

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
    let mut calls_all_have_ids = true;
    let mut calls_any_have_ids = false;
    for (index, call) in calls.iter().enumerate() {
        let call = call.as_object().ok_or_else(|| {
            anyhow::anyhow!(
                "document {document_number} function_calls[{index}] must be a JSON object"
            )
        })?;
        validate_call_keys(call, document_number, index)?;
        let has_id = optional_nullable_bounded_string(
            call.get("call_id"),
            "function_calls[].call_id",
            MAX_REF_CHARS,
            document_number,
        )?
        .is_some();
        calls_all_have_ids &= has_id;
        calls_any_have_ids |= has_id;
    }

    let mut outputs_all_have_ids = true;
    let mut outputs_any_have_ids = false;
    for (index, output) in outputs.iter().enumerate() {
        if output.is_null() {
            bail!(
                "document {document_number} function_call_outputs[{index}] is null; missing output is malformed in LiveKit tool-action v1"
            );
        }
        let output = output.as_object().ok_or_else(|| {
            anyhow::anyhow!(
                "document {document_number} function_call_outputs[{index}] must be a JSON object"
            )
        })?;
        validate_output_keys(output, document_number, index)?;
        let has_id = optional_nullable_bounded_string(
            output.get("call_id"),
            "function_call_outputs[].call_id",
            MAX_REF_CHARS,
            document_number,
        )?
        .is_some();
        outputs_all_have_ids &= has_id;
        outputs_any_have_ids |= has_id;
    }

    if calls_any_have_ids || outputs_any_have_ids {
        if !calls_all_have_ids || !outputs_all_have_ids {
            bail!(
                "document {document_number} call_id pairing requires every call and output to include call_id"
            );
        }
        let mut output_by_id = BTreeMap::new();
        for (index, output) in outputs.iter().enumerate() {
            let output = output.as_object().unwrap();
            let call_id = optional_nullable_bounded_string(
                output.get("call_id"),
                "function_call_outputs[].call_id",
                MAX_REF_CHARS,
                document_number,
            )?
            .unwrap();
            if output_by_id.insert(call_id.clone(), index).is_some() {
                bail!("document {document_number} duplicate output call_id {call_id:?}");
            }
        }

        let mut seen_call_ids = BTreeSet::new();
        let mut pairs = Vec::with_capacity(calls.len());
        for (call_index, call) in calls.iter().enumerate() {
            let call = call.as_object().unwrap();
            let call_id = optional_nullable_bounded_string(
                call.get("call_id"),
                "function_calls[].call_id",
                MAX_REF_CHARS,
                document_number,
            )?
            .unwrap();
            if !seen_call_ids.insert(call_id.clone()) {
                bail!("document {document_number} duplicate call_id {call_id:?}");
            }
            let Some(output_index) = output_by_id.get(&call_id).copied() else {
                bail!("document {document_number} missing output for call_id {call_id:?}");
            };
            pairs.push((call_index, output_index));
        }
        return Ok(pairs);
    }

    Ok((0..calls.len()).map(|index| (index, index)).collect())
}

fn validate_top_level(record: &Map<String, Value>, document_number: usize) -> Result<()> {
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

fn validate_call_keys(
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

fn validate_output_keys(
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

fn bounded_string(
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

fn optional_bounded_string(
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

fn optional_nullable_bounded_string(
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

fn optional_bool(
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

fn required_bool(value: Option<&Value>, field_name: &str, document_number: usize) -> Result<bool> {
    value
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("document {document_number} {field_name} must be a boolean"))
}

fn optional_timestamp(
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

fn normalized_timestamp(
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

enum HashOrRef {
    Hash(String),
    Ref(String),
    Absent,
}

fn hash_or_ref(
    record: &Map<String, Value>,
    raw_key: &str,
    ref_key: &str,
    field_context: &str,
    document_number: usize,
) -> Result<HashOrRef> {
    if record.contains_key(raw_key) && record.contains_key(ref_key) {
        bail!("document {document_number} {field_context}: {raw_key} and {ref_key} must not both be present");
    }
    if let Some(raw) = record.get(raw_key) {
        return Ok(HashOrRef::Hash(sha256_json_value(raw)?));
    }
    if let Some(reference) = optional_bounded_string(
        record.get(ref_key),
        &format!("{field_context}.{ref_key}"),
        MAX_REF_CHARS,
        document_number,
    )? {
        return Ok(HashOrRef::Ref(reference));
    }
    Ok(HashOrRef::Absent)
}

fn sha256_json_value(value: &Value) -> Result<String> {
    let canonical = canonical_json(value)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

fn canonical_json(value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok("null".to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::String(value) => Ok(serde_json::to_string(value)?),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                return Ok(value.to_string());
            }
            if let Some(value) = number.as_u64() {
                return Ok(value.to_string());
            }
            let Some(value) = number.as_f64() else {
                bail!("unsupported JSON number in canonical JSON");
            };
            if !value.is_finite() {
                bail!("non-finite floats are not valid in canonical JSON");
            }
            if value.fract() != 0.0 {
                bail!("non-integer floats are not valid in LiveKit tool-action canonical JSON");
            }
            Ok(format!("{value:.0}"))
        }
        Value::Array(values) => {
            let items = values
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>>>()?;
            Ok(format!("[{}]", items.join(",")))
        }
        Value::Object(map) => {
            let mut parts = Vec::with_capacity(map.len());
            for key in map.keys().collect::<BTreeSet<_>>() {
                let key_json = serde_json::to_string(key)?;
                let value_json = canonical_json(map.get(key).unwrap())?;
                parts.push(format!("{key_json}:{value_json}"));
            }
            Ok(format!("{{{}}}", parts.join(",")))
        }
    }
}

fn parse_import_time(value: Option<&str>) -> Result<DateTime<Utc>> {
    match value {
        Some(value) => Ok(DateTime::parse_from_rfc3339(value)
            .with_context(|| format!("invalid --import-time {value:?}; expected RFC3339"))?
            .with_timezone(&Utc)),
        None => Ok(Utc::now()),
    }
}

fn default_source_artifact_ref(input: &Path) -> String {
    input
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("livekit-tool-action.json")
        .to_string()
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_evidence::bundle::BundleReader;
    use std::fs;

    #[test]
    fn import_writes_verifiable_tool_action_bundle_without_raw_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("livekit-tool-action.json");
        let output = dir.path().join("livekit-tool-action.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","type":"function_tools_executed","event_ref":"turn-42:function_tools_executed:0","created_at":1778320801.5,"function_calls":[{"id":"item_call_lookup_order","call_id":"call_lookup_order_01","name":"lookup_customer_order","arguments":{"order_id":"ord_123","include_items":true},"created_at":1778320801.234,"group_id":null}],"function_call_outputs":[{"id":"item_output_lookup_order","call_id":"call_lookup_order_01","name":"lookup_customer_order","is_error":false,"output":{"status":"shipped","items_count":2},"created_at":1778320801.467}],"has_tool_reply":true,"has_agent_handoff":false}"#,
        )
        .unwrap();

        let code = cmd_livekit_tool_action(LiveKitToolActionArgs {
            input: input.clone(),
            bundle_out: output.clone(),
            source_artifact_ref: Some("livekit-tool-action.json".to_string()),
            run_id: "livekit_test".to_string(),
            import_time: Some("2026-05-09T10:00:02Z".to_string()),
        })
        .unwrap();
        assert_eq!(code, exit_codes::OK);

        let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
        assert_eq!(reader.manifest().event_count, 1);
        let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(events[0].type_, EVENT_TYPE);
        assert_eq!(events[0].source, EVENT_SOURCE);
        assert_eq!(events[0].payload["schema"], RECEIPT_SCHEMA);
        assert_eq!(events[0].payload["source_surface"], SOURCE_SURFACE);
        assert_eq!(
            events[0].payload["function"]["name"],
            "lookup_customer_order"
        );
        assert_eq!(
            events[0].payload["function"]["call_id"],
            "call_lookup_order_01"
        );
        assert_eq!(
            events[0].payload["function"]["created_at"],
            "2026-05-09T10:00:01.234Z"
        );
        assert_eq!(events[0].payload["outcome"]["completed"], true);
        assert_eq!(events[0].payload["outcome"]["is_error"], false);
        assert_eq!(events[0].payload["event_context"]["has_tool_reply"], true);

        let serialized = serde_json::to_string(&events[0].payload).unwrap();
        assert!(!serialized.contains("ord_123"));
        assert!(!serialized.contains("shipped"));
        assert!(!serialized.contains("session_id"));
        assert!(serialized.contains("arguments_hash"));
        assert!(serialized.contains("output_hash"));
    }

    #[test]
    fn import_pairs_by_call_id_before_list_order() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("livekit-tool-action.json");
        let output = dir.path().join("livekit-tool-action.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-45:function_tools_executed:0","created_at":"2026-05-09T10:05:00Z","function_calls":[{"call_id":"call_a","name":"lookup_a","arguments_ref":"arg:a"},{"call_id":"call_b","name":"lookup_b","arguments_ref":"arg:b"}],"function_call_outputs":[{"call_id":"call_b","name":"lookup_b","is_error":false,"output_ref":"out:b"},{"call_id":"call_a","name":"lookup_a","is_error":false,"output_ref":"out:a"}]}"#,
        )
        .unwrap();

        cmd_livekit_tool_action(LiveKitToolActionArgs {
            input,
            bundle_out: output.clone(),
            source_artifact_ref: None,
            run_id: "livekit_pairing_test".to_string(),
            import_time: Some("2026-05-09T10:05:02Z".to_string()),
        })
        .unwrap();

        let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
        let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].payload["function"]["call_id"], "call_a");
        assert_eq!(events[0].payload["outcome"]["output_ref"], "out:a");
        assert_eq!(events[1].payload["function"]["call_id"], "call_b");
        assert_eq!(events[1].payload["outcome"]["output_ref"], "out:b");
    }

    #[test]
    fn import_accepts_multi_row_jsonl_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("livekit-tool-actions.jsonl");
        let output = dir.path().join("livekit-tool-actions.tar.gz");
        fs::write(
            &input,
            concat!(
                r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-46:function_tools_executed:0","created_at":"2026-05-09T10:06:00Z","function_calls":[{"name":"lookup_a","arguments_ref":"arg:a"}],"function_call_outputs":[{"name":"lookup_a","is_error":false,"output_ref":"out:a"}]}"#,
                "\n",
                r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-47:function_tools_executed:0","created_at":"2026-05-09T10:07:00Z","function_calls":[{"name":"lookup_b","arguments_ref":"arg:b"}],"function_call_outputs":[{"name":"lookup_b","is_error":false,"output_ref":"out:b"}]}"#,
                "\n"
            ),
        )
        .unwrap();

        cmd_livekit_tool_action(LiveKitToolActionArgs {
            input,
            bundle_out: output.clone(),
            source_artifact_ref: None,
            run_id: "livekit_jsonl_test".to_string(),
            import_time: Some("2026-05-09T10:07:02Z".to_string()),
        })
        .unwrap();

        let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
        assert_eq!(reader.manifest().event_count, 2);
    }

    #[test]
    fn import_rejects_missing_output_none() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("livekit-tool-action.json");
        let output = dir.path().join("livekit-tool-action.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-44:function_tools_executed:0","created_at":1778320921.5,"function_calls":[{"call_id":"call_missing_output_01","name":"lookup_customer_order","arguments":{"order_id":"ord_404"}}],"function_call_outputs":[null]}"#,
        )
        .unwrap();

        let err = cmd_livekit_tool_action(LiveKitToolActionArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "livekit_missing_output_test".to_string(),
            import_time: Some("2026-05-09T10:02:02Z".to_string()),
        })
        .unwrap_err();
        assert!(err.to_string().contains("missing output is malformed"));
    }

    #[test]
    fn import_rejects_capture_context_and_session_identity() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("livekit-tool-action.json");
        let output = dir.path().join("livekit-tool-action.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-42:function_tools_executed:0","created_at":1778320801.5,"capture_context":{"session_id":"session-secret"},"function_calls":[{"call_id":"call_a","name":"lookup_a","arguments_ref":"arg:a"}],"function_call_outputs":[{"call_id":"call_a","name":"lookup_a","is_error":false,"output_ref":"out:a"}]}"#,
        )
        .unwrap();

        let err = cmd_livekit_tool_action(LiveKitToolActionArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "livekit_context_test".to_string(),
            import_time: Some("2026-05-09T10:02:02Z".to_string()),
        })
        .unwrap_err();
        assert!(err.to_string().contains("capture context"));
    }

    #[test]
    fn import_rejects_non_integer_raw_float_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("livekit-tool-action.json");
        let output = dir.path().join("livekit-tool-action.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","event_ref":"turn-42:function_tools_executed:0","created_at":1778320801.5,"function_calls":[{"call_id":"call_a","name":"lookup_a","arguments":{"confidence":0.25}}],"function_call_outputs":[{"call_id":"call_a","name":"lookup_a","is_error":false,"output_ref":"out:a"}]}"#,
        )
        .unwrap();

        let err = cmd_livekit_tool_action(LiveKitToolActionArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "livekit_float_test".to_string(),
            import_time: Some("2026-05-09T10:02:02Z".to_string()),
        })
        .unwrap_err();
        assert!(err.to_string().contains("non-integer floats"));
    }
}
