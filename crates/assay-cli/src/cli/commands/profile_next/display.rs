use std::collections::BTreeMap;

use crate::cli::commands::profile_types::{
    stability_smoothed, Profile, ProfileEntry, DEFAULT_ALPHA,
};

pub(super) fn show_summary(profile: &Profile, top_n: usize) {
    println!("Profile: {}", profile.name);
    println!("Version: {}", profile.version);
    if let Some(scope) = &profile.scope {
        println!("Scope: {}", scope);
    }
    println!("Created: {}", profile.created_at);
    println!("Updated: {}", profile.updated_at);
    println!("Total runs: {}", profile.total_runs);
    println!();
    println!("Entries:");
    println!("  Files: {}", profile.entries.files.len());
    println!("  Network: {}", profile.entries.network.len());
    println!("  Processes: {}", profile.entries.processes.len());
    println!();

    if profile.total_runs > 0 {
        println!("Stability distribution (α=1.0):");
        show_stability_distribution(&profile.entries.files, profile.total_runs, "  Files");
        show_stability_distribution(&profile.entries.network, profile.total_runs, "  Network");
        show_stability_distribution(
            &profile.entries.processes,
            profile.total_runs,
            "  Processes",
        );
        println!();

        println!("Top {} most stable files:", top_n);
        show_top_stable(&profile.entries.files, profile.total_runs, top_n);

        if !profile.entries.network.is_empty() {
            println!("\nTop {} most stable network destinations:", top_n);
            show_top_stable(&profile.entries.network, profile.total_runs, top_n);
        }
    }
}

fn show_stability_distribution(
    entries: &BTreeMap<String, ProfileEntry>,
    total_runs: u32,
    label: &str,
) {
    if entries.is_empty() {
        return;
    }

    let mut high = 0; // >= 0.8
    let mut mid = 0; // 0.6-0.8
    let mut low = 0; // < 0.6

    for entry in entries.values() {
        let s = stability_smoothed(entry.runs_seen, total_runs, DEFAULT_ALPHA);
        if s >= 0.8 {
            high += 1;
        } else if s >= 0.6 {
            mid += 1;
        } else {
            low += 1;
        }
    }

    println!(
        "{}: {} stable (≥0.8), {} medium (0.6-0.8), {} low (<0.6)",
        label, high, mid, low
    );
}

fn show_top_stable(entries: &BTreeMap<String, ProfileEntry>, total_runs: u32, n: usize) {
    let mut sorted: Vec<_> = entries
        .iter()
        .map(|(k, v)| {
            (
                k,
                v,
                stability_smoothed(v.runs_seen, total_runs, DEFAULT_ALPHA),
            )
        })
        .collect();

    sorted.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

    for (key, entry, stab) in sorted.into_iter().take(n) {
        let key_short = if key.len() > 50 { &key[..50] } else { key };
        println!(
            "  {:.2} ({:>2}/{:>2}) {}",
            stab, entry.runs_seen, total_runs, key_short
        );
    }
}
