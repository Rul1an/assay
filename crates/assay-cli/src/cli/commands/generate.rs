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
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::events::Event;
use super::heuristics::{self, HeuristicsConfig};
use super::profile_types::{self, stability_smoothed, Profile, ProfileEntry};

mod args;
mod model;

pub use args::GenerateArgs;
pub use model::{serialize, Entry, Meta, NetSection, Policy, Section};

// ─────────────────────────────────────────────────────────────────────────────
// CLI Args
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// Single-Run Aggregation
// ─────────────────────────────────────────────────────────────────────────────

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
// Generate from Profile
// ─────────────────────────────────────────────────────────────────────────────

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
fn classify_entry(
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
        // Medium stability OR low-stability with new_is_risky → needs_review
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryFingerprint {
    count: Option<u32>,
    stability_bps: Option<i64>,
    runs_seen: Option<u32>,
    risk: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryChange {
    pattern: String,
    old: EntryFingerprint,
    new: EntryFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct SectionDiff {
    added: Vec<String>,
    removed: Vec<String>,
    changed: Vec<EntryChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct PolicyDiff {
    files_allow: SectionDiff,
    files_review: SectionDiff,
    files_deny: SectionDiff,
    network_allow: SectionDiff,
    network_review: SectionDiff,
    network_deny: SectionDiff,
    processes_allow: SectionDiff,
    processes_review: SectionDiff,
    processes_deny: SectionDiff,
}

impl PolicyDiff {
    fn summary_counts(&self) -> (usize, usize, usize) {
        let sections = [
            &self.files_allow,
            &self.files_review,
            &self.files_deny,
            &self.network_allow,
            &self.network_review,
            &self.network_deny,
            &self.processes_allow,
            &self.processes_review,
            &self.processes_deny,
        ];
        let added = sections.iter().map(|s| s.added.len()).sum();
        let removed = sections.iter().map(|s| s.removed.len()).sum();
        let changed = sections.iter().map(|s| s.changed.len()).sum();
        (added, removed, changed)
    }

    fn is_empty(&self) -> bool {
        self.summary_counts() == (0, 0, 0)
    }
}

fn parse_existing_policy(path: &PathBuf) -> Result<Policy> {
    let raw = std::fs::read_to_string(path)?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ext == "json" {
        return Ok(serde_json::from_str(&raw)?);
    }
    match serde_yaml::from_str(&raw) {
        Ok(p) => Ok(p),
        Err(_) => Ok(serde_json::from_str(&raw)?),
    }
}

fn entry_pattern(entry: &Entry) -> String {
    match entry {
        Entry::Simple(s) => s.clone(),
        Entry::WithMeta { pattern, .. } => pattern.clone(),
    }
}

fn fp_stability_bps(v: Option<f64>) -> Option<i64> {
    v.map(|x| (x * 10000.0).round() as i64)
}

fn entry_fingerprint(entry: &Entry) -> EntryFingerprint {
    match entry {
        Entry::Simple(_) => EntryFingerprint {
            count: None,
            stability_bps: None,
            runs_seen: None,
            risk: None,
        },
        Entry::WithMeta {
            count,
            stability,
            runs_seen,
            risk,
            ..
        } => EntryFingerprint {
            count: *count,
            stability_bps: fp_stability_bps(*stability),
            runs_seen: *runs_seen,
            risk: risk.clone(),
        },
    }
}

fn diff_entries(old: &[Entry], new: &[Entry]) -> SectionDiff {
    let old_map: BTreeMap<String, EntryFingerprint> = old
        .iter()
        .map(|e| (entry_pattern(e), entry_fingerprint(e)))
        .collect();
    let new_map: BTreeMap<String, EntryFingerprint> = new
        .iter()
        .map(|e| (entry_pattern(e), entry_fingerprint(e)))
        .collect();

    let mut out = SectionDiff::default();
    for (pattern, new_fp) in &new_map {
        match old_map.get(pattern) {
            None => out.added.push(pattern.clone()),
            Some(old_fp) if old_fp != new_fp => out.changed.push(EntryChange {
                pattern: pattern.clone(),
                old: old_fp.clone(),
                new: new_fp.clone(),
            }),
            _ => {}
        }
    }
    for pattern in old_map.keys() {
        if !new_map.contains_key(pattern) {
            out.removed.push(pattern.clone());
        }
    }
    out
}

fn diff_string_lists(old: &[String], new: &[String]) -> SectionDiff {
    let old_set: BTreeMap<String, EntryFingerprint> = old
        .iter()
        .cloned()
        .map(|s| {
            (
                s,
                EntryFingerprint {
                    count: None,
                    stability_bps: None,
                    runs_seen: None,
                    risk: None,
                },
            )
        })
        .collect();
    let new_set: BTreeMap<String, EntryFingerprint> = new
        .iter()
        .cloned()
        .map(|s| {
            (
                s,
                EntryFingerprint {
                    count: None,
                    stability_bps: None,
                    runs_seen: None,
                    risk: None,
                },
            )
        })
        .collect();

    let mut out = SectionDiff::default();
    for pattern in new_set.keys() {
        if !old_set.contains_key(pattern) {
            out.added.push(pattern.clone());
        }
    }
    for pattern in old_set.keys() {
        if !new_set.contains_key(pattern) {
            out.removed.push(pattern.clone());
        }
    }
    out
}

fn diff_policies(old: &Policy, new: &Policy) -> PolicyDiff {
    PolicyDiff {
        files_allow: diff_entries(&old.files.allow, &new.files.allow),
        files_review: diff_entries(&old.files.needs_review, &new.files.needs_review),
        files_deny: diff_string_lists(&old.files.deny, &new.files.deny),
        network_allow: diff_entries(
            &old.network.allow_destinations,
            &new.network.allow_destinations,
        ),
        network_review: diff_entries(&old.network.needs_review, &new.network.needs_review),
        network_deny: diff_string_lists(
            &old.network.deny_destinations,
            &new.network.deny_destinations,
        ),
        processes_allow: diff_entries(&old.processes.allow, &new.processes.allow),
        processes_review: diff_entries(&old.processes.needs_review, &new.processes.needs_review),
        processes_deny: diff_string_lists(&old.processes.deny, &new.processes.deny),
    }
}

fn print_section_diff(label: &str, diff: &SectionDiff) {
    if diff.added.is_empty() && diff.removed.is_empty() && diff.changed.is_empty() {
        return;
    }
    eprintln!("  {}:", label);
    for v in &diff.added {
        eprintln!("    + {}", v);
    }
    for v in &diff.removed {
        eprintln!("    - {}", v);
    }
    for c in &diff.changed {
        eprintln!("    ~ {}", c.pattern);
    }
}

fn print_policy_diff(diff: &PolicyDiff, output_path: &Path) {
    eprintln!();
    eprintln!("Policy diff ({} -> generated):", output_path.display());
    if diff.is_empty() {
        eprintln!("  (no changes)");
        return;
    }
    print_section_diff("files.allow", &diff.files_allow);
    print_section_diff("files.needs_review", &diff.files_review);
    print_section_diff("files.deny", &diff.files_deny);
    print_section_diff("network.allow_destinations", &diff.network_allow);
    print_section_diff("network.needs_review", &diff.network_review);
    print_section_diff("network.deny_destinations", &diff.network_deny);
    print_section_diff("processes.allow", &diff.processes_allow);
    print_section_diff("processes.needs_review", &diff.processes_review);
    print_section_diff("processes.deny", &diff.processes_deny);
    let (added, removed, changed) = diff.summary_counts();
    eprintln!();
    eprintln!(
        "  Summary: +{} added, -{} removed, ~{} changed",
        added, removed, changed
    );
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
            match parse_existing_policy(&args.output) {
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
        let diff = diff_policies(&old_policy, &policy);
        print_policy_diff(&diff, &args.output);
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
