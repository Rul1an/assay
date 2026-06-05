use super::coverage::{
    enforce_bundle_input_coverage, extract_replay_coverage, extract_run_id, extract_seeds,
};
use super::paths::{
    cassette_dirs, collect_files_recursive, find_first_existing, find_run_json, find_summary_json,
    select_config_path, select_source_root, select_trace_path,
};
use crate::cli::args::{BundleArgs, BundleCreateArgs, BundleSub};
use crate::exit_codes;
use anyhow::Context;
use assay_core::replay::{
    build_file_manifest, capture_toolchain, scrub_content, verify_bundle, write_bundle_tar_gz,
    BundleEntry, ReplayCoverage, ReplayManifest, ReplayOutputs, ReplaySeeds, ScrubPolicy,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub async fn run(args: BundleArgs, _legacy_mode: bool) -> anyhow::Result<i32> {
    match args.cmd {
        BundleSub::Create(c) => cmd_create(c),
        BundleSub::Verify(v) => super::verify::cmd_verify(v),
    }
}

fn cmd_create(args: BundleCreateArgs) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let (source_root, selection_method) = select_source_root(&args, &cwd)?;
    let mut entries_map: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut outputs = ReplayOutputs {
        run: None,
        summary: None,
        junit: None,
        sarif: None,
    };
    let mut source_run_id: Option<String> = None;
    let mut seeds: Option<ReplaySeeds> = None;
    let mut replay_coverage: Option<ReplayCoverage> = None;
    let mut config_digest: Option<String> = None;
    let mut policy_digest: Option<String> = None;
    let mut baseline_digest: Option<String> = None;
    let mut trace_digest: Option<String> = None;
    let mut trace_path: Option<String> = None;
    let mut config_snapshot_present = false;
    let mut trace_snapshot_present = false;

    if let Some(run_json_path) = find_run_json(&source_root, args.from.as_ref()) {
        let run_bytes = std::fs::read(&run_json_path)?;
        entries_map.insert("outputs/run.json".to_string(), run_bytes.clone());
        outputs.run = Some("outputs/run.json".to_string());
        if let Ok(v) = serde_json::from_slice::<Value>(&run_bytes) {
            source_run_id = extract_run_id(&v);
            seeds = extract_seeds(&v);
            replay_coverage = extract_replay_coverage(&v);
            if let Some(d) = v
                .get("provenance")
                .and_then(|p| p.get("policy_pack_digest"))
                .and_then(|x| x.as_str())
            {
                policy_digest = Some(d.to_string());
            }
            if let Some(d) = v
                .get("provenance")
                .and_then(|p| p.get("baseline_digest"))
                .and_then(|x| x.as_str())
            {
                baseline_digest = Some(d.to_string());
            }
            if let Some(d) = v
                .get("provenance")
                .and_then(|p| p.get("trace_digest"))
                .and_then(|x| x.as_str())
            {
                trace_digest = Some(d.to_string());
            }
        }
    }

    if let Some(summary_path) = find_summary_json(&source_root) {
        let bytes = std::fs::read(&summary_path)?;
        entries_map.insert("outputs/summary.json".to_string(), bytes.clone());
        outputs.summary = Some("outputs/summary.json".to_string());
        if seeds.is_none() {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                seeds = extract_seeds(&v);
            }
        }
    }

    if let Some(junit_path) = find_first_existing(
        &source_root,
        &[
            PathBuf::from("junit.xml"),
            PathBuf::from(".assay/reports/junit.xml"),
        ],
    ) {
        let bytes = std::fs::read(&junit_path)?;
        entries_map.insert("outputs/junit.xml".to_string(), bytes);
        outputs.junit = Some("outputs/junit.xml".to_string());
    }
    if let Some(sarif_path) = find_first_existing(
        &source_root,
        &[
            PathBuf::from("sarif.json"),
            PathBuf::from(".assay/reports/sarif.json"),
        ],
    ) {
        let bytes = std::fs::read(&sarif_path)?;
        entries_map.insert("outputs/sarif.json".to_string(), bytes);
        outputs.sarif = Some("outputs/sarif.json".to_string());
    }

    if let Some(config_path) = select_config_path(&args, &source_root) {
        let bytes = std::fs::read(&config_path).with_context(|| {
            format!(
                "failed to read config for bundle: {}",
                config_path.display()
            )
        })?;
        let name = config_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("eval.yaml");
        entries_map.insert(format!("files/{}", name), bytes.clone());
        config_snapshot_present = true;
        let hash = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
        if config_digest.is_none() {
            config_digest = Some(hash.clone());
        }
        if policy_digest.is_none() {
            policy_digest = Some(hash);
        }
    }

    if let Some(t_path) = select_trace_path(&args, &source_root) {
        let bytes = std::fs::read(&t_path)
            .with_context(|| format!("failed to read trace for bundle: {}", t_path.display()))?;
        let name = t_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("trace.jsonl");
        let bundle_trace_path = format!("files/{}", name);
        entries_map.insert(bundle_trace_path.clone(), bytes.clone());
        trace_snapshot_present = true;
        if trace_digest.is_none() {
            trace_digest = Some(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))));
        }
        trace_path = Some(bundle_trace_path);
    }

    for cassette_dir in cassette_dirs(&source_root) {
        if cassette_dir.exists() {
            let files = collect_files_recursive(&cassette_dir)?;
            for path in files {
                let rel = match path.strip_prefix(&cassette_dir) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let rel_posix = rel.to_string_lossy().replace('\\', "/");
                let bundle_path = format!("cassettes/{}", rel_posix);
                let raw = std::fs::read(&path)?;
                entries_map
                    .entry(bundle_path)
                    .or_insert_with(|| scrub_content(&raw));
            }
        }
    }

    let mut entries = Vec::new();
    for (path, data) in entries_map {
        entries.push(BundleEntry { path, data });
    }
    replay_coverage = replay_coverage.map(|coverage| {
        enforce_bundle_input_coverage(coverage, config_snapshot_present, trace_snapshot_present)
    });
    let file_manifest = build_file_manifest(&entries)?;

    let run_id = args
        .run_id
        .clone()
        .or(source_run_id.clone())
        .unwrap_or_else(|| "latest".to_string());

    let out_path = args
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!(".assay/bundles/{}.tar.gz", run_id)));

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut manifest = ReplayManifest::minimal(env!("CARGO_PKG_VERSION").to_string());
    manifest.created_at = Some(chrono::Utc::now().to_rfc3339());
    manifest.source_run_path = Some(source_root.to_string_lossy().to_string());
    manifest.selection_method = Some(selection_method);
    if outputs.run.is_some()
        || outputs.summary.is_some()
        || outputs.junit.is_some()
        || outputs.sarif.is_some()
    {
        manifest.outputs = Some(outputs);
    }
    manifest.toolchain = Some(capture_toolchain());
    manifest.seeds = seeds;
    manifest.scrub_policy = Some(ScrubPolicy::default());
    manifest.workflow_run_id = std::env::var("GITHUB_RUN_ID").ok();
    manifest.config_digest = config_digest;
    manifest.trace_path = trace_path;
    manifest.trace_digest = trace_digest;
    manifest.policy_digest = policy_digest;
    manifest.baseline_digest = baseline_digest;
    manifest.replay_coverage = replay_coverage;
    manifest.files = Some(file_manifest);

    let f = std::fs::File::create(&out_path)?;
    write_bundle_tar_gz(f, &manifest, &entries)?;

    // Always verify newly-created bundles before reporting success.
    let verify = verify_bundle(std::fs::File::open(&out_path)?)?;
    for w in &verify.warnings {
        eprintln!("warning: {}", w);
    }
    if !verify.errors.is_empty() {
        for e in &verify.errors {
            eprintln!("error: {}", e);
        }
        anyhow::bail!(
            "bundle create failed verification: {} ({} error(s))",
            out_path.display(),
            verify.errors.len()
        );
    }

    if verify.warnings.is_empty() {
        eprintln!("bundle created: {}", out_path.display());
    } else {
        eprintln!(
            "bundle created with warnings: {} ({} warning(s))",
            out_path.display(),
            verify.warnings.len()
        );
    }
    Ok(exit_codes::EXIT_SUCCESS)
}
