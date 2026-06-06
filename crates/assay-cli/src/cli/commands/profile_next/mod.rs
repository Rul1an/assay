//! Profile commands for multi-run stability analysis (Phase 3)
//!
//! # Usage
//! ```bash
//! assay profile init --output profile.yaml --name my-app
//! assay profile update --profile profile.yaml -i trace.jsonl --run-id ci-123
//! assay profile show --profile profile.yaml
//! ```

mod aggregate;
mod display;
mod input;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Instant;

use crate::cli::commands::pipeline_error::elapsed_ms;
use crate::cli::commands::profile_types::*;
use aggregate::{aggregate_run, merge_run};
use display::show_summary;
use input::read_events;

pub use input::Event;

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

pub fn run(args: ProfileArgs) -> Result<i32> {
    match args.cmd {
        ProfileCmd::Init(a) => cmd_init(a),
        ProfileCmd::Update(a) => cmd_update(a),
        ProfileCmd::Show(a) => cmd_show(a),
    }
}

#[derive(Debug, Serialize)]
struct ProfilePerfMetrics {
    load_profile_ms: u64,
    read_events_ms: u64,
    aggregate_ms: u64,
    merge_ms: u64,
    save_profile_ms: u64,
    profile_store_ms: u64,
    total_ms: u64,
    run_entries: usize,
    run_id_window_len: usize,
    run_id_digest_window_len: usize,
    run_id_memory_bytes: u64,
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

fn enforce_scope(profile: &mut Profile, new_scope: Option<&String>, force: bool) -> Result<()> {
    // Hard Scope Guard (SOTA: prevent pollution from different configs)
    if let Some(ref current_scope) = profile.scope {
        if let Some(scope) = new_scope {
            if current_scope != scope {
                if force {
                    eprintln!(
                        "WARNING: Scope mismatch (profile='{}', update='{}'). Forcing update.",
                        current_scope, scope
                    );
                } else {
                    anyhow::bail!(
                        "Scope mismatch: profile scope is '{}' but update scope is '{}'. \
                        This prevents accidentally merging runs from different configurations. \
                        Use --force to override.",
                        current_scope,
                        scope
                    );
                }
            }
        }
    } else if let Some(scope) = new_scope {
        // First time seeing a scope -> lock it
        eprintln!("Setting profile scope to '{}'", scope);
        profile.scope = Some(scope.clone());
    }
    Ok(())
}

fn cmd_update(args: UpdateArgs) -> Result<i32> {
    let total_start = Instant::now();

    // Load existing profile
    let load_start = Instant::now();
    let mut profile = load_profile(&args.profile)
        .with_context(|| format!("failed to load profile: {}", args.profile.display()))?;
    let load_profile_ms = elapsed_ms(load_start);

    // Enforce scope guard
    enforce_scope(&mut profile, args.scope.as_ref(), args.force)?;

    // Idempotency check
    if profile.has_run(&args.run_id) {
        if args.strict {
            anyhow::bail!("run_id '{}' already merged (strict mode)", args.run_id);
        }
        eprintln!("Skipping: run_id '{}' already merged", args.run_id);
        return Ok(0);
    }

    // Read events
    let read_start = Instant::now();
    let events = read_events(&args.input)?;
    let read_events_ms = elapsed_ms(read_start);
    if events.is_empty() {
        eprintln!("Warning: no events in input");
    }

    // Aggregate this run (deduplicated per artifact)
    let aggregate_start = Instant::now();
    let run_data = aggregate_run(&events);
    let aggregate_ms = elapsed_ms(aggregate_start);
    let run_entries = run_data.files.len() + run_data.network.len() + run_data.processes.len();

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
    let merge_start = Instant::now();
    let (new_count, updated_count) = merge_run(&mut profile, &run_data);
    let merge_ms = elapsed_ms(merge_start);

    // Update metadata
    profile.total_runs += 1;
    let run_id_digest_evicted = profile.add_run_id(args.run_id.clone());
    profile.updated_at = chrono::Utc::now().to_rfc3339();

    // Save
    let save_start = Instant::now();
    save_profile(&profile, &args.profile)?;
    let save_profile_ms = elapsed_ms(save_start);

    let profile_store_ms = load_profile_ms
        .saturating_add(merge_ms)
        .saturating_add(save_profile_ms);
    let total_ms = elapsed_ms(total_start);
    let perf = ProfilePerfMetrics {
        load_profile_ms,
        read_events_ms,
        aggregate_ms,
        merge_ms,
        save_profile_ms,
        profile_store_ms,
        total_ms,
        run_entries,
        run_id_window_len: profile.run_ids.len(),
        run_id_digest_window_len: profile.run_id_digests.len(),
        run_id_memory_bytes: profile.run_id_memory_bytes_estimate(),
    };

    eprintln!(
        "Updated profile: {} total runs, {} new entries, {} updated",
        profile.total_runs, new_count, updated_count
    );

    if args.verbose || profile_store_ms >= 500 {
        eprintln!(
            "profile-perf: load={}ms read={}ms aggregate={}ms merge={}ms save={}ms store={}ms total={}ms entries={}",
            perf.load_profile_ms,
            perf.read_events_ms,
            perf.aggregate_ms,
            perf.merge_ms,
            perf.save_profile_ms,
            perf.profile_store_ms,
            perf.total_ms,
            perf.run_entries
        );
    }
    if perf.load_profile_ms > 500 {
        eprintln!(
            "WARNING: profile load is slow ({}ms > 500ms trigger)",
            perf.load_profile_ms
        );
    }
    if perf.merge_ms > 1_000 {
        eprintln!(
            "WARNING: profile merge is slow ({}ms > 1000ms trigger)",
            perf.merge_ms
        );
    }
    if run_id_digest_evicted {
        eprintln!(
            "WARNING: run-id digest window is full ({} entries); old run-id dedupe evidence will be evicted over time",
            perf.run_id_digest_window_len
        );
    }

    if let Ok(path) = std::env::var("ASSAY_PROFILE_PERF_JSON") {
        let json = serde_json::to_string_pretty(&perf)?;
        std::fs::write(&path, json)
            .with_context(|| format!("failed to write profile perf json: {}", path))?;
        eprintln!("Wrote profile perf metrics: {}", path);
    }

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

#[cfg(test)]
mod tests;
