use crate::storage::Store;
use anyhow::Context;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Seek, Write};
use std::path::Path;

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
        ingest_into_store(&store, input)
    } else {
        // Open input stream
        let file = File::open(input).context("failed to open input file")?;
        assay_common::file_output::precheck_preserving_inputs(output, &[&file])?;
        let open_output = || {
            assay_common::file_output::open_preserving_inputs(output, &[&file])
                .context("failed to create output file")
        };
        // Preserve streaming to devices and pipes. A regular destination can wait
        // until the final source record has also passed validation.
        if output.metadata().is_ok_and(|m| !m.is_file()) {
            return write_events(&file, &mut open_output()?);
        }
        let (mut staged, stats) = stage_events(&file)?;
        std::io::copy(&mut staged, &mut open_output()?)?;
        Ok(stats)
    }
}

fn write_events(input: &File, output: &mut impl Write) -> anyhow::Result<IngestStats> {
    let mut count = 0;
    for event in super::upgrader::StreamUpgrader::new(BufReader::new(input)).observed() {
        let observed = event.context("failed to process trace entry")?;
        super::migration::write_event_line(output, &observed)?;
        count += 1;
    }
    Ok(IngestStats { event_count: count })
}

// Stage the observed events, so SQLite reads the same truncation observations
// as JSONL without running the upgrader on the converted content a second time.
fn stage_events(input: &File) -> anyhow::Result<(File, IngestStats)> {
    let mut staged = tempfile::tempfile()?;
    let mut writer = BufWriter::new(&mut staged);
    let stats = write_events(input, &mut writer)?;
    writer.flush()?;
    drop(writer);
    staged.rewind()?;
    Ok((staged, stats))
}

pub fn ingest_into_store(
    store: &crate::storage::store::Store,
    input: &Path,
) -> anyhow::Result<IngestStats> {
    // Open input stream
    let file = File::open(input).context("failed to open input file")?;
    if file.metadata()?.is_file() {
        // SQLite resolves URI names itself. Compare its main database with the trace before
        // creating tables: an empty trace is also a valid, uninitialized SQLite database.
        // The first PRAGMA row is main, and its TEXT bytes preserve Unix filenames.
        let database: Vec<u8> =
            store
                .conn
                .lock()
                .unwrap()
                .query_row("PRAGMA database_list", [], |row| {
                    Ok(row.get_ref(2)?.as_bytes()?.to_vec())
                })?;
        #[cfg(unix)]
        let database = {
            use std::os::unix::ffi::OsStrExt;
            Path::new(std::ffi::OsStr::from_bytes(&database))
        };
        #[cfg(not(unix))]
        let database = Path::new(std::str::from_utf8(&database)?);
        if !database.as_os_str().is_empty() {
            assay_common::file_output::check_regular_output(database, &file)?;
        }
    }
    let (staged, stats) = stage_events(&file)?;

    // Ensure schema (idempotent) - caller usually does this but safe to repeat
    store.init_schema()?;
    let mut conn = store.conn.lock().unwrap();
    let tx = conn.transaction()?;
    Store::clear_eval_scope(&tx)?;
    for line in BufReader::new(staged).lines() {
        let observed = super::observation::parse_observed_line(&line?)?;
        let id = observed.event().episode_id();
        if Store::mark_ingested_episode(&tx, id)? {
            Store::purge_episode_in_tx(&tx, id)?;
        }
        Store::insert_observed_in_tx(&tx, &observed, None, None)?;
    }
    tx.commit()?;
    Ok(stats)
}
