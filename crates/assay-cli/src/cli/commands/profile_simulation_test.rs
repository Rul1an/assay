//! Simulation tests for multi-run stability (Phase 3)
//!
//! These tests verify:
//! 1. Stable artifacts get high stability scores
//! 2. Noise artifacts get low stability scores
//! 3. Idempotency prevents double-counting
//! 4. Run ID ring buffer works correctly

use super::profile_types::{stability_smoothed, Profile, MAX_RUN_IDS};
use std::collections::BTreeSet;

/// Simulate merging a run into a profile
fn merge_run(profile: &mut Profile, run_id: &str, files: &[&str], network: &[&str]) {
    // Idempotency check
    if profile.has_run(run_id) {
        return;
    }

    profile.total_runs += 1;
    profile.add_run_id(run_id.to_string());

    // Merge files (deduplicated per run)
    let uniq_files: BTreeSet<_> = files.iter().copied().collect();
    for path in uniq_files {
        let entry = profile.entries.files.entry(path.to_string()).or_default();
        entry.merge_run(0, 1); // timestamp=0, hits=1
    }

    // Merge network
    let uniq_net: BTreeSet<_> = network.iter().copied().collect();
    for dest in uniq_net {
        let entry = profile.entries.network.entry(dest.to_string()).or_default();
        entry.merge_run(0, 1);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: Stable vs Noise
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_stable_vs_noise() {
    let mut profile = Profile::new("test-app", None);

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
    assert_eq!(profile.entries.files[stable_file].runs_seen, 10);

    // Stable network seen in all 10 runs
    assert_eq!(profile.entries.network[stable_net].runs_seen, 10);

    // Noise file seen in only 1 run
    assert_eq!(profile.entries.files[noise_file].runs_seen, 1);

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
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: Idempotency
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_idempotency() {
    let mut profile = Profile::new("test", None);

    // Merge same run twice
    merge_run(&mut profile, "run-1", &["/a"], &[]);
    merge_run(&mut profile, "run-1", &["/a"], &[]);

    // Should only count once
    assert_eq!(profile.total_runs, 1);
    assert_eq!(profile.entries.files["/a"].runs_seen, 1);

    // Different run should count
    merge_run(&mut profile, "run-2", &["/a"], &[]);
    assert_eq!(profile.total_runs, 2);
    assert_eq!(profile.entries.files["/a"].runs_seen, 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: Ring Buffer
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_run_id_ring_buffer() {
    let mut profile = Profile::new("test", None);

    // Add more than MAX_RUN_IDS
    for i in 0..(MAX_RUN_IDS + 50) {
        merge_run(&mut profile, &format!("run-{}", i), &["/a"], &[]);
    }

    // Total runs should be all of them
    assert_eq!(profile.total_runs, (MAX_RUN_IDS + 50) as u32);

    // But run_ids should be capped
    assert_eq!(profile.run_ids.len(), MAX_RUN_IDS);

    // Check oldest and newest logic robustly
    // Old runs should be evicted
    assert!(!profile.has_run("run-0"));

    // Newest run should be present
    let newest_id = format!("run-{}", MAX_RUN_IDS + 50 - 1);
    assert!(profile.has_run(&newest_id));
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
    let mut profile = Profile::new("test", None);

    // Flaky file appears in 50% of runs
    for i in 0..10 {
        let run_id = format!("run-{}", i);
        let files = if i % 2 == 0 { vec!["/flaky"] } else { vec![] };
        merge_run(&mut profile, &run_id, &files, &[]);
    }

    assert_eq!(profile.total_runs, 10);
    assert_eq!(profile.entries.files["/flaky"].runs_seen, 5);

    let stab = stability_smoothed(5, 10, 1.0);
    assert!((stab - 0.5).abs() < 0.01);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: New Artifact in Recent Run
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_new_artifact() {
    let mut profile = Profile::new("test", None);

    // 9 runs without the artifact
    for i in 0..9 {
        merge_run(&mut profile, &format!("run-{}", i), &["/stable"], &[]);
    }

    // Run 10 introduces a new artifact
    merge_run(&mut profile, "run-9", &["/stable", "/new"], &[]);

    assert_eq!(profile.total_runs, 10);
    assert_eq!(profile.entries.files["/stable"].runs_seen, 10);
    assert_eq!(profile.entries.files["/new"].runs_seen, 1);

    let stable_stab = stability_smoothed(10, 10, 1.0);
    let new_stab = stability_smoothed(1, 10, 1.0);

    // New artifact gets low stability despite appearing in recent run
    assert!(stable_stab > 0.9);
    assert!(new_stab < 0.2);
}
