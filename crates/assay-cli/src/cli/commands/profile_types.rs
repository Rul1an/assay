//! Profile types for multi-run stability analysis (Phase 3)
//!
//! A Profile accumulates observations across multiple runs to determine
//! which artifacts are stable (consistently observed) vs noise (sporadic).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROFILE_VERSION: &str = "1.0";
pub const MAX_RUN_IDS: usize = 200;

// ─────────────────────────────────────────────────────────────────────────────
// Profile Schema
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub version: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,

    /// Scope fingerprint (config hash, suite name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    pub total_runs: u32,

    /// Idempotency: last N run IDs (ring buffer)
    #[serde(default)]
    pub run_ids: Vec<String>,

    pub entries: ProfileEntries,
}

impl Profile {
    pub fn new(name: &str, scope: Option<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            version: PROFILE_VERSION.into(),
            name: name.into(),
            created_at: now.clone(),
            updated_at: now,
            scope,
            total_runs: 0,
            run_ids: Vec::new(),
            entries: ProfileEntries::default(),
        }
    }

    pub fn has_run(&self, run_id: &str) -> bool {
        self.run_ids.iter().any(|id| id == run_id)
    }

    pub fn add_run_id(&mut self, run_id: String) {
        self.run_ids.push(run_id);
        if self.run_ids.len() > MAX_RUN_IDS {
            self.run_ids.remove(0);
        }
    }

    pub fn total_entries(&self) -> usize {
        self.entries.files.len() + self.entries.network.len() + self.entries.processes.len()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileEntries {
    #[serde(default)]
    pub files: BTreeMap<String, ProfileEntry>,
    #[serde(default)]
    pub network: BTreeMap<String, ProfileEntry>,
    #[serde(default)]
    pub processes: BTreeMap<String, ProfileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileEntry {
    pub first_seen: u64,
    pub last_seen: u64,
    pub runs_seen: u32,
    #[serde(default)]
    pub hits_total: u64,
}

impl ProfileEntry {
    pub fn new(timestamp: u64, hits: u64) -> Self {
        Self {
            first_seen: timestamp,
            last_seen: timestamp,
            runs_seen: 1,
            hits_total: hits,
        }
    }

    pub fn merge_run(&mut self, timestamp: u64, hits: u64) {
        self.runs_seen += 1;
        self.hits_total += hits;
        if timestamp > 0 {
            if self.first_seen == 0 || timestamp < self.first_seen {
                self.first_seen = timestamp;
            }
            if timestamp > self.last_seen {
                self.last_seen = timestamp;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Stability Scoring
// ─────────────────────────────────────────────────────────────────────────────

/// Laplace-smoothed stability: (runs_seen + α) / (total_runs + 2α)
/// With α=1: 1/1→0.67, 10/10→0.92, 5/10→0.5, 0/10→0.08
pub fn stability_smoothed(runs_seen: u32, total_runs: u32, alpha: f64) -> f64 {
    if total_runs == 0 {
        return 0.0;
    }
    (runs_seen as f64 + alpha) / (total_runs as f64 + 2.0 * alpha)
}

pub const DEFAULT_ALPHA: f64 = 1.0;

#[allow(dead_code)] // Will be used in SOTA 2026 refinements
#[derive(Debug, Clone)]
pub struct StabilityConfig {
    pub promote_threshold: f64, // >= this → allow
    pub review_threshold: f64,  // < this → needs_review (if new_is_risky)
    pub alpha: f64,
    pub new_is_risky: bool,
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            promote_threshold: 0.8,
            review_threshold: 0.6,
            alpha: DEFAULT_ALPHA,
            new_is_risky: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// IO Helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn load_profile(path: &std::path::Path) -> anyhow::Result<Profile> {
    let content = std::fs::read_to_string(path)?;
    Ok(if path.extension().map(|e| e == "json").unwrap_or(false) {
        serde_json::from_str(&content)?
    } else {
        serde_yaml::from_str(&content)?
    })
}

pub fn save_profile(profile: &Profile, path: &std::path::Path) -> anyhow::Result<()> {
    let content = if path.extension().map(|e| e == "json").unwrap_or(false) {
        serde_json::to_string_pretty(profile)?
    } else {
        serde_yaml::to_string(profile)?
    };
    std::fs::write(path, content)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stability_scores() {
        assert!((stability_smoothed(1, 1, 1.0) - 0.67).abs() < 0.01);
        assert!((stability_smoothed(10, 10, 1.0) - 0.92).abs() < 0.01);
        assert!((stability_smoothed(5, 10, 1.0) - 0.5).abs() < 0.01);
        assert!((stability_smoothed(0, 10, 1.0) - 0.08).abs() < 0.01);
        assert_eq!(stability_smoothed(0, 0, 1.0), 0.0);
    }

    #[test]
    fn idempotency() {
        let mut p = Profile::new("test", None);
        assert!(!p.has_run("run-1"));
        p.add_run_id("run-1".into());
        assert!(p.has_run("run-1"));
    }

    #[test]
    fn ring_buffer() {
        let mut p = Profile::new("test", None);
        for i in 0..(MAX_RUN_IDS + 10) {
            p.add_run_id(format!("run-{}", i));
        }
        assert_eq!(p.run_ids.len(), MAX_RUN_IDS);
        assert!(!p.has_run("run-0"));
        assert!(p.has_run("run-10"));
    }

    #[test]
    fn entry_merge() {
        let mut e = ProfileEntry::new(100, 5);
        e.merge_run(200, 3);
        assert_eq!(e.runs_seen, 2);
        assert_eq!(e.hits_total, 8);
        assert_eq!(e.first_seen, 100);
        assert_eq!(e.last_seen, 200);
    }
}
