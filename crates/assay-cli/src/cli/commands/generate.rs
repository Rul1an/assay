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

use super::heuristics::{self, HeuristicsConfig};
use super::profile_types;

mod args;
mod diff;
mod ingest;
mod model;
mod profile;

pub use args::GenerateArgs;
pub use ingest::{aggregate, read_events, Aggregated};
pub use model::{serialize, Entry, Meta, NetSection, Policy, Section};
pub use profile::generate_from_profile;

// ─────────────────────────────────────────────────────────────────────────────
// Generate from Single Run
// ─────────────────────────────────────────────────────────────────────────────

pub fn generate_from_trace(
    name: &str,
    agg: &Aggregated,
    use_heuristics: bool,
    cfg: &HeuristicsConfig,
) -> Policy {
    let mut files = Section::default();
    let mut network = NetSection::default();
    let mut processes = Section::default();

    for (path, stats) in &agg.files {
        let risk = if use_heuristics {
            Some(heuristics::analyze_entropy(path, cfg))
        } else {
            None
        };
        let entry = make_entry_simple(path, stats.count, risk.as_ref());
        match risk.as_ref().map(|r| &r.level) {
            Some(heuristics::RiskLevel::DenyRecommended) => files.deny.push(path.clone()),
            Some(heuristics::RiskLevel::NeedsReview) => files.needs_review.push(entry),
            _ => files.allow.push(entry),
        }
    }

    for (dest, stats) in &agg.network {
        let risk = if use_heuristics {
            Some(heuristics::analyze_dest(dest, cfg))
        } else {
            None
        };
        let entry = make_entry_simple(dest, stats.count, risk.as_ref());
        match risk.as_ref().map(|r| &r.level) {
            Some(heuristics::RiskLevel::DenyRecommended) => {
                network.deny_destinations.push(dest.clone())
            }
            Some(heuristics::RiskLevel::NeedsReview) => network.needs_review.push(entry),
            _ => network.allow_destinations.push(entry),
        }
    }

    for (path, stats) in &agg.processes {
        let risk = if use_heuristics {
            Some(heuristics::analyze_entropy(path, cfg))
        } else {
            None
        };
        let entry = make_entry_simple(path, stats.count, risk.as_ref());
        match risk.as_ref().map(|r| &r.level) {
            Some(heuristics::RiskLevel::DenyRecommended) => processes.deny.push(path.clone()),
            Some(heuristics::RiskLevel::NeedsReview) => processes.needs_review.push(entry),
            _ => processes.allow.push(entry),
        }
    }

    Policy {
        _meta: Some(Meta {
            name: name.into(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            profile_runs: None,
            min_stability: None,
            min_runs: None,
        }),
        files,
        network,
        processes,
    }
}

fn make_entry_simple(
    pattern: &str,
    count: u32,
    risk: Option<&heuristics::RiskAssessment>,
) -> Entry {
    match risk {
        Some(r) if r.level > heuristics::RiskLevel::Low => Entry::WithMeta {
            pattern: pattern.into(),
            count: Some(count),
            stability: None,
            runs_seen: None,
            risk: match r.level {
                heuristics::RiskLevel::Low => Some("low".into()),
                heuristics::RiskLevel::NeedsReview => Some("needs_review".into()),
                heuristics::RiskLevel::DenyRecommended => Some("deny_recommended".into()),
            },
            reasons: if r.reasons.is_empty() {
                None
            } else {
                Some(r.reasons.clone())
            },
        },
        _ if count > 1 => Entry::WithMeta {
            pattern: pattern.into(),
            count: Some(count),
            stability: None,
            runs_seen: None,
            risk: None,
            reasons: None,
        },
        _ => Entry::Simple(pattern.into()),
    }
}

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

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::diff::diff_policies;
    use super::profile::classify_entry;
    use super::*;

    #[test]
    fn classify_stable_low_risk() {
        let args = GenerateArgs {
            input: None,
            profile: None,
            output: "".into(),
            name: "".into(),
            format: "yaml".into(),
            dry_run: false,
            diff: false,
            heuristics: false,
            entropy_threshold: 3.8,
            min_stability: 0.8,
            min_runs: 1,
            wilson_z: 1.96,
            review_threshold: 0.6,
            new_is_risky: false,
            alpha: 1.0,
        };

        // High stability, no risk → allow
        assert_eq!(
            classify_entry(0.9, None, &args, 10, 10),
            Some(("allow", false))
        );

        // Medium stability → needs_review
        assert_eq!(
            classify_entry(0.7, None, &args, 10, 7),
            Some(("needs_review", false))
        );

        // Low stability, not risky → skip
        assert_eq!(classify_entry(0.3, None, &args, 10, 3), None);
    }

    #[test]
    fn classify_new_is_risky() {
        let args = GenerateArgs {
            input: None,
            profile: None,
            output: "".into(),
            name: "".into(),
            format: "yaml".into(),
            dry_run: false,
            diff: false,
            heuristics: false,
            entropy_threshold: 3.8,
            min_stability: 0.8,
            min_runs: 1,
            wilson_z: 1.96,
            review_threshold: 0.6,
            new_is_risky: true,
            alpha: 1.0,
        };

        // Low stability with new_is_risky → needs_review
        assert_eq!(
            classify_entry(0.3, None, &args, 10, 3),
            Some(("needs_review", false))
        );
    }

    #[test]
    fn classify_min_runs_gates_early_noise() {
        // Too early: total_runs < min_runs should skip unless new_is_risky
        let mut args = default_args();
        args.min_runs = 5;

        // total_runs=1 < 5 -> gated
        assert_eq!(classify_entry(0.99, None, &args, 1, 1), None);

        args.new_is_risky = true;
        assert_eq!(
            classify_entry(0.99, None, &args, 1, 1),
            Some(("needs_review", false))
        );

        // entry.runs_seen < min_runs -> gated (even if total_runs ok)
        args.new_is_risky = false;
        assert_eq!(classify_entry(0.99, None, &args, 10, 1), None);
    }

    #[test]
    fn classify_risk_overrides() {
        let args = GenerateArgs {
            input: None,
            profile: None,
            output: "".into(),
            name: "".into(),
            format: "yaml".into(),
            dry_run: false,
            diff: false,
            heuristics: true,
            entropy_threshold: 3.8,
            min_stability: 0.8,
            min_runs: 5,
            wilson_z: 1.96,
            review_threshold: 0.6,
            new_is_risky: false,
            alpha: 1.0,
        };

        let mut risk = heuristics::RiskAssessment::default();
        risk.add(heuristics::RiskLevel::DenyRecommended, "test".to_string());

        // High stability but deny risk → deny
        assert_eq!(
            classify_entry(0.95, Some(&risk), &args, 1, 1),
            Some(("deny", true))
        );
    }

    fn default_args() -> GenerateArgs {
        GenerateArgs {
            input: None,
            profile: None,
            output: "".into(),
            name: "".into(),
            format: "yaml".into(),
            dry_run: false,
            diff: false,
            heuristics: false,
            entropy_threshold: 3.8,
            min_stability: 0.8,
            min_runs: 1,
            wilson_z: 1.96,
            review_threshold: 0.6,
            new_is_risky: false,
            alpha: 1.0,
        }
    }

    fn e(pattern: &str, count: Option<u32>, stability: Option<f64>) -> Entry {
        Entry::WithMeta {
            pattern: pattern.to_string(),
            count,
            stability,
            runs_seen: None,
            risk: None,
            reasons: None,
        }
    }

    #[test]
    fn diff_empty_to_populated() {
        let old = Policy::default();
        let mut new = Policy::default();
        new.files.allow.push(Entry::Simple("/tmp/a".into()));
        new.network
            .allow_destinations
            .push(Entry::Simple("api.example.com:443".into()));

        let diff = diff_policies(&old, &new);
        assert_eq!(diff.files_allow.added, vec!["/tmp/a".to_string()]);
        assert_eq!(
            diff.network_allow.added,
            vec!["api.example.com:443".to_string()]
        );
    }

    #[test]
    fn diff_removed_entries() {
        let mut old = Policy::default();
        old.files.allow.push(Entry::Simple("/tmp/old".into()));
        let new = Policy::default();

        let diff = diff_policies(&old, &new);
        assert_eq!(diff.files_allow.removed, vec!["/tmp/old".to_string()]);
        assert!(diff.files_allow.added.is_empty());
    }

    #[test]
    fn diff_stability_change() {
        let mut old = Policy::default();
        old.files.allow.push(e("/tmp/file", Some(3), Some(0.70)));
        let mut new = Policy::default();
        new.files.allow.push(e("/tmp/file", Some(3), Some(0.90)));

        let diff = diff_policies(&old, &new);
        assert_eq!(diff.files_allow.changed.len(), 1);
        assert_eq!(diff.files_allow.changed[0].pattern, "/tmp/file");
    }

    #[test]
    fn diff_no_changes() {
        let mut old = Policy::default();
        old.files.allow.push(Entry::Simple("/tmp/same".into()));
        let new = old.clone();

        let diff = diff_policies(&old, &new);
        assert!(diff.is_empty());
    }
}
