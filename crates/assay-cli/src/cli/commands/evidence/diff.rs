use anyhow::{Context, Result};
use assay_evidence::diff::engine::diff_bundles;
use assay_evidence::VerifyLimits;
use clap::Args;
use std::fs::File;

#[derive(Debug, Args, Clone)]
pub struct DiffArgs {
    /// Baseline bundle (or candidate if --baseline-dir is used)
    #[arg(value_name = "BASELINE")]
    pub baseline: std::path::PathBuf,

    /// Candidate bundle
    #[arg(value_name = "CANDIDATE")]
    pub candidate: Option<std::path::PathBuf>,

    /// Output format: human or json
    #[arg(long, default_value = "human")]
    pub format: String,

    /// Baseline directory (look for {dir}/{key}.tar.gz)
    #[arg(long)]
    pub baseline_dir: Option<std::path::PathBuf>,

    /// Baseline key name (used with --baseline-dir)
    #[arg(long)]
    pub key: Option<String>,
}

pub fn cmd_diff(args: DiffArgs) -> Result<i32> {
    let (baseline_path, candidate_path) = resolve_paths(&args)?;

    let baseline_file = File::open(&baseline_path)
        .with_context(|| format!("failed to open baseline {}", baseline_path.display()))?;
    let candidate_file = File::open(&candidate_path)
        .with_context(|| format!("failed to open candidate {}", candidate_path.display()))?;

    let limits = VerifyLimits::default();
    let report = diff_bundles(baseline_file, candidate_file, limits)?;

    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        _ => {
            eprintln!("Assay Evidence Diff");
            eprintln!("===================");
            eprintln!(
                "Baseline:  {} ({} events)",
                report.baseline.run_id, report.baseline.event_count
            );
            eprintln!(
                "Candidate: {} ({} events)",
                report.candidate.run_id, report.candidate.event_count
            );
            eprintln!("Event count delta: {:+}", report.summary.event_count_delta);
            eprintln!();

            print_diff_set("Network", &report.network);
            print_diff_set("Filesystem", &report.filesystem);
            print_diff_set("Processes", &report.processes);

            if report.is_empty() {
                eprintln!("No differences found.");
            }
        }
    }

    Ok(0)
}

fn resolve_paths(args: &DiffArgs) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    if let (Some(dir), Some(key)) = (&args.baseline_dir, &args.key) {
        // --baseline-dir mode: first positional arg is the candidate
        let baseline_path = dir.join(format!("{}.tar.gz", key));
        let candidate_path = args.baseline.clone();
        Ok((baseline_path, candidate_path))
    } else {
        // Two-positional mode
        let candidate = args
            .candidate
            .as_ref()
            .context("candidate bundle path is required (or use --baseline-dir + --key)")?;
        Ok((args.baseline.clone(), candidate.clone()))
    }
}

fn print_diff_set(category: &str, diff: &assay_evidence::diff::DiffSet) {
    if diff.is_empty() {
        return;
    }
    eprintln!("{}:", category);
    for added in &diff.added {
        eprintln!("  + {}", added);
    }
    for removed in &diff.removed {
        eprintln!("  - {}", removed);
    }
    eprintln!();
}
