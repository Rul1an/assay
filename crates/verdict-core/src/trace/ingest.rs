use anyhow::Context;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

pub fn ingest_file(input: &Path, output: &Path) -> anyhow::Result<()> {
    // Open input stream
    let file = File::open(input).context("failed to open input file")?;
    let reader = BufReader::new(file);

    // Use Upgrader to stream events (V1->V2 or V2 passthrough)
    let upgrader = super::upgrader::StreamUpgrader::new(reader);

    // Check output format
    let is_sqlite = output
        .extension()
        .is_some_and(|ext| ext == "db" || ext == "sqlite");

    if is_sqlite {
        let store = crate::storage::store::Store::open(output)?;
        store.init_schema()?;
        let mut batch = Vec::with_capacity(1000);

        for event_result in upgrader {
            let event = event_result.context("failed to process trace entry")?;
            batch.push(event);

            if batch.len() >= 1000 {
                store.insert_batch(&batch, None, None)?;
                batch.clear();
            }
        }
        if !batch.is_empty() {
            store.insert_batch(&batch, None, None)?;
        }
    } else {
        // JSONL Output
        let mut out_file = File::create(output).context("failed to create output file")?;
        for event_result in upgrader {
            let event = event_result.context("failed to process trace entry")?;
            let out_line = serde_json::to_string(&event)?;
            writeln!(out_file, "{}", out_line)?;
        }
    }

    Ok(())
}
