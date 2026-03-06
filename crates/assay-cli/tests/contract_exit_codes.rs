#![allow(deprecated)]
use assay_core::replay::{
    build_file_manifest, read_bundle_tar_gz, write_bundle_tar_gz, BundleEntry, ReplayCoverage,
    ReplayManifest,
};
use assert_cmd::Command;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use tempfile::tempdir;

fn read_run_json(dir: &std::path::Path) -> Value {
    let path = dir.join("run.json");
    if !path.exists() {
        panic!("run.json missing in {}", dir.display());
    }
    let content = fs::read_to_string(&path).unwrap();
    serde_json::from_str(&content).expect("Invalid JSON in run.json")
}

fn read_summary_json(dir: &std::path::Path) -> Value {
    let path = dir.join("summary.json");
    if !path.exists() {
        panic!("summary.json missing in {}", dir.display());
    }
    let content = fs::read_to_string(&path).unwrap();
    serde_json::from_str(&content).expect("Invalid JSON in summary.json")
}

fn assert_schema(v: &Value) {
    assert!(
        v.get("exit_code").expect("missing exit_code").is_i64(),
        "exit_code must be int"
    );
    assert!(
        v.get("reason_code")
            .expect("missing reason_code")
            .is_string(),
        "reason_code must be string"
    );
    if let Some(w) = v.get("warnings") {
        let arr = w.as_array().expect("warnings must be array");
        for item in arr {
            assert!(item.is_string(), "warning items must be strings");
        }
    }
}

/// E7.2: Early-exit run.json must have seed_version present; order_seed/judge_seed keys present and null.
fn assert_run_json_seeds_early_exit(v: &Value) {
    assert_eq!(
        v.get("seed_version").and_then(Value::as_u64),
        Some(1),
        "run.json must have seed_version == 1"
    );
    assert!(v.get("order_seed").is_some(), "order_seed key must exist");
    assert!(v.get("judge_seed").is_some(), "judge_seed key must exist");
    assert!(
        v["order_seed"].is_null(),
        "order_seed must be null on early exit"
    );
    assert!(
        v["judge_seed"].is_null(),
        "judge_seed must be null on early exit"
    );
}

/// E7.2: Successful run run.json: seed_version 1; order_seed string (no number precision loss); judge_seed key present (null until implemented).
fn assert_run_json_seeds_happy(v: &Value) {
    assert_eq!(
        v.get("seed_version").and_then(Value::as_u64),
        Some(1),
        "run.json must have seed_version == 1"
    );
    assert!(
        v["order_seed"].is_string(),
        "order_seed must be string to avoid JSON precision loss"
    );
    assert!(v.get("judge_seed").is_some(), "judge_seed key must exist");
    assert!(
        v["judge_seed"].is_null(),
        "judge_seed reserved, must be null until implemented"
    );
}

/// E7.2: Early-exit summary.json must have seeds with seed_version; order_seed/judge_seed keys present (null or string).
fn assert_summary_seeds_early_exit(v: &Value) {
    let seeds = v
        .get("seeds")
        .expect("summary.json must have seeds on early exit");
    assert_eq!(
        seeds.get("seed_version").and_then(Value::as_u64),
        Some(1),
        "summary seeds must have seed_version == 1"
    );
    assert!(
        seeds.get("order_seed").is_some(),
        "order_seed key must exist"
    );
    assert!(
        seeds.get("judge_seed").is_some(),
        "judge_seed key must exist"
    );
    assert!(
        seeds["order_seed"].is_null() || seeds["order_seed"].is_string(),
        "order_seed must be string or null"
    );
    assert!(
        seeds["judge_seed"].is_null() || seeds["judge_seed"].is_string(),
        "judge_seed must be string or null"
    );
}

/// E7.2: Successful run summary.json: seeds with seed_version; order_seed string, judge_seed null (reserved).
fn assert_summary_seeds_happy(v: &Value) {
    let seeds = v
        .get("seeds")
        .expect("summary.json must have seeds on success");
    assert_eq!(
        seeds.get("seed_version").and_then(Value::as_u64),
        Some(1),
        "summary seeds must have seed_version == 1"
    );
    assert!(
        seeds["order_seed"].is_string(),
        "summary seeds.order_seed must be string (no precision loss)"
    );
    assert!(
        seeds.get("judge_seed").is_some(),
        "judge_seed key must exist"
    );
    assert!(
        seeds["judge_seed"].is_null(),
        "judge_seed reserved, null until implemented"
    );
}

#[cfg(test)]
#[path = "exit_codes/core.rs"]
mod core;
#[cfg(test)]
#[path = "exit_codes/replay.rs"]
mod replay;

fn test_status_map(run_json: &Value) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let Some(rows) = run_json.get("results").and_then(Value::as_array) else {
        return out;
    };
    for row in rows {
        let Some(test_id) = row.get("test_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(status) = row.get("status").and_then(Value::as_str) else {
            continue;
        };
        out.insert(test_id.to_string(), status.to_string());
    }
    out
}
