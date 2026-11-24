//! Opt-in conversion of legacy replay rows to native JSONL.
use super::observation::{decode_observations, ObservedTraceEvent};
use super::schema::{default_replay_meta, legacy_response_key, TraceEntryV1, TraceEvent};
use super::truncation::compute_sha256_str;
use super::upgrader::{observe_event, upgrade_v1_to_v2, LEGACY_STEP_SUFFIX};
use anyhow::ensure;
use serde_json::Value;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

pub(super) const MAX_LINE_BYTES: u64 = 16 * 1024 * 1024;

// Charge the newline to the same limit as the reader, including for escaped JSON.
struct EventLineWriter<'a, W> {
    inner: &'a mut W,
    remaining: u64,
}

impl<W: Write> Write for EventLineWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other("output line exceeds 16 MiB"));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub(super) fn write_event_line(
    out: &mut impl Write,
    event: &ObservedTraceEvent,
) -> anyhow::Result<()> {
    let mut line = EventLineWriter {
        inner: out,
        remaining: MAX_LINE_BYTES,
    };
    serde_json::to_writer(&mut line, event)?;
    line.write_all(b"\n")?;
    Ok(())
}

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
    for event in super::upgrader::StreamUpgrader::for_migration(BufReader::new(input)).observed() {
        let event = event?;
        ensure_lossless(&event)?;
        write_event_line(&mut out, &event)?;
    }
    out.flush()?;
    drop(out);
    staged.persist_noclobber(output)?;
    Ok(())
}

// Scanned events end with the measurement made by this conversion.
fn ensure_lossless(event: &ObservedTraceEvent) -> anyhow::Result<()> {
    if !super::upgrader::declared_field_roots(event.event()).is_empty() {
        if let Some(loss) = event.observations().last().and_then(|o| o.losses.first()) {
            anyhow::bail!("migration would truncate {}", loss.field);
        }
    }
    Ok(())
}

pub(super) fn open_ids() -> rusqlite::Result<rusqlite::Connection> {
    let db = rusqlite::Connection::open("")?;
    db.execute_batch(
        "CREATE TABLE ids (id TEXT PRIMARY KEY, native INTEGER NOT NULL) WITHOUT ROWID; BEGIN",
    )?;
    Ok(db)
}

// These can identify the original full fields of an already-shortened native event.
fn recorded_digests(event: &TraceEvent) -> [Option<&str>; 2] {
    match event {
        TraceEvent::Step(step) => [step.content_sha256.as_deref(), None],
        TraceEvent::ToolCall(call) => [call.args_sha256.as_deref(), call.result_sha256.as_deref()],
        _ => [None, None],
    }
}

fn observe_for_migration(
    event: TraceEvent,
    prior: Vec<super::observation::TruncationObservation>,
) -> anyhow::Result<ObservedTraceEvent> {
    let before = recorded_digests(&event).map(|hash| hash.map(str::to_owned));
    let observed = observe_event(event, prior);
    for (old, new) in before.iter().zip(recorded_digests(observed.event())) {
        ensure!(
            old.is_none() || old.as_deref() == new,
            "migration would replace a recorded digest"
        );
    }
    Ok(observed)
}

// Native events can refer to the same episode repeatedly. A legacy row contributes
// one whole episode and cannot share its identity with another row or native reference.
fn register(ids: &rusqlite::Connection, id: &str, native: bool) -> anyhow::Result<()> {
    let changed = ids.prepare_cached("INSERT INTO ids VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET native=1 WHERE ids.native AND excluded.native")?
        .execute(rusqlite::params![id, native])?;
    ensure!(
        changed == 1,
        "duplicate legacy request_id or conflicting derived or recorded episode or step ID"
    );
    Ok(())
}

pub(super) fn convert_line(
    source: &str,
    ids: &rusqlite::Connection,
    preserve_native_digests: bool,
) -> anyhow::Result<Vec<ObservedTraceEvent>> {
    let mut value: Value = serde_json::from_str(source)?;
    let native = match &value {
        // A string `type` is recognized by the rule replay uses. A present non-string `type`
        // keeps the native route, where deserializing the event refuses it.
        Value::Object(row) => {
            crate::trace::schema::native_event_type(&value).is_some()
                || row.get("type").is_some_and(|t| !t.is_string())
        }
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
        let observed = if preserve_native_digests {
            observe_for_migration(event, prior)?
        } else {
            observe_event(event, prior)
        };
        return Ok(vec![observed]);
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
    upgrade_v1_to_v2(v1, model, untagged)
        .into_iter()
        .map(|event| {
            let observed = observe_event(event, Vec::new());
            ensure_lossless(&observed)?;
            Ok(observed)
        })
        .collect()
}
