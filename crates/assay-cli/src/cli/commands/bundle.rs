use super::super::args::{BundleArgs, BundleCreateArgs, BundleSub, BundleVerifyArgs};
use crate::exit_codes;
use anyhow::Context;
use assay_core::replay::{
    build_file_manifest, capture_toolchain, scrub_content, verify_bundle, write_bundle_tar_gz,
    BundleEntry, ReplayCoverage, ReplayManifest, ReplayOutputs, ReplaySeeds, ScrubPolicy,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub async fn run(args: BundleArgs, _legacy_mode: bool) -> anyhow::Result<i32> {
    match args.cmd {
        BundleSub::Create(c) => cmd_create(c),
        BundleSub::Verify(v) => cmd_verify(v),
    }
}

fn cmd_verify(args: BundleVerifyArgs) -> anyhow::Result<i32> {
    let file = std::fs::File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle: {}", args.bundle.display()))?;
    let res = verify_bundle(file)?;
    for w in &res.warnings {
        eprintln!("warning: {}", w);
    }
    if !res.errors.is_empty() {
        for e in &res.errors {
            eprintln!("error: {}", e);
        }
        return Ok(exit_codes::EXIT_CONFIG_ERROR);
    }
    eprintln!("bundle verify: OK ({})", args.bundle.display());
    Ok(exit_codes::EXIT_SUCCESS)
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
        anyhow::bail!("created bundle failed verification: {}", out_path.display());
    }

    eprintln!("bundle created: {}", out_path.display());
    Ok(exit_codes::EXIT_SUCCESS)
}

fn select_source_root(args: &BundleCreateArgs, cwd: &Path) -> anyhow::Result<(PathBuf, String)> {
    if let Some(from) = &args.from {
        let p = absolutize(from, cwd);
        if !p.exists() {
            anyhow::bail!("--from path does not exist: {}", p.display());
        }
        if p.is_file() {
            let parent = p
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| cwd.to_path_buf());
            return Ok((parent, "explicit-from".to_string()));
        }
        return Ok((p, "explicit-from".to_string()));
    }
    if let Some(run_id) = &args.run_id {
        for rel in &[
            format!(".assay/{}", run_id),
            format!(".assay/run_{}", run_id),
            format!(".assay/runs/{}", run_id),
        ] {
            let p = cwd.join(rel);
            if p.exists() {
                return Ok((p, "run-id".to_string()));
            }
        }
        anyhow::bail!(
            "--run-id was provided but no matching path exists under .assay for id {}",
            run_id
        );
    }
    if let Some(latest) = find_latest_run_json(cwd)? {
        let parent = latest
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| cwd.to_path_buf());
        return Ok((parent, "mtime-latest".to_string()));
    }
    Ok((cwd.to_path_buf(), "cwd-fallback".to_string()))
}

fn find_latest_run_json(root: &Path) -> anyhow::Result<Option<PathBuf>> {
    let candidates = collect_named_files(root, "run.json", 6)?;
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for p in candidates {
        let meta = match std::fs::metadata(&p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mtime = match meta.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };
        match &best {
            Some((bt, _)) if &mtime <= bt => {}
            _ => best = Some((mtime, p)),
        }
    }
    Ok(best.map(|(_, p)| p))
}

fn find_run_json(source_root: &Path, from: Option<&PathBuf>) -> Option<PathBuf> {
    if let Some(from) = from {
        if from.is_file() && from.file_name().and_then(|x| x.to_str()) == Some("run.json") {
            return Some(from.clone());
        }
    }
    find_first_existing(
        source_root,
        &[PathBuf::from("run.json"), PathBuf::from(".assay/run.json")],
    )
}

fn find_summary_json(source_root: &Path) -> Option<PathBuf> {
    find_first_existing(
        source_root,
        &[
            PathBuf::from("summary.json"),
            PathBuf::from(".assay/summary.json"),
        ],
    )
}

fn select_config_path(args: &BundleCreateArgs, source_root: &Path) -> Option<PathBuf> {
    if let Some(cfg) = &args.config {
        return Some(cfg.clone());
    }
    find_first_existing(
        source_root,
        &[PathBuf::from("eval.yaml"), PathBuf::from("assay.yaml")],
    )
}

fn select_trace_path(args: &BundleCreateArgs, source_root: &Path) -> Option<PathBuf> {
    if let Some(t) = &args.trace_file {
        return Some(t.clone());
    }
    find_first_existing(
        source_root,
        &[
            PathBuf::from("trace.jsonl"),
            PathBuf::from("traces/ci.jsonl"),
            PathBuf::from("traces/run.jsonl"),
            PathBuf::from("traces/trace.jsonl"),
        ],
    )
}

fn find_first_existing(source_root: &Path, candidates: &[PathBuf]) -> Option<PathBuf> {
    for c in candidates {
        let p = if c.is_absolute() {
            c.clone()
        } else {
            source_root.join(c)
        };
        if p.exists() && p.is_file() {
            return Some(p);
        }
    }
    None
}

fn collect_named_files(
    root: &Path,
    needle: &str,
    max_depth: usize,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_named_files_inner(root, needle, max_depth, 0, &mut out)?;
    Ok(out)
}

fn collect_named_files_inner(
    dir: &Path,
    needle: &str,
    max_depth: usize,
    depth: usize,
    out: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    if depth > max_depth {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if should_skip_recursive_dir(name) {
                continue;
            }
            collect_named_files_inner(&p, needle, max_depth, depth + 1, out)?;
        } else if ft.is_file() && p.file_name().and_then(|s| s.to_str()) == Some(needle) {
            out.push(p);
        }
    }
    Ok(())
}

fn collect_files_recursive(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_files_recursive_inner(root, &mut out)?;
    Ok(out)
}

fn collect_files_recursive_inner(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if should_skip_recursive_dir(name) {
                continue;
            }
            collect_files_recursive_inner(&path, out)?;
        } else if ft.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn should_skip_recursive_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".venv" | "venv" | "__pycache__" | "dist" | "build"
    )
}

fn cassette_dirs(source_root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![
        source_root.join("cassettes"),
        source_root.join(".assay/cassettes"),
        source_root.join(".assay/vcr"),
    ];
    if let Ok(vcr_dir) = std::env::var("ASSAY_VCR_DIR") {
        let p = PathBuf::from(vcr_dir);
        if p.is_absolute() {
            dirs.push(p);
        } else {
            dirs.push(source_root.join(p));
        }
    }
    dirs
}

fn extract_run_id(v: &Value) -> Option<String> {
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

fn extract_seeds(v: &Value) -> Option<ReplaySeeds> {
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

fn extract_replay_coverage(v: &Value) -> Option<ReplayCoverage> {
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

fn absolutize(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}
