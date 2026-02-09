//! Learning Mode: Generate policy
//!
//! # Usage
//! ```bash
//! # Single-run mode
//! assay generate -i trace.jsonl --heuristics
//!
//! # Profile mode (stability analysis)
//! assay generate --profile profile.yaml --min-stability 0.8
//! assay generate --profile profile.yaml --min-stability 0.8 --new-is-risky
//! ```

use anyhow::Result;

use super::heuristics::HeuristicsConfig;
use super::profile_types;

mod args;
mod diff;
mod ingest;
mod model;
mod profile;
mod single_run;

pub use args::GenerateArgs;
pub use ingest::{aggregate, read_events};
pub use model::{serialize, Policy};
pub use profile::generate_from_profile;
pub use single_run::generate_from_trace;

// ─────────────────────────────────────────────────────────────────────────────
// Entry Point
// ─────────────────────────────────────────────────────────────────────────────

pub fn run(args: GenerateArgs) -> Result<i32> {
    // Validate: need either --input or --profile
    if args.input.is_none() && args.profile.is_none() {
        anyhow::bail!("specify either --input (single-run) or --profile (multi-run)");
    }
    if args.input.is_some() && args.profile.is_some() {
        anyhow::bail!("cannot use both --input and --profile");
    }

    args.validate()?;

    let heur_cfg = HeuristicsConfig {
        entropy_threshold: args.entropy_threshold,
        ..Default::default()
    };

    let policy = if let Some(profile_path) = &args.profile {
        // Profile mode (Phase 3)
        let profile = profile_types::load_profile(profile_path)?;
        eprintln!(
            "Loaded profile: {} runs, {} entries",
            profile.total_runs,
            profile.total_entries()
        );
        generate_from_profile(&args.name, &profile, &args, &heur_cfg)
    } else {
        // Single-run mode (Phase 2)
        let input = args.input.as_ref().unwrap();
        let events = read_events(input)?;
        let agg = aggregate(&events);
        eprintln!(
            "Aggregated {} unique from {} events",
            agg.total(),
            events.len()
        );
        generate_from_trace(&args.name, &agg, args.heuristics, &heur_cfg)
    };

    if args.diff {
        let old_policy = if args.output.exists() {
            match diff::parse_existing_policy(&args.output) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!(
                        "warning: failed to parse existing policy at {} ({}); diffing against empty policy",
                        args.output.display(),
                        e
                    );
                    Policy::default()
                }
            }
        } else {
            Policy::default()
        };
        let diff = diff::diff_policies(&old_policy, &policy);
        diff::print_policy_diff(&diff, &args.output);
    }

    // Report
    let allow_count = policy.files.allow.len()
        + policy.network.allow_destinations.len()
        + policy.processes.allow.len();
    let review_count = policy.files.needs_review.len()
        + policy.network.needs_review.len()
        + policy.processes.needs_review.len();
    let deny_count = policy.files.deny.len()
        + policy.network.deny_destinations.len()
        + policy.processes.deny.len();
    eprintln!(
        "Policy: {} allow, {} needs_review, {} deny",
        allow_count, review_count, deny_count
    );

    let output = serialize(&policy, &args.format)?;

    if args.dry_run {
        println!("{}", output);
    } else {
        std::fs::write(&args.output, &output)?;
        eprintln!("Wrote {}", args.output.display());
    }

    Ok(0)
}
