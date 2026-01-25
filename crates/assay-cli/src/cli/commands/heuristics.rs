//! Learning Mode Heuristics
//!
//! - Entropy detection for suspicious paths
//! - Network fanout analysis
//!
//! # Risk Levels
//! | Level            | Meaning                              |
//! |------------------|--------------------------------------|
//! | Low              | Normal, auto-allow                   |
//! | NeedsReview      | Suspicious, human should verify      |
//! | DenyRecommended  | Likely malicious                     |

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

// ─────────────────────────────────────────────────────────────────────────────
// Risk Classification
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    #[default]
    Low,
    NeedsReview,
    DenyRecommended,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub reasons: Vec<String>,
}

impl RiskAssessment {
    pub fn add(&mut self, level: RiskLevel, reason: impl Into<String>) {
        if level > self.level {
            self.level = level;
        }
        self.reasons.push(reason.into());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HeuristicsConfig {
    /// Entropy threshold (bits/char). Default 3.8, range 0-8
    pub entropy_threshold: f64,
    /// Min segment length to analyze
    pub min_segment_len: usize,
    /// Fanout thresholds (unique IPs per process)
    pub fanout_warn: usize,
    pub fanout_deny: usize,
    /// Safe patterns to skip
    pub allowlist: Vec<&'static str>,
    /// Suspicious ports
    pub suspicious_ports: Vec<u16>,
    /// Port scan threshold (unique ports per process)
    pub port_scan_threshold: usize,
}

impl Default for HeuristicsConfig {
    fn default() -> Self {
        Self {
            entropy_threshold: 3.8,
            min_segment_len: 8,
            fanout_warn: 10,
            fanout_deny: 50,
            allowlist: vec!["/proc/", "/sys/", "/run/user/", ".so."],
            suspicious_ports: vec![22, 23, 445, 139, 3389, 1433, 3306, 5432],
            port_scan_threshold: 20,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entropy Analysis
// ─────────────────────────────────────────────────────────────────────────────

/// Shannon entropy (bits per character)
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

/// Find highest entropy path segment
pub fn max_entropy_segment(path: &str) -> Option<(String, f64)> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(|s| (s.to_string(), entropy(s)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// Analyze path for suspicious entropy
pub fn analyze_entropy(path: &str, cfg: &HeuristicsConfig) -> RiskAssessment {
    let mut r = RiskAssessment::default();

    // Skip allowlisted
    if cfg.allowlist.iter().any(|p| path.contains(p)) {
        return r;
    }

    let (seg, ent) = match max_entropy_segment(path) {
        Some(v) => v,
        None => return r,
    };

    if seg.len() < cfg.min_segment_len {
        return r;
    }

    if ent >= cfg.entropy_threshold {
        let reason = format!("high entropy '{}' ({:.2} bits)", truncate(&seg, 16), ent);
        if ent > 4.5 {
            r.add(RiskLevel::DenyRecommended, reason);
        } else {
            r.add(RiskLevel::NeedsReview, reason);
        }
    }

    // Hash-like detection
    if seg.len() >= 32 {
        let hex_ratio =
            seg.chars().filter(|c| c.is_ascii_hexdigit()).count() as f64 / seg.len() as f64;
        if hex_ratio > 0.8 {
            r.add(
                RiskLevel::NeedsReview,
                format!("looks like hash: '{}'", truncate(&seg, 16)),
            );
        }
    }

    r
}

/// Analyze destination for suspicious ports (stateless)
pub fn analyze_dest(dest: &str, cfg: &HeuristicsConfig) -> RiskAssessment {
    let mut r = RiskAssessment::default();
    if let (_, Some(port)) = parse_dest(dest) {
        if cfg.suspicious_ports.contains(&port) {
            r.add(RiskLevel::NeedsReview, format!("sensitive port {}", port));
        }
    }
    r
}

// ─────────────────────────────────────────────────────────────────────────────
// Network Fanout Analysis
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ProcNetStats {
    pub ips: BTreeSet<String>,
    pub ports: BTreeSet<u16>,
}

pub struct NetworkAnalyzer {
    cfg: HeuristicsConfig,
    per_pid: HashMap<u32, ProcNetStats>,
    global: BTreeMap<String, u32>,
}

impl NetworkAnalyzer {
    pub fn new(cfg: HeuristicsConfig) -> Self {
        Self {
            cfg,
            per_pid: HashMap::new(),
            global: BTreeMap::new(),
        }
    }

    pub fn record(&mut self, pid: u32, dest: &str) {
        let (ip, port) = parse_dest(dest);
        let s = self.per_pid.entry(pid).or_default();
        s.ips.insert(ip);
        if let Some(p) = port {
            s.ports.insert(p);
        }
        *self.global.entry(dest.to_string()).or_default() += 1;
    }

    pub fn assess_dest(&self, dest: &str) -> RiskAssessment {
        let mut r = RiskAssessment::default();
        if let (_, Some(port)) = parse_dest(dest) {
            if self.cfg.suspicious_ports.contains(&port) {
                r.add(RiskLevel::NeedsReview, format!("sensitive port {}", port));
            }
        }
        r
    }

    pub fn assess_pid(&self, pid: u32) -> RiskAssessment {
        let mut r = RiskAssessment::default();
        if let Some(s) = self.per_pid.get(&pid) {
            let n = s.ips.len();
            if n >= self.cfg.fanout_deny {
                r.add(
                    RiskLevel::DenyRecommended,
                    format!("{} unique IPs - scanning?", n),
                );
            } else if n >= self.cfg.fanout_warn {
                r.add(RiskLevel::NeedsReview, format!("{} unique IPs", n));
            }
            if s.ports.len() > self.cfg.port_scan_threshold {
                r.add(
                    RiskLevel::DenyRecommended,
                    format!("{} ports - port scan?", s.ports.len()),
                );
            }
        }
        r
    }
}

pub fn parse_dest(dest: &str) -> (String, Option<u16>) {
    if dest.starts_with('[') {
        if let Some(i) = dest.find(']') {
            let ip = dest[1..i].to_string();
            let port = dest[i + 1..].strip_prefix(':').and_then(|p| p.parse().ok());
            return (ip, port);
        }
    }
    if let Some(i) = dest.rfind(':') {
        (dest[..i].to_string(), dest[i + 1..].parse().ok())
    } else {
        (dest.to_string(), None)
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_low() {
        assert!(entropy("aaaaaaaaaa") < 0.1);
    }

    #[test]
    fn entropy_high() {
        assert!(entropy("a1b2c3d4e5f6g7h8") > 3.5);
    }

    #[test]
    fn safe_path() {
        let r = analyze_entropy("/etc/passwd", &HeuristicsConfig::default());
        assert_eq!(r.level, RiskLevel::Low);
    }

    #[test]
    fn suspicious_path() {
        let r = analyze_entropy(
            "/tmp/a1b2c3d4e5f6g7h8i9j0k1l2.sh",
            &HeuristicsConfig::default(),
        );
        assert!(r.level >= RiskLevel::NeedsReview);
    }

    #[test]
    fn fanout_warn() {
        let cfg = HeuristicsConfig {
            fanout_warn: 3,
            fanout_deny: 5,
            ..Default::default()
        };
        let mut na = NetworkAnalyzer::new(cfg);
        for i in 1..=4 {
            na.record(1, &format!("10.0.0.{}:80", i));
        }
        assert!(na.assess_pid(1).level >= RiskLevel::NeedsReview);
    }

    #[test]
    fn parse_ipv6() {
        assert_eq!(parse_dest("[::1]:8080"), ("::1".into(), Some(8080)));
    }
}
