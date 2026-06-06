use anyhow::Result;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    FileOpen {
        path: String,
        #[serde(default)]
        timestamp: u64,
    },
    NetConnect {
        dest: String,
        #[serde(default)]
        timestamp: u64,
    },
    ProcExec {
        path: String,
        #[serde(default)]
        timestamp: u64,
    },
}

pub(super) fn read_events(path: &PathBuf) -> Result<Vec<Event>> {
    let reader: Box<dyn BufRead> = if path.to_string_lossy() == "-" {
        Box::new(BufReader::new(std::io::stdin()))
    } else {
        Box::new(BufReader::new(std::fs::File::open(path)?))
    };

    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if let Ok(e) = serde_json::from_str(&line) {
            events.push(e);
        }
    }
    Ok(events)
}
