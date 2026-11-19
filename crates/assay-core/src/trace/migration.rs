//! Opt-in conversion of legacy replay rows to native JSONL.
use super::observation::{decode_observations, ObservedTraceEvent};
use super::schema::{default_replay_meta, legacy_response_key, TraceEntryV1, TraceEvent};
use super::truncation::compute_sha256_str;
use super::upgrader::{observe_event, upgrade_v1_to_v2, LegacyOutput, LEGACY_STEP_SUFFIX};
use anyhow::{ensure, Context};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;

const MAX_LINE_BYTES: u64 = 16 * 1024 * 1024;

/// Publish a new JSONL file only after all source rows have converted successfully.
pub fn migrate_file(input: &Path, output: &Path) -> anyhow::Result<()> {
    ensure!(
        output
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl")),
        "migration output must be a new .jsonl file"
    );
    ensure!(!output.try_exists()?, "migration output already exists");
    let input = File::open(input)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    let mut out = BufWriter::new(staged.as_file_mut());
    let mut reader = BufReader::new(input);
    let mut line = String::new();
    let ids = rusqlite::Connection::open("")?;
    ids.execute_batch(
        "CREATE TABLE ids (id TEXT PRIMARY KEY, native INTEGER NOT NULL) WITHOUT ROWID; BEGIN",
    )?;
    for line_no in 1u64.. {
        line.clear();
        let size = reader
            .by_ref()
            .take(MAX_LINE_BYTES + 1)
            .read_line(&mut line)?;
        if size == 0 {
            break;
        }
        ensure!(
            size as u64 <= MAX_LINE_BYTES,
            "trace line {line_no} exceeds 16 MiB"
        );
        let source = line.trim();
        if source.is_empty() {
            continue;
        }
        for event in
            convert_line(source, &ids).with_context(|| format!("convert trace line {line_no}"))?
        {
            // The upgrader appends its own measurement last for every scanned event.
            if !super::upgrader::declared_field_roots(event.event()).is_empty() {
                if let Some(loss) = event.observations().last().and_then(|o| o.losses.first()) {
                    anyhow::bail!(
                        "trace line {line_no}: migration would truncate {}",
                        loss.field
                    );
                }
            }
            serde_json::to_writer(&mut out, &event)?;
            writeln!(out)?;
        }
    }
    out.flush()?;
    drop(out);
    staged.persist_noclobber(output)?;
    Ok(())
}

// Native events can refer to the same episode repeatedly. A legacy row contributes
// one whole episode and cannot share its identity with another row or native reference.
fn register(ids: &rusqlite::Connection, id: &str, native: bool) -> anyhow::Result<()> {
    let changed = ids.prepare_cached("INSERT INTO ids VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET native=1 WHERE ids.native AND excluded.native")?
        .execute(rusqlite::params![id, native])?;
    ensure!(
        changed == 1,
        "duplicate legacy request_id or conflicting episode or step ID"
    );
    Ok(())
}

fn convert_line(
    source: &str,
    ids: &rusqlite::Connection,
) -> anyhow::Result<Vec<ObservedTraceEvent>> {
    let mut value: Value = serde_json::from_str(source)?;
    let native = match &value {
        Value::Object(row) => row
            .get("type")
            .is_some_and(|t| t.as_str() != Some("assay.trace")),
        Value::Array(fields) => fields.first().is_some_and(Value::is_string),
        _ => false,
    };
    if native {
        let prior = decode_observations(value.get("observations"));
        let event: TraceEvent = serde_json::from_value(value)?;
        register(ids, event.episode_id(), true)?;
        let step = match &event {
            TraceEvent::Step(e) => Some(&e.step_id),
            TraceEvent::ToolCall(e) => Some(&e.step_id),
            _ => None,
        };
        if let Some(id) = step
            .and_then(|s| s.strip_suffix(LEGACY_STEP_SUFFIX))
            .filter(|s| *s != event.episode_id())
        {
            register(ids, id, true)?;
        }
        return Ok(vec![observe_event(event, prior)]);
    }
    let untagged = value.is_object() && value.get("type").is_none();
    if untagged {
        let parsed = crate::providers::trace::parse_legacy_record(&value);
        value["type"] = "assay.trace".into();
        value["prompt"] = parsed.prompt.into();
        value["response"] = parsed.response.into();
        value["meta"] = parsed.meta;
    }
    let response_key = legacy_response_key(&value);
    let missing_meta = match &value {
        Value::Object(row) => !row.contains_key("meta"),
        Value::Array(fields) => fields.len() == 5,
        _ => false,
    };
    let model = value.as_object_mut().and_then(|row| {
        row.entry("schema_version").or_insert(1.into());
        row.entry("request_id").or_insert_with(|| {
            format!("assay:legacy:sha256:{}", compute_sha256_str(source)).into()
        });
        if response_key == "text" {
            if let Some(text) = row.remove("text") {
                row.insert("response".into(), text);
            }
        }
        row.remove("model")
    });
    let mut v1: TraceEntryV1 = serde_json::from_value(value)?;
    ensure!(
        v1.entry_type == "assay.trace" && v1.schema_version == 1,
        "expected assay.trace version 1"
    );
    register(ids, &v1.request_id, false)?;
    if missing_meta {
        v1.meta = default_replay_meta();
    }
    let kind = if untagged {
        LegacyOutput::UntaggedMigration
    } else {
        LegacyOutput::NamedMigration
    };
    Ok(upgrade_v1_to_v2(v1, model, kind)
        .into_iter()
        .map(|e| observe_event(e, Vec::new()))
        .collect())
}
