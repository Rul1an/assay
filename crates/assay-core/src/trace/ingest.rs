use anyhow::Context;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

use super::schema::TraceEvent;

pub struct IngestStats {
    pub event_count: usize,
}

pub fn ingest_file(input: &Path, output: &Path) -> anyhow::Result<IngestStats> {
    // Check output format
    let is_sqlite = output.to_str() == Some(":memory:")
        || output
            .extension()
            .is_some_and(|ext| ext == "db" || ext == "sqlite");

    if is_sqlite {
        let store = crate::storage::store::Store::open(output)?;
        store.init_schema()?;
        ingest_into_store(&store, input)
    } else {
        // Open input stream
        let file = File::open(input).context("failed to open input file")?;
        let reader = BufReader::new(file);

        // Use Upgrader to stream events (V1->V2 or V2 passthrough)
        let upgrader = super::upgrader::StreamUpgrader::new(reader).observed();

        // JSONL Output
        let mut out_file = File::create(output).context("failed to create output file")?;
        let mut count = 0;
        for event_result in upgrader {
            let observed = event_result.context("failed to process trace entry")?;
            let out_line = serde_json::to_string(&observed)?;
            writeln!(out_file, "{}", out_line)?;
            count += 1;
        }
        Ok(IngestStats { event_count: count })
    }
}

pub fn ingest_into_store(
    store: &crate::storage::store::Store,
    input: &Path,
) -> anyhow::Result<IngestStats> {
    // Open input stream
    let file = File::open(input).context("failed to open input file")?;
    let reader = BufReader::new(file);

    // Use Upgrader to stream events (V1->V2 or V2 passthrough)
    let upgrader = super::upgrader::StreamUpgrader::new(reader).observed();

    let mut count = 0;

    // Ensure schema (idempotent) - caller usually does this but safe to repeat
    store.init_schema()?;
    store.clear_assertion_eval_scope()?;
    let mut batch = Vec::with_capacity(1000);
    let mut purged = HashSet::new();

    for event_result in upgrader {
        let observed = event_result.context("failed to process trace entry")?;
        let episode_id = event_episode_id(observed.event()).to_string();
        if purged.insert(episode_id.clone()) {
            store.purge_episode(&episode_id)?;
            store.record_assertion_eval_episode(&episode_id)?;
        }
        batch.push(observed);
        count += 1;

        if batch.len() >= 1000 {
            store.insert_observed_batch(&batch, None, None)?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        store.insert_observed_batch(&batch, None, None)?;
    }

    Ok(IngestStats { event_count: count })
}

fn event_episode_id(event: &TraceEvent) -> &str {
    match event {
        TraceEvent::EpisodeStart(e) => &e.episode_id,
        TraceEvent::Step(e) => &e.episode_id,
        TraceEvent::ToolCall(e) => &e.episode_id,
        TraceEvent::EpisodeEnd(e) => &e.episode_id,
    }
}
