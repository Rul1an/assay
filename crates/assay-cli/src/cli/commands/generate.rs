//! Learning Mode: Generate policy (Phase 3 - with profile support)
//!
//! # Usage
//! ```bash
//! # Single-run mode (Phase 2)
//! assay generate -i trace.jsonl --heuristics
//!
//! # Profile mode (Phase 3)
//! assay generate --profile profile.yaml --min-stability 0.8
//! assay generate --profile profile.yaml --min-stability 0.8 --new-is-risky
//! ```

use anyhow::Result;
use clap::Args;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use super::profile_types::{self, stability_smoothed, Profile, ProfileEntry};

// ─────────────────────────────────────────────────────────────────────────────
// CLI Args
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Args, Debug, Clone)]
#[command(about = "Generate policy from trace or profile")]
pub struct GenerateArgs {
    /// Input trace file (single-run mode)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Profile file (multi-run mode)
    #[arg(long)]
    pub profile: Option<PathBuf>,

    #[arg(short, long, default_value = "policy.yaml")]
    pub output: PathBuf,

    #[arg(long, default_value = "Generated Policy")]
    pub name: String,

    #[arg(long, default_value = "yaml")]
    pub format: String,

    #[arg(long)]
    pub dry_run: bool,

    // ─── Single-run heuristics (Phase 2) ───
    #[arg(long)]
    pub heuristics: bool,

    #[arg(long, default_value_t = 3.8)]
    pub entropy_threshold: f64,

    // ─── Profile stability (Phase 3) ───
    /// Minimum stability to auto-allow (profile mode)
    #[arg(long, default_value_t = 0.8)]
    pub min_stability: f64,

    /// Below this, mark as needs_review if --new-is-risky
    #[arg(long, default_value_t = 0.6)]
    pub review_threshold: f64,

    /// Treat low-stability items as risky (else skip them)
    #[arg(long)]
    pub new_is_risky: bool,

    /// Smoothing parameter (Laplace)
    #[arg(long, default_value_t = 1.0)]
    pub alpha: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Input Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    FileOpen {
        path: String,
        #[serde(default)]
        pid: u32,
        #[serde(default)]
        timestamp: u64,
    },
    NetConnect {
        dest: String,
        #[serde(default)]
        pid: u32,
        #[serde(default)]
        timestamp: u64,
    },
    ProcExec {
        path: String,
        #[serde(default)]
        pid: u32,
        #[serde(default)]
        timestamp: u64,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Output Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Policy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _meta: Option<Meta>,
    pub files: Section,
    pub network: NetSection,
    pub processes: Section,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Meta {
    pub name: String,
    pub generated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_runs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_stability: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Section {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs_review: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NetSection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_destinations: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs_review: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_destinations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Entry {
    Simple(String),
    WithMeta {
        pattern: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        count: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stability: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        runs_seen: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        risk: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasons: Option<Vec<String>>,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Inline Heuristics (from Phase 2)
// ─────────────────────────────────────────────────────────────────────────────

mod heuristics {
    use std::collections::HashMap;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
    pub enum RiskLevel {
        #[default]
        Low,
        NeedsReview,
        DenyRecommended,
    }

    impl RiskLevel {
        pub fn as_str(&self) -> &'static str {
            match self {
                Self::Low => "low",
                Self::NeedsReview => "needs_review",
                Self::DenyRecommended => "deny_recommended",
            }
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct Assessment {
        pub level: RiskLevel,
        pub reasons: Vec<String>,
    }

    impl Assessment {
        pub fn add(&mut self, level: RiskLevel, reason: String) {
            if level > self.level {
                self.level = level;
            }
            self.reasons.push(reason);
        }
    }

    pub struct Config {
        pub entropy_threshold: f64,
        pub allowlist: Vec<&'static str>,
    }
    impl Default for Config {
        fn default() -> Self {
            Self {
                entropy_threshold: 3.8,
                allowlist: vec!["/proc/", "/sys/", "/run/user/", ".so."],
            }
        }
    }

    pub fn entropy(s: &str) -> f64 {
        if s.is_empty() {
            return 0.0;
        }
        let mut freq: HashMap<char, usize> = HashMap::new();
        for c in s.chars() {
            *freq.entry(c).or_default() += 1;
        }
        let len = s.len() as f64;
        freq.values()
            .map(|&n| {
                let p = n as f64 / len;
                -p * p.log2()
            })
            .sum()
    }

    pub fn analyze_path(path: &str, cfg: &Config) -> Assessment {
        let mut r = Assessment::default();
        if cfg.allowlist.iter().any(|p| path.contains(p)) {
            return r;
        }
        let max_seg = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| (s, entropy(s)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        if let Some((seg, ent)) = max_seg {
            if seg.len() >= 8 && ent >= cfg.entropy_threshold {
                let trunc = if seg.len() > 16 { &seg[..16] } else { seg };
                if ent > 4.5 {
                    r.add(
                        RiskLevel::DenyRecommended,
                        format!("high entropy '{}'", trunc),
                    );
                } else {
                    r.add(
                        RiskLevel::NeedsReview,
                        format!("entropy {:.2}: '{}'", ent, trunc),
                    );
                }
            }
        }
        r
    }

    pub fn analyze_dest(dest: &str) -> Assessment {
        let mut r = Assessment::default();
        let suspicious = [22, 23, 445, 139, 3389, 1433, 3306, 5432];
        if let Some(port_str) = dest.rsplit(':').next() {
            if let Ok(port) = port_str.parse::<u16>() {
                if suspicious.contains(&port) {
                    r.add(RiskLevel::NeedsReview, format!("sensitive port {}", port));
                }
            }
        }
        r
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Single-Run Aggregation (Phase 2)
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
// Generate from Single Run (Phase 2)
// ─────────────────────────────────────────────────────────────────────────────

pub fn generate_from_trace(
    name: &str,
    agg: &Aggregated,
    use_heuristics: bool,
    cfg: &heuristics::Config,
) -> Policy {
    let mut files = Section::default();
    let mut network = NetSection::default();
    let mut processes = Section::default();

    for (path, stats) in &agg.files {
        let risk = if use_heuristics {
            Some(heuristics::analyze_path(path, cfg))
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
            Some(heuristics::analyze_dest(dest))
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
            Some(heuristics::analyze_path(path, cfg))
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
        }),
        files,
        network,
        processes,
    }
}

fn make_entry_simple(pattern: &str, count: u32, risk: Option<&heuristics::Assessment>) -> Entry {
    match risk {
        Some(r) if r.level > heuristics::RiskLevel::Low => Entry::WithMeta {
            pattern: pattern.into(),
            count: Some(count),
            stability: None,
            runs_seen: None,
            risk: Some(r.level.as_str().into()),
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
// Generate from Profile (Phase 3)
// ─────────────────────────────────────────────────────────────────────────────

pub fn generate_from_profile(
    name: &str,
    profile: &Profile,
    args: &GenerateArgs,
    heur_cfg: &heuristics::Config,
) -> Policy {
    let total_runs = profile.total_runs;
    let alpha = args.alpha;

    let mut files = Section::default();
    let mut network = NetSection::default();
    let mut processes = Section::default();

    // Process files
    for (path, entry) in &profile.entries.files {
        let stab = stability_smoothed(entry.runs_seen, total_runs, alpha);
        let risk = if args.heuristics {
            Some(heuristics::analyze_path(path, heur_cfg))
        } else {
            None
        };

        if let Some((section, is_deny)) = classify_entry(stab, risk.as_ref(), args) {
            let out_entry = make_entry_profile(path, entry, stab, total_runs, risk.as_ref());
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
        let stab = stability_smoothed(entry.runs_seen, total_runs, alpha);
        let risk = if args.heuristics {
            Some(heuristics::analyze_dest(dest))
        } else {
            None
        };

        if let Some((section, _)) = classify_entry(stab, risk.as_ref(), args) {
            let out_entry = make_entry_profile(dest, entry, stab, total_runs, risk.as_ref());
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
        let stab = stability_smoothed(entry.runs_seen, total_runs, alpha);
        let risk = if args.heuristics {
            Some(heuristics::analyze_path(path, heur_cfg))
        } else {
            None
        };

        if let Some((section, _)) = classify_entry(stab, risk.as_ref(), args) {
            let out_entry = make_entry_profile(path, entry, stab, total_runs, risk.as_ref());
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
        }),
        files,
        network,
        processes,
    }
}

/// Returns (section, is_deny) or None if should skip
fn classify_entry(
    stab: f64,
    risk: Option<&heuristics::Assessment>,
    args: &GenerateArgs,
) -> Option<(&'static str, bool)> {
    // Priority: heuristics risk overrides stability
    if let Some(r) = risk {
        match r.level {
            heuristics::RiskLevel::DenyRecommended => return Some(("deny", true)),
            heuristics::RiskLevel::NeedsReview => return Some(("needs_review", false)),
            _ => {}
        }
    }

    // Stability-based classification
    if stab >= args.min_stability {
        Some(("allow", false))
    } else if stab >= args.review_threshold {
        Some(("needs_review", false))
    } else if args.new_is_risky {
        Some(("needs_review", false))
    } else {
        None // Skip low-stability items
    }
}

fn make_entry_profile(
    pattern: &str,
    entry: &ProfileEntry,
    stab: f64,
    total_runs: u32,
    risk: Option<&heuristics::Assessment>,
) -> Entry {
    let mut reasons = Vec::new();
    reasons.push(format!(
        "stability {:.2} ({}/{})",
        stab, entry.runs_seen, total_runs
    ));
    if let Some(r) = risk {
        reasons.extend(r.reasons.clone());
    }

    Entry::WithMeta {
        pattern: pattern.into(),
        count: Some(entry.hits_total as u32),
        stability: Some((stab * 100.0).round() / 100.0), // Round to 2 decimals
        runs_seen: Some(entry.runs_seen),
        risk: risk.map(|r| r.level.as_str().into()),
        reasons: Some(reasons),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Serialization
// ─────────────────────────────────────────────────────────────────────────────

pub fn serialize(policy: &Policy, format: &str) -> Result<String> {
    Ok(match format {
        "json" => serde_json::to_string_pretty(policy)?,
        _ => serde_yaml::to_string(policy)?,
    })
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

    let heur_cfg = heuristics::Config {
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
            heuristics: false,
            entropy_threshold: 3.8,
            min_stability: 0.8,
            review_threshold: 0.6,
            new_is_risky: false,
            alpha: 1.0,
        };

        // High stability, no risk → allow
        assert_eq!(classify_entry(0.9, None, &args), Some(("allow", false)));

        // Medium stability → needs_review
        assert_eq!(
            classify_entry(0.7, None, &args),
            Some(("needs_review", false))
        );

        // Low stability, not risky → skip
        assert_eq!(classify_entry(0.3, None, &args), None);
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
            heuristics: false,
            entropy_threshold: 3.8,
            min_stability: 0.8,
            review_threshold: 0.6,
            new_is_risky: true,
            alpha: 1.0,
        };

        // Low stability with new_is_risky → needs_review
        assert_eq!(
            classify_entry(0.3, None, &args),
            Some(("needs_review", false))
        );
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
            heuristics: true,
            entropy_threshold: 3.8,
            min_stability: 0.8,
            review_threshold: 0.6,
            new_is_risky: false,
            alpha: 1.0,
        };

        let mut risk = heuristics::Assessment::default();
        risk.add(heuristics::RiskLevel::DenyRecommended, "test".into());

        // High stability but deny risk → deny
        assert_eq!(
            classify_entry(0.95, Some(&risk), &args),
            Some(("deny", true))
        );
    }
}
