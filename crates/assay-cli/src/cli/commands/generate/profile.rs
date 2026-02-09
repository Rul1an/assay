use super::super::heuristics::{self, HeuristicsConfig};
use super::super::profile_types::{self, stability_smoothed, Profile, ProfileEntry};
use super::args::GenerateArgs;
use super::model::{Entry, Meta, NetSection, Policy, Section};

pub fn generate_from_profile(
    name: &str,
    profile: &Profile,
    args: &GenerateArgs,
    heur_cfg: &HeuristicsConfig,
) -> Policy {
    let total_runs = profile.total_runs;
    let alpha = args.alpha;

    let mut files = Section::default();
    let mut network = NetSection::default();
    let mut processes = Section::default();

    // Process files
    for (path, entry) in &profile.entries.files {
        let stab_display = stability_smoothed(entry.runs_seen, total_runs, alpha);
        let stab_gate =
            profile_types::stability_wilson_lower(entry.runs_seen, total_runs, args.wilson_z);
        let risk = if args.heuristics {
            Some(heuristics::analyze_entropy(path, heur_cfg))
        } else {
            None
        };

        if let Some((section, is_deny)) =
            classify_entry(stab_gate, risk.as_ref(), args, total_runs, entry.runs_seen)
        {
            let out_entry = make_entry_profile(
                path,
                entry,
                stab_display,
                stab_gate,
                total_runs,
                args,
                risk.as_ref(),
            );
            match (section, is_deny) {
                ("deny", _) => files.deny.push(path.clone()),
                ("needs_review", _) => files.needs_review.push(out_entry),
                ("allow", _) => files.allow.push(out_entry),
                _ => {}
            }
        }
    }

    // Process network
    for (dest, entry) in &profile.entries.network {
        let stab_display = stability_smoothed(entry.runs_seen, total_runs, alpha);
        let stab_gate =
            profile_types::stability_wilson_lower(entry.runs_seen, total_runs, args.wilson_z);
        let risk = if args.heuristics {
            Some(heuristics::analyze_dest(dest, heur_cfg))
        } else {
            None
        };

        if let Some((section, _)) =
            classify_entry(stab_gate, risk.as_ref(), args, total_runs, entry.runs_seen)
        {
            let out_entry = make_entry_profile(
                dest,
                entry,
                stab_display,
                stab_gate,
                total_runs,
                args,
                risk.as_ref(),
            );
            match section {
                "deny" => network.deny_destinations.push(dest.clone()),
                "needs_review" => network.needs_review.push(out_entry),
                "allow" => network.allow_destinations.push(out_entry),
                _ => {}
            }
        }
    }

    // Process processes
    for (path, entry) in &profile.entries.processes {
        let stab_display = stability_smoothed(entry.runs_seen, total_runs, alpha);
        let stab_gate =
            profile_types::stability_wilson_lower(entry.runs_seen, total_runs, args.wilson_z);
        let risk = if args.heuristics {
            Some(heuristics::analyze_entropy(path, heur_cfg))
        } else {
            None
        };

        if let Some((section, _)) =
            classify_entry(stab_gate, risk.as_ref(), args, total_runs, entry.runs_seen)
        {
            let out_entry = make_entry_profile(
                path,
                entry,
                stab_display,
                stab_gate,
                total_runs,
                args,
                risk.as_ref(),
            );
            match section {
                "deny" => processes.deny.push(path.clone()),
                "needs_review" => processes.needs_review.push(out_entry),
                "allow" => processes.allow.push(out_entry),
                _ => {}
            }
        }
    }

    Policy {
        _meta: Some(Meta {
            name: name.into(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            profile_runs: Some(total_runs),
            min_stability: Some(args.min_stability),
            min_runs: if args.min_runs > 1 {
                Some(args.min_runs)
            } else {
                None
            },
        }),
        files,
        network,
        processes,
    }
}

/// Returns (section, is_deny) or None if should skip
/// `stab_gate` is Wilson lower bound for conservative gating
pub(super) fn classify_entry(
    stab_gate: f64,
    risk: Option<&heuristics::RiskAssessment>,
    args: &GenerateArgs,
    total_runs: u32,
    runs_seen: u32,
) -> Option<(&'static str, bool)> {
    // Priority: heuristics risk overrides stability
    if let Some(r) = risk {
        match r.level {
            heuristics::RiskLevel::DenyRecommended => return Some(("deny", true)),
            heuristics::RiskLevel::NeedsReview => return Some(("needs_review", false)),
            _ => {}
        }
    }

    // Safety belt: not enough runs (profile-wide or entry-specific) -> don't auto-allow yet
    if total_runs < args.min_runs || runs_seen < args.min_runs {
        return if args.new_is_risky {
            Some(("needs_review", false))
        } else {
            None
        };
    }

    // Stability-based classification (using Wilson lower bound for gating)
    if stab_gate >= args.min_stability {
        Some(("allow", false))
    } else if stab_gate >= args.review_threshold || args.new_is_risky {
        // Medium stability OR low-stability with new_is_risky -> needs_review
        Some(("needs_review", false))
    } else {
        None // Skip low-stability items
    }
}

fn make_entry_profile(
    pattern: &str,
    entry: &ProfileEntry,
    stab_display: f64,
    stab_gate: f64,
    total_runs: u32,
    args: &GenerateArgs,
    risk: Option<&heuristics::RiskAssessment>,
) -> Entry {
    let mut reasons = Vec::new();

    // Safety belt feedback
    if total_runs < args.min_runs || entry.runs_seen < args.min_runs {
        reasons.push(format!(
            "min_runs gate: need >= {} runs (profile={}, entry={})",
            args.min_runs, total_runs, entry.runs_seen
        ));
    }

    // Show both Laplace (human-readable) and Wilson (gating) scores
    reasons.push(format!(
        "wilson_lb {:.2} (z={:.2})",
        stab_gate, args.wilson_z
    ));
    reasons.push(format!(
        "laplace {:.2} (α={:.1}, {}/{})",
        stab_display, args.alpha, entry.runs_seen, total_runs
    ));
    if let Some(r) = risk {
        reasons.extend(r.reasons.clone());
    }

    Entry::WithMeta {
        pattern: pattern.into(),
        count: Some(entry.hits_total as u32),
        stability: Some((stab_display * 100.0).round() / 100.0), // Round to 2 decimals
        runs_seen: Some(entry.runs_seen),
        risk: risk.map(|r| match r.level {
            heuristics::RiskLevel::Low => "low".into(),
            heuristics::RiskLevel::NeedsReview => "needs_review".into(),
            heuristics::RiskLevel::DenyRecommended => "deny_recommended".into(),
        }),
        reasons: Some(reasons),
    }
}

#[cfg(test)]
mod tests {
    use super::classify_entry;
    use crate::cli::commands::generate::GenerateArgs;
    use crate::cli::commands::heuristics;
    use std::path::PathBuf;

    fn default_args() -> GenerateArgs {
        GenerateArgs {
            input: None,
            profile: None,
            output: PathBuf::from("policy.yaml"),
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

    #[test]
    fn classify_stable_low_risk() {
        let args = default_args();

        assert_eq!(
            classify_entry(0.9, None, &args, 10, 10),
            Some(("allow", false))
        );
        assert_eq!(
            classify_entry(0.7, None, &args, 10, 7),
            Some(("needs_review", false))
        );
        assert_eq!(classify_entry(0.3, None, &args, 10, 3), None);
    }

    #[test]
    fn classify_new_is_risky() {
        let mut args = default_args();
        args.new_is_risky = true;

        assert_eq!(
            classify_entry(0.3, None, &args, 10, 3),
            Some(("needs_review", false))
        );
    }

    #[test]
    fn classify_min_runs_gates_early_noise() {
        let mut args = default_args();
        args.min_runs = 5;

        assert_eq!(classify_entry(0.99, None, &args, 1, 1), None);

        args.new_is_risky = true;
        assert_eq!(
            classify_entry(0.99, None, &args, 1, 1),
            Some(("needs_review", false))
        );

        args.new_is_risky = false;
        assert_eq!(classify_entry(0.99, None, &args, 10, 1), None);
    }

    #[test]
    fn classify_risk_overrides() {
        let mut args = default_args();
        args.heuristics = true;
        args.min_runs = 5;

        let mut risk = heuristics::RiskAssessment::default();
        risk.add(heuristics::RiskLevel::DenyRecommended, "test".to_string());

        assert_eq!(
            classify_entry(0.95, Some(&risk), &args, 1, 1),
            Some(("deny", true))
        );
    }
}
