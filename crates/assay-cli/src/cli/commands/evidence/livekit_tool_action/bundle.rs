use super::constants::{EVENT_SOURCE, EVENT_TYPE};
use super::input::parse_input_documents;
use super::reduce::reduce_tool_action_event;
use anyhow::{bail, Context, Result};
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, Utc};
use std::path::Path;

pub(super) fn read_livekit_tool_actions(
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
