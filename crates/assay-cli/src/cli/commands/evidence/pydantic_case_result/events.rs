use super::constants::{EVENT_SOURCE, EVENT_TYPE};
use super::reduce::reduce_case_result;
use anyhow::{bail, Context, Result};
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub(super) fn read_case_results(
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

    let file =
        File::open(input).with_context(|| format!("failed to open input {}", input.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut saw_jsonl_row = false;

    for (line_index, line_result) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line_result.with_context(|| format!("failed to read line {line_number}"))?;
        if line.trim().is_empty() {
            continue;
        }
        saw_jsonl_row = true;
        let row: Value = serde_json::from_str(&line)
            .with_context(|| format!("invalid JSONL object at line {line_number}"))?;
        let seq = events.len() as u64;
        let payload = reduce_case_result(
            &row,
            source_artifact_ref,
            source_artifact_digest,
            import_time,
            line_number,
        )?;
        let event = EvidenceEvent::new(EVENT_TYPE, EVENT_SOURCE, run_id, seq, payload)
            .with_time(import_time)
            .with_producer(producer);
        events.push(event);
    }

    if !saw_jsonl_row {
        bail!("input contains no JSONL rows");
    }

    Ok(events)
}
