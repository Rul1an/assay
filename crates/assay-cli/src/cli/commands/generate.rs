//! Learning Mode: Generate policy from observed events (v2.2 Phase 1 + 2)

use crate::cli::args::GenerateArgs;
use crate::cli::commands::heuristics::{
    self, parse_dest, HeuristicsConfig, RiskAssessment, RiskLevel,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

// ─────────────────────────────────────────────────────────────────────────────
// Input Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ObservedEvent {
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
// Output Types (SOTA / assay-policy aligned)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Default)]
pub struct Policy {
    #[serde(default, skip_serializing_if = "FilePolicy::is_empty")]
    pub files: FilePolicy,

    #[serde(default, skip_serializing_if = "NetworkPolicy::is_empty")]
    pub network: NetworkPolicy,

    #[serde(default, skip_serializing_if = "ProcessPolicy::is_empty")]
    pub processes: ProcessPolicy,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FilePolicy {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allow: BTreeSet<String>,

    // Items that need review due to risk heuristics
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub needs_review: BTreeMap<String, RiskInfo>,
}

impl FilePolicy {
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.needs_review.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NetworkPolicy {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allow_cidrs: BTreeSet<String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub needs_review: BTreeMap<String, RiskInfo>,
}

impl NetworkPolicy {
    pub fn is_empty(&self) -> bool {
        self.allow_cidrs.is_empty() && self.needs_review.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProcessPolicy {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allow_executables: BTreeSet<String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub needs_review: BTreeMap<String, RiskInfo>,
}

impl ProcessPolicy {
    pub fn is_empty(&self) -> bool {
        self.allow_executables.is_empty() && self.needs_review.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskInfo {
    pub risk: RiskLevel,
    pub reasons: Vec<String>,
    pub count: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Aggregation
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub count: u32,
    pub first_seen: u64,
    pub last_seen: u64,
    pub example_pids: BTreeSet<u32>,
}

impl Stats {
    fn update(&mut self, ts: u64, pid: u32) {
        self.count += 1;
        if ts > 0 {
            if self.first_seen == 0 || ts < self.first_seen {
                self.first_seen = ts;
            }
            if ts > self.last_seen {
                self.last_seen = ts;
            }
        }
        if self.example_pids.len() < 5 {
            self.example_pids.insert(pid);
        }
    }
}

#[derive(Debug, Default)]
pub struct Aggregated {
    pub files: BTreeMap<String, Stats>,
    pub network: BTreeMap<String, Stats>,
    pub processes: BTreeMap<String, Stats>,
    // Track full fanout separate from stats to avoid capping
    pub raw_fanout: std::collections::HashMap<u32, BTreeSet<String>>,
}

impl Aggregated {
    pub fn total(&self) -> usize {
        self.files.len() + self.network.len() + self.processes.len()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Core Logic
// ─────────────────────────────────────────────────────────────────────────────

pub fn read_events(path: &PathBuf) -> Result<Vec<ObservedEvent>> {
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
        if line.trim().is_empty() || line.trim().starts_with('#') {
            continue;
        }
        total_lines += 1;
        match serde_json::from_str(&line) {
            Ok(e) => events.push(e),
            Err(e) => {
                error_count += 1;
                eprintln!("line {}: {}", i + 1, e);
            }
        }
    }

    if error_count > 0 {
        if events.is_empty() {
            return Err(anyhow::anyhow!(
                "failed to parse any events from input: {} parse errors over {} event lines",
                error_count,
                total_lines
            ));
        }
        let error_rate = error_count as f64 / total_lines as f64;
        if error_rate > 0.5 {
            eprintln!(
                "warning: high parse error rate: {} errors, {} successfully parsed ({} total event lines)",
                error_count,
                events.len(),
                total_lines
            );
        }
    }
    Ok(events)
}

pub fn aggregate(events: &[ObservedEvent]) -> Aggregated {
    let mut agg = Aggregated::default();
    for ev in events {
        match ev {
            ObservedEvent::FileOpen {
                path,
                timestamp,
                pid,
            } => {
                agg.files
                    .entry(path.clone())
                    .or_default()
                    .update(*timestamp, *pid);
            }
            ObservedEvent::NetConnect {
                dest,
                timestamp,
                pid,
            } => {
                agg.network
                    .entry(dest.clone())
                    .or_default()
                    .update(*timestamp, *pid);

                let entry = agg.raw_fanout.entry(*pid).or_default();
                if entry.len() < 5000 {
                    entry.insert(dest.clone());
                }
            }
            ObservedEvent::ProcExec {
                path,
                timestamp,
                pid,
            } => {
                agg.processes
                    .entry(path.clone())
                    .or_default()
                    .update(*timestamp, *pid);
            }
        }
    }
    agg
}

pub fn generate_policy(
    _name: &str,
    strictness: f64,
    _source: Option<String>,
    agg: &Aggregated,
    _with_timestamp: bool,
) -> Policy {
    let mut policy = Policy::default();

    // Map strictness (0.0-1.0) to heuristics config
    // strictness 0.5 (default) -> entropy 3.8, fanout 10
    // strictness 1.0 (strict)  -> entropy 3.0, fanout 5
    // strictness 0.0 (loose)   -> entropy 5.0, fanout 50
    let mut cfg = HeuristicsConfig::default();

    if strictness > 0.0 {
        // Linear interpolation roughly
        // 0.0 -> 5.0
        // 1.0 -> 3.0
        // formula: 5.0 - (strictness * 2.0)
        cfg.entropy_threshold = (5.0 - (strictness * 2.0)).max(2.5);

        // Fanout
        // 0.0 -> 50
        // 1.0 -> 5
        let f_warn = (50.0 - (strictness * 45.0)) as usize;
        cfg.fanout_warn = f_warn.max(3);
        cfg.fanout_deny = f_warn * 5;

        // Port Scan
        // 0.0 -> 40
        // 1.0 -> 10
        cfg.port_scan_threshold = ((40.0 - strictness * 30.0) as usize).max(10);
    }

    // Global Fanout Analysis (Accurate)
    let mut net_analyzer = heuristics::NetworkAnalyzer::new(cfg.clone());
    for (pid, dests) in &agg.raw_fanout {
        for dest in dests {
            net_analyzer.record(*pid, dest);
        }
    }

    // Map suspicious PIDs back to their destinations to ensure we catch all of them
    // dest -> max_risk
    let mut suspicious_dest_risks: std::collections::HashMap<String, RiskAssessment> =
        std::collections::HashMap::new();
    for (pid, dests) in &agg.raw_fanout {
        let risk = net_analyzer.assess_pid(*pid);
        if risk.level != RiskLevel::Low {
            for dest in dests {
                // If this dest was touched by a suspicious PID, record it
                if let Some(existing) = suspicious_dest_risks.get_mut(dest) {
                    if risk.level > existing.level {
                        *existing = risk.clone();
                    }
                } else {
                    suspicious_dest_risks.insert(dest.clone(), risk.clone());
                }
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Files
    // ─────────────────────────────────────────────────────────────────────────
    for (path, stats) in &agg.files {
        let risk = heuristics::analyze_entropy(path, &cfg);
        if risk.level != RiskLevel::Low {
            policy.files.needs_review.insert(
                path.clone(),
                RiskInfo {
                    risk: risk.level,
                    reasons: risk.reasons,
                    count: stats.count,
                },
            );
        } else {
            policy.files.allow.insert(path.clone());
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Network
    // ─────────────────────────────────────────────────────────────────────────
    for (dest, stats) in &agg.network {
        // Destination Risk
        let dest_risk = net_analyzer.assess_dest(dest);

        // Fanout Risk: Check if *any* PID that touched this was risky
        // derived from the full raw_fanout, not just statistics.example_pids
        let fanout_risk = suspicious_dest_risks.get(dest).cloned().unwrap_or_default();

        if dest_risk.level != RiskLevel::Low || fanout_risk.level != RiskLevel::Low {
            let mut reasons = dest_risk.reasons;
            reasons.extend(fanout_risk.reasons);
            let level = if fanout_risk.level > dest_risk.level {
                fanout_risk.level
            } else {
                dest_risk.level
            };

            policy.network.needs_review.insert(
                dest.clone(),
                RiskInfo {
                    risk: level,
                    reasons: {
                        reasons.sort();
                        reasons.dedup();
                        reasons
                    },
                    count: stats.count,
                },
            );
        } else {
            // Apply standard allow logic
            use std::net::IpAddr;
            let (ip_str, _) = parse_dest(dest);

            if let Ok(ip) = ip_str.parse::<IpAddr>() {
                let cidr = match ip {
                    IpAddr::V4(_) => format!("{}/32", ip),
                    IpAddr::V6(_) => format!("{}/128", ip),
                };
                policy.network.allow_cidrs.insert(cidr);
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Processes
    // ─────────────────────────────────────────────────────────────────────────
    for (path, stats) in &agg.processes {
        let risk = heuristics::analyze_entropy(path, &cfg);
        if risk.level != RiskLevel::Low {
            policy.processes.needs_review.insert(
                path.clone(),
                RiskInfo {
                    risk: risk.level,
                    reasons: risk.reasons,
                    count: stats.count,
                },
            );
        } else {
            policy.processes.allow_executables.insert(path.clone());
        }
    }

    policy
}

pub fn serialize(policy: &Policy, format: &str) -> Result<String> {
    Ok(match format {
        "json" => serde_json::to_string_pretty(policy)?,
        _ => {
            let yaml = serde_yaml::to_string(policy)?;
            let header = format!(
                "# Generated Policy (v2.0)\n# Source: Learned from observations\n# Generated at: {}\n\n",
                chrono::Utc::now().to_rfc3339()
            );
            format!("{}{}", header, yaml)
        }
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry Point
// ─────────────────────────────────────────────────────────────────────────────

pub fn run(args: GenerateArgs) -> Result<i32> {
    let events = read_events(&args.input)?;
    let agg = aggregate(&events);

    eprintln!(
        "Aggregated {} unique from {} events",
        agg.total(),
        events.len()
    );

    let source =
        (args.input.to_string_lossy() != "-").then(|| args.input.to_string_lossy().to_string());

    let policy = generate_policy(&args.name, args.strictness, source, &agg, true);
    let output = serialize(&policy, &args.format)?;

    if args.dry_run {
        println!("{}", output);
    } else {
        if let Some(parent) = args.output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&args.output, &output)?;
        eprintln!("Wrote {}", args.output.display());
    }

    Ok(super::exit_codes::OK)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_file_event() {
        let e: ObservedEvent =
            serde_json::from_str(r#"{"type":"file_open","path":"/etc/passwd"}"#).unwrap();
        assert!(matches!(e, ObservedEvent::FileOpen { path, .. } if path == "/etc/passwd"));
    }

    #[test]
    fn deterministic_order() {
        let events = vec![
            ObservedEvent::FileOpen {
                path: "/z".into(),
                pid: 1,
                timestamp: 0,
            },
            ObservedEvent::FileOpen {
                path: "/a".into(),
                pid: 1,
                timestamp: 0,
            },
        ];
        let agg = aggregate(&events);
        let policy = generate_policy("T", 0.5, None, &agg, false);

        let allowed: Vec<_> = policy.files.allow.into_iter().collect();
        assert_eq!(allowed, vec!["/a", "/z"]);
    }
}
