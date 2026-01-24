//! Simulation tests for multi-run stability (Phase 3)
//!
//! These tests verify:
//! 1. Stable artifacts get high stability scores
//! 2. Noise artifacts get low stability scores
//! 3. Idempotency prevents double-counting
//! 4. Run ID ring buffer works correctly

use std::collections::BTreeMap;

// Import types (adjust path for actual integration)
// use assay_cli::commands::{profile, profile_types, generate};

/// Simulated profile types (inline for standalone testing)
mod profile_types {
    use super::*;

    pub const MAX_RUN_IDS: usize = 200;

    #[derive(Debug, Clone, Default)]
    pub struct Profile {
        pub name: String,
        pub total_runs: u32,
        pub run_ids: Vec<String>,
        pub files: BTreeMap<String, Entry>,
        pub network: BTreeMap<String, Entry>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct Entry {
        pub runs_seen: u32,
        pub hits_total: u64,
    }

    impl Profile {
        pub fn new(name: &str) -> Self {
            Self {
                name: name.into(),
                ..Default::default()
            }
        }

        pub fn has_run(&self, run_id: &str) -> bool {
            self.run_ids.contains(&run_id.to_string())
        }

        pub fn add_run_id(&mut self, run_id: String) {
            self.run_ids.push(run_id);
            if self.run_ids.len() > MAX_RUN_IDS {
                self.run_ids.remove(0);
            }
        }
    }

    pub fn stability_smoothed(runs_seen: u32, total_runs: u32, alpha: f64) -> f64 {
        if total_runs == 0 {
            return 0.0;
        }
        (runs_seen as f64 + alpha) / (total_runs as f64 + 2.0 * alpha)
    }
}

use profile_types::*;

/// Simulate merging a run into a profile
fn merge_run(profile: &mut Profile, run_id: &str, files: &[&str], network: &[&str]) {
    // Idempotency check
    if profile.has_run(run_id) {
        return;
    }

    profile.total_runs += 1;
    profile.add_run_id(run_id.to_string());

    // Merge files (deduplicated per run)
    for path in files {
        let entry = profile.files.entry(path.to_string()).or_default();
        entry.runs_seen += 1;
        entry.hits_total += 1;
    }

    // Merge network
    for dest in network {
        let entry = profile.network.entry(dest.to_string()).or_default();
        entry.runs_seen += 1;
        entry.hits_total += 1;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: Stable vs Noise
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_stable_vs_noise() {
    let mut profile = Profile::new("test-app");

    // Stable artifacts appear in every run
    let stable_file = "/etc/passwd";
    let stable_net = "db.internal:5432";

    // Noise artifact appears in only 1 run
    let noise_file = "/tmp/random-abc123.tmp";

    // Simulate 10 runs
    for i in 0..10 {
        let run_id = format!("run-{}", i);

        // Stable artifacts in every run
        let mut files = vec![stable_file];
        let network = vec![stable_net];

        // Noise only in run 0
        if i == 0 {
            files.push(noise_file);
        }

        merge_run(&mut profile, &run_id, &files, &network);
    }

    // Verify counts
    assert_eq!(profile.total_runs, 10);

    // Stable file seen in all 10 runs
    assert_eq!(profile.files[stable_file].runs_seen, 10);

    // Stable network seen in all 10 runs
    assert_eq!(profile.network[stable_net].runs_seen, 10);

    // Noise file seen in only 1 run
    assert_eq!(profile.files[noise_file].runs_seen, 1);

    // Verify stability scores (α=1.0)
    let stable_stab = stability_smoothed(10, 10, 1.0);
    let noise_stab = stability_smoothed(1, 10, 1.0);

    // Stable should be ~0.92
    assert!(
        stable_stab > 0.9,
        "stable should be >0.9, got {}",
        stable_stab
    );

    // Noise should be ~0.17
    assert!(noise_stab < 0.2, "noise should be <0.2, got {}", noise_stab);

    // With min_stability=0.8:
    // - stable_file → allow
    // - noise_file → skip (or needs_review if new_is_risky)
    assert!(stable_stab >= 0.8);
    assert!(noise_stab < 0.6);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: Idempotency
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_idempotency() {
    let mut profile = Profile::new("test");

    // Merge same run twice
    merge_run(&mut profile, "run-1", &["/a"], &[]);
    merge_run(&mut profile, "run-1", &["/a"], &[]);

    // Should only count once
    assert_eq!(profile.total_runs, 1);
    assert_eq!(profile.files["/a"].runs_seen, 1);

    // Different run should count
    merge_run(&mut profile, "run-2", &["/a"], &[]);
    assert_eq!(profile.total_runs, 2);
    assert_eq!(profile.files["/a"].runs_seen, 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: Ring Buffer
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_run_id_ring_buffer() {
    let mut profile = Profile::new("test");

    // Add more than MAX_RUN_IDS
    for i in 0..(MAX_RUN_IDS + 50) {
        merge_run(&mut profile, &format!("run-{}", i), &["/a"], &[]);
    }

    // Total runs should be all of them
    assert_eq!(profile.total_runs, (MAX_RUN_IDS + 50) as u32);

    // But run_ids should be capped
    assert_eq!(profile.run_ids.len(), MAX_RUN_IDS);

    // Old runs should be evicted from idempotency check
    // (but total_runs stays accurate)
    assert!(!profile.has_run("run-0"));
    assert!(!profile.has_run("run-49"));
    assert!(profile.has_run("run-50"));
    assert!(profile.has_run(&format!("run-{}", MAX_RUN_IDS + 49)));
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: Stability Thresholds
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_stability_thresholds() {
    // Test various scenarios
    let cases = [
        (1, 1, 0.67),   // 1 run seen, 1 total → ~0.67
        (10, 10, 0.92), // all runs → ~0.92
        (5, 10, 0.5),   // half runs → 0.5
        (0, 10, 0.08),  // never seen → ~0.08
        (8, 10, 0.75),  // 80% raw → 0.75 smoothed
        (9, 10, 0.83),  // 90% raw → 0.83 smoothed
    ];

    for (seen, total, expected) in cases {
        let actual = stability_smoothed(seen, total, 1.0);
        assert!(
            (actual - expected).abs() < 0.02,
            "stability({}/{}) expected {:.2}, got {:.2}",
            seen,
            total,
            expected,
            actual
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: Partial Stability (Flaky)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_flaky_artifact() {
    let mut profile = Profile::new("test");

    // Flaky file appears in 50% of runs
    for i in 0..10 {
        let run_id = format!("run-{}", i);
        let files = if i % 2 == 0 { vec!["/flaky"] } else { vec![] };
        merge_run(&mut profile, &run_id, &files, &[]);
    }

    assert_eq!(profile.total_runs, 10);
    assert_eq!(profile.files["/flaky"].runs_seen, 5);

    let stab = stability_smoothed(5, 10, 1.0);
    assert!((stab - 0.5).abs() < 0.01);

    // With min_stability=0.8, review_threshold=0.6:
    // - 0.5 < 0.6 → either skip or needs_review (depending on new_is_risky)
    // This is correct behavior: flaky artifacts shouldn't auto-allow
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: New Artifact in Recent Run
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_new_artifact() {
    let mut profile = Profile::new("test");

    // 9 runs without the artifact
    for i in 0..9 {
        merge_run(&mut profile, &format!("run-{}", i), &["/stable"], &[]);
    }

    // Run 10 introduces a new artifact
    merge_run(&mut profile, "run-9", &["/stable", "/new"], &[]);

    assert_eq!(profile.total_runs, 10);
    assert_eq!(profile.files["/stable"].runs_seen, 10);
    assert_eq!(profile.files["/new"].runs_seen, 1);

    let stable_stab = stability_smoothed(10, 10, 1.0);
    let new_stab = stability_smoothed(1, 10, 1.0);

    // New artifact gets low stability despite appearing in recent run
    assert!(stable_stab > 0.9);
    assert!(new_stab < 0.2);

    // This is the key insight: new artifacts need to prove themselves
    // over multiple runs before being promoted to allow
}
