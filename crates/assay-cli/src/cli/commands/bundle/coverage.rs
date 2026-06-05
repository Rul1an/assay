use assay_core::replay::{ReplayCoverage, ReplaySeeds};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn extract_run_id(v: &Value) -> Option<String> {
    if let Some(id) = v.get("run_id") {
        if let Some(s) = id.as_str() {
            return Some(s.to_string());
        }
        if let Some(n) = id.as_i64() {
            return Some(n.to_string());
        }
        if let Some(n) = id.as_u64() {
            return Some(n.to_string());
        }
    }
    None
}

pub(super) fn extract_seeds(v: &Value) -> Option<ReplaySeeds> {
    // summary.json style
    if let Some(seeds) = v.get("seeds") {
        let seed_version = seeds
            .get("seed_version")
            .and_then(|x| x.as_u64())
            .map(|x| x as u32);
        let order_seed = seed_to_string(seeds.get("order_seed"));
        let judge_seed = seed_to_string(seeds.get("judge_seed"));
        return Some(ReplaySeeds {
            seed_version,
            order_seed,
            judge_seed,
        });
    }
    // run.json style
    let seed_version = v
        .get("seed_version")
        .and_then(|x| x.as_u64())
        .map(|x| x as u32);
    let order_seed = seed_to_string(v.get("order_seed"));
    let judge_seed = seed_to_string(v.get("judge_seed"));
    if seed_version.is_some() || order_seed.is_some() || judge_seed.is_some() {
        return Some(ReplaySeeds {
            seed_version,
            order_seed,
            judge_seed,
        });
    }
    None
}

pub(super) fn extract_replay_coverage(v: &Value) -> Option<ReplayCoverage> {
    let results = v.get("results")?.as_array()?;
    let mut complete_tests = Vec::new();
    let mut incomplete_tests = Vec::new();
    let mut reason = BTreeMap::new();

    for row in results {
        let Some(test_id) = row.get("test_id").and_then(|x| x.as_str()) else {
            continue;
        };
        let status = row
            .get("status")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let msg = row
            .get("message")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim();

        if status == "error" {
            incomplete_tests.push(test_id.to_string());
            if !msg.is_empty() {
                reason.insert(test_id.to_string(), truncate_reason(msg, 240));
            }
        } else {
            complete_tests.push(test_id.to_string());
        }
    }

    if complete_tests.is_empty() && incomplete_tests.is_empty() {
        return None;
    }

    Some(ReplayCoverage {
        complete_tests,
        incomplete_tests,
        reason: if reason.is_empty() {
            None
        } else {
            Some(reason)
        },
    })
}

pub(super) fn enforce_bundle_input_coverage(
    mut coverage: ReplayCoverage,
    config_snapshot_present: bool,
    trace_snapshot_present: bool,
) -> ReplayCoverage {
    // "complete_tests" are only complete when mandatory replay inputs are present in bundle.
    if !config_snapshot_present {
        mark_all_incomplete(&mut coverage, "config snapshot missing from bundle");
    }
    if !trace_snapshot_present {
        mark_all_incomplete(&mut coverage, "trace snapshot missing from bundle");
    }
    coverage
}

fn mark_all_incomplete(coverage: &mut ReplayCoverage, reason_message: &str) {
    let mut all = BTreeSet::new();
    for test_id in &coverage.complete_tests {
        all.insert(test_id.clone());
    }
    for test_id in &coverage.incomplete_tests {
        all.insert(test_id.clone());
    }

    if all.is_empty() {
        return;
    }

    let reason = coverage.reason.get_or_insert_with(BTreeMap::new);
    for test_id in &all {
        reason
            .entry(test_id.clone())
            .or_insert_with(|| reason_message.to_string());
    }

    coverage.complete_tests.clear();
    coverage.incomplete_tests = all.into_iter().collect();
}

fn truncate_reason(message: &str, max_chars: usize) -> String {
    if message.chars().count() <= max_chars {
        return message.to_string();
    }
    message.chars().take(max_chars).collect()
}

fn seed_to_string(v: Option<&Value>) -> Option<String> {
    let v = v?;
    if v.is_null() {
        return None;
    }
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if let Some(n) = v.as_u64() {
        return Some(n.to_string());
    }
    None
}
