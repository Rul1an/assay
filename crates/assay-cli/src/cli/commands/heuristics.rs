//! Learning Mode Heuristics
//!
//! - Entropy detection for suspicious paths
//! - Destination risk hints
//!
//! # Risk Levels
//! | Level            | Meaning                              |
//! |------------------|--------------------------------------|
//! | Low              | Normal, auto-allow                   |
//! | NeedsReview      | Suspicious, human should verify      |
//! | DenyRecommended  | Likely malicious                     |

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// Safe patterns to skip
    pub allowlist: Vec<&'static str>,
    /// Suspicious ports
    pub suspicious_ports: Vec<u16>,
}

impl Default for HeuristicsConfig {
    fn default() -> Self {
        Self {
            entropy_threshold: 3.8,
            min_segment_len: 8,
            allowlist: vec!["/proc/", "/sys/", "/run/user/", ".so."],
            suspicious_ports: vec![22, 23, 445, 139, 3389, 1433, 3306, 5432],
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
    fn parse_ipv6() {
        assert_eq!(parse_dest("[::1]:8080"), ("::1".into(), Some(8080)));
    }
}
