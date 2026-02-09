use anyhow::Result;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use super::super::events::Event;

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub count: u32,
    pub first_seen: u64,
    pub last_seen: u64,
}

impl Stats {
    fn update(&mut self, ts: u64) {
        self.count += 1;
        if ts > 0 {
            if self.first_seen == 0 || ts < self.first_seen {
                self.first_seen = ts;
            }
            if ts > self.last_seen {
                self.last_seen = ts;
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct Aggregated {
    pub files: BTreeMap<String, Stats>,
    pub network: BTreeMap<String, Stats>,
    pub processes: BTreeMap<String, Stats>,
}

impl Aggregated {
    pub fn total(&self) -> usize {
        self.files.len() + self.network.len() + self.processes.len()
    }
}

pub fn read_events(path: &PathBuf) -> Result<Vec<Event>> {
    let reader: Box<dyn BufRead> = if path.to_string_lossy() == "-" {
        Box::new(BufReader::new(std::io::stdin()))
    } else {
        Box::new(BufReader::new(std::fs::File::open(path)?))
    };
    let mut events = Vec::new();
    let mut total_lines = 0;
    let mut error_count = 0;

    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        total_lines += 1;
        match serde_json::from_str(&line) {
            Ok(e) => events.push(e),
            Err(_) => {
                error_count += 1;
                if error_count <= 3 {
                    eprintln!("warning: skipping line {}: unparsable event", i + 1);
                }
            }
        }
    }

    if error_count > 3 {
        eprintln!("warning: skipped {} unparsable lines total", error_count);
    }

    if events.is_empty() && error_count > 0 {
        anyhow::bail!(
            "no valid events found ({} lines skipped, 0 ok)",
            error_count
        );
    }

    if total_lines > 0 {
        let error_rate = error_count as f64 / total_lines as f64;
        if error_rate > 0.5 {
            eprintln!(
                "warning: high error rate ({:.1}%) - check input format",
                error_rate * 100.0
            );
        }
    }

    Ok(events)
}

pub fn aggregate(events: &[Event]) -> Aggregated {
    let mut agg = Aggregated::default();
    for ev in events {
        match ev {
            Event::FileOpen {
                path, timestamp, ..
            } => agg
                .files
                .entry(path.clone())
                .or_default()
                .update(*timestamp),
            Event::NetConnect {
                dest, timestamp, ..
            } => agg
                .network
                .entry(dest.clone())
                .or_default()
                .update(*timestamp),
            Event::ProcExec {
                path, timestamp, ..
            } => agg
                .processes
                .entry(path.clone())
                .or_default()
                .update(*timestamp),
        }
    }
    agg
}
