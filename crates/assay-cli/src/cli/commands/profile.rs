//! Profile commands for multi-run stability analysis (Phase 3)
//!
//! # Usage
//! ```bash
//! assay profile init --output profile.yaml --name my-app
//! assay profile update --profile profile.yaml -i trace.jsonl --run-id ci-123
//! assay profile show --profile profile.yaml
//! ```

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::profile_types::*;

// ─────────────────────────────────────────────────────────────────────────────
// CLI Args
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Args, Debug, Clone)]
#[command(about = "Manage multi-run profiles for stability analysis")]
pub struct ProfileArgs {
    #[command(subcommand)]
    pub cmd: ProfileCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ProfileCmd {
    /// Initialize a new profile
    Init(InitArgs),
    /// Update profile with a new run
    Update(UpdateArgs),
    /// Show profile summary
    Show(ShowArgs),
}

#[derive(Args, Debug, Clone)]
pub struct InitArgs {
    #[arg(short, long, default_value = "assay-profile.yaml")]
    pub output: PathBuf,

    #[arg(long, default_value = "default")]
    pub name: String,

    /// Scope fingerprint (config hash, suite name)
    #[arg(long)]
    pub scope: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct UpdateArgs {
    #[arg(long)]
    pub profile: PathBuf,

    #[arg(short, long)]
    pub input: PathBuf,

    /// Idempotency key (required) - e.g. CI run id
    #[arg(long)]
    pub run_id: String,

    /// Fail if run_id already merged
    #[arg(long)]
    pub strict: bool,

    /// Scope fingerprint check (prevents pollution)
    #[arg(long)]
    pub scope: Option<String>,

    /// Force update even if scope mismatch
    #[arg(long)]
    pub force: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Args, Debug, Clone)]
pub struct ShowArgs {
    #[arg(long)]
    pub profile: PathBuf,

    /// Output format: summary, yaml, json
    #[arg(long, default_value = "summary")]
    pub format: String,

    /// Show top N entries per category
    #[arg(long, default_value_t = 10)]
    pub top: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// Event Types (reuse from generate.rs or define here)
// ─────────────────────────────────────────────────────────────────────────────

use serde::Deserialize;

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

fn read_events(path: &PathBuf) -> Result<Vec<Event>> {
    use std::io::{BufRead, BufReader};

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

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

pub fn run(args: ProfileArgs) -> Result<i32> {
    match args.cmd {
        ProfileCmd::Init(a) => cmd_init(a),
        ProfileCmd::Update(a) => cmd_update(a),
        ProfileCmd::Show(a) => cmd_show(a),
    }
}

fn cmd_init(args: InitArgs) -> Result<i32> {
    if args.output.exists() {
        anyhow::bail!("profile already exists: {}", args.output.display());
    }

    let profile = Profile::new(&args.name, args.scope);
    save_profile(&profile, &args.output)?;

    eprintln!("Created profile: {}", args.output.display());
    Ok(0)
}

fn cmd_update(args: UpdateArgs) -> Result<i32> {
    // Load existing profile
    let mut profile = load_profile(&args.profile)
        .with_context(|| format!("failed to load profile: {}", args.profile.display()))?;

    // Hard Scope Guard (SOTA: prevent pollution from different configs)
    if let Some(ref current_scope) = profile.scope {
        if let Some(ref new_scope) = args.scope {
            if current_scope != new_scope {
                if args.force {
                    eprintln!(
                        "WARNING: Scope mismatch (profile='{}', update='{}'). Forcing update.",
                        current_scope, new_scope
                    );
                } else {
                    anyhow::bail!(
                        "Scope mismatch: profile scope is '{}' but update scope is '{}'. \
                        This prevents accidentally merging runs from different configurations. \
                        Use --force to override.",
                        current_scope,
                        new_scope
                    );
                }
            }
        }
    } else if let Some(ref new_scope) = args.scope {
        // First time seeing a scope -> lock it
        eprintln!("Setting profile scope to '{}'", new_scope);
        profile.scope = Some(new_scope.clone());
    }

    // Idempotency check
    if profile.has_run(&args.run_id) {
        if args.strict {
            anyhow::bail!("run_id '{}' already merged (strict mode)", args.run_id);
        }
        eprintln!("Skipping: run_id '{}' already merged", args.run_id);
        return Ok(0);
    }

    // Read events
    let events = read_events(&args.input)?;
    if events.is_empty() {
        eprintln!("Warning: no events in input");
    }

    // Aggregate this run (deduplicated per artifact)
    let run_data = aggregate_run(&events);

    if args.verbose {
        eprintln!(
            "Run {}: {} files, {} network, {} processes",
            args.run_id,
            run_data.files.len(),
            run_data.network.len(),
            run_data.processes.len()
        );
    }

    // Merge into profile
    let (new_count, updated_count) = merge_run(&mut profile, &run_data);

    // Update metadata
    profile.total_runs += 1;
    profile.add_run_id(args.run_id.clone());
    profile.updated_at = chrono::Utc::now().to_rfc3339();

    // Save
    save_profile(&profile, &args.profile)?;

    eprintln!(
        "Updated profile: {} total runs, {} new entries, {} updated",
        profile.total_runs, new_count, updated_count
    );

    Ok(0)
}

fn cmd_show(args: ShowArgs) -> Result<i32> {
    let profile = load_profile(&args.profile)?;

    match args.format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&profile)?),
        "yaml" => println!("{}", serde_yaml::to_string(&profile)?),
        _ => show_summary(&profile, args.top),
    }

    Ok(0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Aggregation & Merge
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct RunData {
    files: BTreeMap<String, RunEntry>,
    network: BTreeMap<String, RunEntry>,
    processes: BTreeMap<String, RunEntry>,
}

#[derive(Debug, Default)]
struct RunEntry {
    timestamp: u64,
    hits: u64,
}

fn aggregate_run(events: &[Event]) -> RunData {
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

fn merge_run(profile: &mut Profile, run: &RunData) -> (usize, usize) {
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

// ─────────────────────────────────────────────────────────────────────────────
// Summary Display
// ─────────────────────────────────────────────────────────────────────────────

fn show_summary(profile: &Profile, top_n: usize) {
    println!("Profile: {}", profile.name);
    println!("Version: {}", profile.version);
    if let Some(scope) = &profile.scope {
        println!("Scope: {}", scope);
    }
    println!("Created: {}", profile.created_at);
    println!("Updated: {}", profile.updated_at);
    println!("Total runs: {}", profile.total_runs);
    println!();
    println!("Entries:");
    println!("  Files: {}", profile.entries.files.len());
    println!("  Network: {}", profile.entries.network.len());
    println!("  Processes: {}", profile.entries.processes.len());
    println!();

    if profile.total_runs > 0 {
        println!("Stability distribution (α=1.0):");
        show_stability_distribution(&profile.entries.files, profile.total_runs, "  Files");
        show_stability_distribution(&profile.entries.network, profile.total_runs, "  Network");
        show_stability_distribution(
            &profile.entries.processes,
            profile.total_runs,
            "  Processes",
        );
        println!();

        println!("Top {} most stable files:", top_n);
        show_top_stable(&profile.entries.files, profile.total_runs, top_n);

        if !profile.entries.network.is_empty() {
            println!("\nTop {} most stable network destinations:", top_n);
            show_top_stable(&profile.entries.network, profile.total_runs, top_n);
        }
    }
}

fn show_stability_distribution(
    entries: &BTreeMap<String, ProfileEntry>,
    total_runs: u32,
    label: &str,
) {
    if entries.is_empty() {
        return;
    }

    let mut high = 0; // >= 0.8
    let mut mid = 0; // 0.6-0.8
    let mut low = 0; // < 0.6

    for entry in entries.values() {
        let s = stability_smoothed(entry.runs_seen, total_runs, DEFAULT_ALPHA);
        if s >= 0.8 {
            high += 1;
        } else if s >= 0.6 {
            mid += 1;
        } else {
            low += 1;
        }
    }

    println!(
        "{}: {} stable (≥0.8), {} medium (0.6-0.8), {} low (<0.6)",
        label, high, mid, low
    );
}

fn show_top_stable(entries: &BTreeMap<String, ProfileEntry>, total_runs: u32, n: usize) {
    let mut sorted: Vec<_> = entries
        .iter()
        .map(|(k, v)| {
            (
                k,
                v,
                stability_smoothed(v.runs_seen, total_runs, DEFAULT_ALPHA),
            )
        })
        .collect();

    sorted.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

    for (key, entry, stab) in sorted.into_iter().take(n) {
        let key_short = if key.len() > 50 { &key[..50] } else { key };
        println!(
            "  {:.2} ({:>2}/{:>2}) {}",
            stab, entry.runs_seen, total_runs, key_short
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_dedup() {
        let events = vec![
            Event::FileOpen {
                path: "/a".into(),
                timestamp: 100,
            },
            Event::FileOpen {
                path: "/a".into(),
                timestamp: 200,
            },
            Event::FileOpen {
                path: "/b".into(),
                timestamp: 150,
            },
        ];
        let run = aggregate_run(&events);
        assert_eq!(run.files.len(), 2);
        assert_eq!(run.files["/a"].hits, 2);
        assert_eq!(run.files["/a"].timestamp, 200);
    }

    #[test]
    fn merge_new_entries() {
        let mut profile = Profile::new("test", None);
        let events = vec![Event::FileOpen {
            path: "/a".into(),
            timestamp: 100,
        }];
        let run = aggregate_run(&events);
        let (new, updated) = merge_run(&mut profile, &run);

        assert_eq!(new, 1);
        assert_eq!(updated, 0);
        assert_eq!(profile.entries.files["/a"].runs_seen, 1);
    }

    #[test]
    fn merge_existing_entries() {
        let mut profile = Profile::new("test", None);
        profile
            .entries
            .files
            .insert("/a".into(), ProfileEntry::new(100, 5));

        let events = vec![
            Event::FileOpen {
                path: "/a".into(),
                timestamp: 200,
            },
            Event::FileOpen {
                path: "/a".into(),
                timestamp: 200,
            },
        ];
        let run = aggregate_run(&events);
        let (new, updated) = merge_run(&mut profile, &run);

        assert_eq!(new, 0);
        assert_eq!(updated, 1);
        assert_eq!(profile.entries.files["/a"].runs_seen, 2);
        assert_eq!(profile.entries.files["/a"].hits_total, 7); // 5 + 2
    }
}
