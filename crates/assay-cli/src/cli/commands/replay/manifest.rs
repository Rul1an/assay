use assay_core::replay::ReplayManifest;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(super) fn offline_dependency_message(manifest: &ReplayManifest) -> Option<String> {
    let coverage = manifest.replay_coverage.as_ref()?;
    if coverage.incomplete_tests.is_empty() {
        return None;
    }

    let first = coverage.incomplete_tests[0].clone();
    let first_reason = coverage
        .reason
        .as_ref()
        .and_then(|m| m.get(&first))
        .cloned()
        .unwrap_or_else(|| "missing dependency".to_string());

    Some(format!(
        "offline replay blocked: {} incomplete test(s); first={} reason={}",
        coverage.incomplete_tests.len(),
        first,
        first_reason
    ))
}

pub(super) fn resolve_config_path(
    manifest: &ReplayManifest,
    entries: &[(String, Vec<u8>)],
    workspace: &Path,
) -> Option<PathBuf> {
    let mut candidates = BTreeSet::new();

    candidates.insert("files/eval.yaml".to_string());
    candidates.insert("files/assay.yaml".to_string());

    if let Some(files) = &manifest.files {
        for path in files.keys() {
            if is_config_candidate(path) {
                candidates.insert(path.clone());
            }
        }
    }

    for (path, _) in entries {
        if is_config_candidate(path) {
            candidates.insert(path.clone());
        }
        if path.starts_with("files/") && is_yaml_or_json(path) {
            candidates.insert(path.clone());
        }
    }

    candidates
        .into_iter()
        .map(|p| workspace.join(p))
        .find(|p| p.exists() && p.is_file())
}

pub(super) fn resolve_trace_path(
    manifest: &ReplayManifest,
    entries: &[(String, Vec<u8>)],
    workspace: &Path,
) -> Option<PathBuf> {
    let mut candidates = BTreeSet::new();

    if let Some(trace_path) = &manifest.trace_path {
        candidates.insert(trace_path.clone());
    }

    candidates.insert("files/trace.jsonl".to_string());
    candidates.insert("files/ci.jsonl".to_string());
    candidates.insert("files/run.jsonl".to_string());

    for (path, _) in entries {
        if path.starts_with("files/") && path.ends_with(".jsonl") {
            candidates.insert(path.clone());
        }
    }

    candidates
        .into_iter()
        .map(|p| workspace.join(p))
        .find(|p| p.exists() && p.is_file())
}

fn is_config_candidate(path: &str) -> bool {
    if !path.starts_with("files/") {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    if !(lower.ends_with(".yaml") || lower.ends_with(".yml") || lower.ends_with(".json")) {
        return false;
    }

    lower.ends_with("eval.yaml")
        || lower.ends_with("eval.yml")
        || lower.ends_with("assay.yaml")
        || lower.ends_with("assay.yml")
        || lower.ends_with("config.yaml")
        || lower.ends_with("config.yml")
        || lower.ends_with("config.json")
}

fn is_yaml_or_json(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".yaml") || lower.ends_with(".yml") || lower.ends_with(".json")
}

pub(super) fn source_run_id_from_bundle(
    manifest: &ReplayManifest,
    entries: &[(String, Vec<u8>)],
) -> Option<String> {
    if let Some(outputs) = &manifest.outputs {
        if let Some(run_path) = &outputs.run {
            if let Some(id) = run_id_from_entry(run_path, entries) {
                return Some(id);
            }
        }
    }

    run_id_from_entry("outputs/run.json", entries)
}

fn run_id_from_entry(path: &str, entries: &[(String, Vec<u8>)]) -> Option<String> {
    let (_, data) = entries.iter().find(|(p, _)| p == path)?;
    let value: serde_json::Value = serde_json::from_slice(data).ok()?;
    let run_id = value.get("run_id")?;
    if let Some(s) = run_id.as_str() {
        return Some(s.to_string());
    }
    if let Some(n) = run_id.as_i64() {
        return Some(n.to_string());
    }
    if let Some(n) = run_id.as_u64() {
        return Some(n.to_string());
    }
    None
}
