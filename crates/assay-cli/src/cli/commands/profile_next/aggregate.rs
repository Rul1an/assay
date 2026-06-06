use std::collections::BTreeMap;

use super::Event;
use crate::cli::commands::profile_types::{Profile, ProfileEntry};

#[derive(Debug, Default)]
pub(super) struct RunData {
    pub(super) files: BTreeMap<String, RunEntry>,
    pub(super) network: BTreeMap<String, RunEntry>,
    pub(super) processes: BTreeMap<String, RunEntry>,
}

#[derive(Debug, Default)]
pub(super) struct RunEntry {
    pub(super) timestamp: u64,
    pub(super) hits: u64,
}

pub(super) fn aggregate_run(events: &[Event]) -> RunData {
    let mut data = RunData::default();

    for ev in events {
        match ev {
            Event::FileOpen { path, timestamp } => {
                let e = data.files.entry(path.clone()).or_default();
                e.hits += 1;
                if *timestamp > e.timestamp {
                    e.timestamp = *timestamp;
                }
            }
            Event::NetConnect { dest, timestamp } => {
                let e = data.network.entry(dest.clone()).or_default();
                e.hits += 1;
                if *timestamp > e.timestamp {
                    e.timestamp = *timestamp;
                }
            }
            Event::ProcExec { path, timestamp } => {
                let e = data.processes.entry(path.clone()).or_default();
                e.hits += 1;
                if *timestamp > e.timestamp {
                    e.timestamp = *timestamp;
                }
            }
        }
    }

    data
}

pub(super) fn merge_run(profile: &mut Profile, run: &RunData) -> (usize, usize) {
    let mut new_count = 0;
    let mut updated_count = 0;

    // Merge files
    for (key, run_entry) in &run.files {
        if let Some(entry) = profile.entries.files.get_mut(key) {
            entry.merge_run(run_entry.timestamp, run_entry.hits);
            updated_count += 1;
        } else {
            profile.entries.files.insert(
                key.clone(),
                ProfileEntry::new(run_entry.timestamp, run_entry.hits),
            );
            new_count += 1;
        }
    }

    // Merge network
    for (key, run_entry) in &run.network {
        if let Some(entry) = profile.entries.network.get_mut(key) {
            entry.merge_run(run_entry.timestamp, run_entry.hits);
            updated_count += 1;
        } else {
            profile.entries.network.insert(
                key.clone(),
                ProfileEntry::new(run_entry.timestamp, run_entry.hits),
            );
            new_count += 1;
        }
    }

    // Merge processes
    for (key, run_entry) in &run.processes {
        if let Some(entry) = profile.entries.processes.get_mut(key) {
            entry.merge_run(run_entry.timestamp, run_entry.hits);
            updated_count += 1;
        } else {
            profile.entries.processes.insert(
                key.clone(),
                ProfileEntry::new(run_entry.timestamp, run_entry.hits),
            );
            new_count += 1;
        }
    }

    (new_count, updated_count)
}
