use super::super::args::{JudgeArgs, ReplayArgs, RunArgs};
use crate::exit_codes::{ReasonCode, RunOutcome};
use anyhow::Context;
use assay_core::replay::{read_bundle_tar_gz, verify_bundle, ReplayManifest};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub async fn run(args: ReplayArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    let bundle_digest = match sha256_file(&args.bundle) {
        Ok(d) => d,
        Err(err) => {
            eprintln!(
                "warning: failed to compute bundle digest for {}: {}; using sha256:unknown",
                args.bundle.display(),
                err
            );
            "sha256:unknown".to_string()
        }
    };
    let replay_mode = if args.live { "live" } else { "offline" };

    let file = match std::fs::File::open(&args.bundle) {
        Ok(file) => file,
        Err(err) => {
            return write_replay_failure(
                &args,
                &bundle_digest,
                replay_mode,
                None,
                ReasonCode::ECfgParse,
                format!("failed to open bundle {}: {}", args.bundle.display(), err),
                None,
            );
        }
    };
    let verify = match verify_bundle(file) {
        Ok(v) => v,
        Err(err) => {
            return write_replay_failure(
                &args,
                &bundle_digest,
                replay_mode,
                None,
                ReasonCode::ECfgParse,
                format!("failed to verify bundle: {}", err),
                None,
            );
        }
    };
    for warning in &verify.warnings {
        eprintln!("warning: {}", warning);
    }
    if !verify.errors.is_empty() {
        for error in &verify.errors {
            eprintln!("error: {}", error);
        }
        let first = verify
            .errors
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown verify error".to_string());
        return write_replay_failure(
            &args,
            &bundle_digest,
            replay_mode,
            None,
            ReasonCode::ECfgParse,
            format!(
                "replay bundle verification failed ({} error(s)); first={}",
                verify.errors.len(),
                first
            ),
            None,
        );
    }

    let file = match std::fs::File::open(&args.bundle) {
        Ok(file) => file,
        Err(err) => {
            return write_replay_failure(
                &args,
                &bundle_digest,
                replay_mode,
                None,
                ReasonCode::ECfgParse,
                format!(
                    "failed to open verified bundle {}: {}",
                    args.bundle.display(),
                    err
                ),
                None,
            );
        }
    };
    let read = match read_bundle_tar_gz(file) {
        Ok(read) => read,
        Err(err) => {
            return write_replay_failure(
                &args,
                &bundle_digest,
                replay_mode,
                None,
                ReasonCode::ECfgParse,
                format!("failed to read replay bundle: {}", err),
                None,
            );
        }
    };
    let source_run_id = source_run_id_from_bundle(&read.manifest, &read.entries);

    if !args.live {
        if let Some(msg) = offline_dependency_message(&read.manifest) {
            return write_missing_dependency(
                &args,
                &bundle_digest,
                replay_mode,
                source_run_id,
                msg,
            );
        }
    }

    let workspace = match ReplayWorkspace::new() {
        Ok(workspace) => workspace,
        Err(err) => {
            return write_replay_failure(
                &args,
                &bundle_digest,
                replay_mode,
                source_run_id.clone(),
                ReasonCode::ECfgParse,
                format!("failed to create replay workspace: {}", err),
                None,
            );
        }
    };
    if let Err(err) = write_entries(workspace.path(), &read.entries) {
        return write_replay_failure(
            &args,
            &bundle_digest,
            replay_mode,
            source_run_id.clone(),
            ReasonCode::ECfgParse,
            format!("failed to materialize replay bundle contents: {}", err),
            None,
        );
    }

    let config_path = match resolve_config_path(&read.manifest, &read.entries, workspace.path()) {
        Some(p) => p,
        None => {
            return write_missing_dependency(
                &args,
                &bundle_digest,
                replay_mode,
                source_run_id,
                "Replay bundle missing config snapshot under files/".to_string(),
            )
        }
    };

    let trace_path = resolve_trace_path(&read.manifest, &read.entries, workspace.path());
    if !args.live && trace_path.is_none() {
        return write_missing_dependency(
            &args,
            &bundle_digest,
            replay_mode,
            source_run_id.clone(),
            "Replay bundle missing trace required for offline replay".to_string(),
        );
    }

    if let Some(seed) = args.seed {
        if let Err(err) = apply_seed_override(&config_path, seed) {
            return write_replay_failure(
                &args,
                &bundle_digest,
                replay_mode,
                source_run_id.clone(),
                ReasonCode::ECfgParse,
                format!("failed to apply seed override: {}", err),
                None,
            );
        }
    }

    let run_args = replay_run_args(
        config_path,
        trace_path,
        workspace.path().join("replay.db"),
        !args.live,
        args.exit_codes,
    );

    let exit_code = match super::run::run(run_args, legacy_mode).await {
        Ok(code) => code,
        Err(err) => {
            return write_replay_failure(
                &args,
                &bundle_digest,
                replay_mode,
                source_run_id.clone(),
                ReasonCode::ECfgParse,
                format!("replay execution failed: {}", err),
                None,
            );
        }
    };

    if let Err(err) = annotate_replay_outputs(&bundle_digest, replay_mode, source_run_id) {
        eprintln!("warning: failed to annotate replay provenance: {}", err);
    }

    Ok(exit_code)
}

fn replay_run_args(
    config: PathBuf,
    trace_file: Option<PathBuf>,
    db: PathBuf,
    replay_strict: bool,
    exit_codes: crate::exit_codes::ExitCodeVersion,
) -> RunArgs {
    let judge = JudgeArgs {
        no_judge: true,
        ..JudgeArgs::default()
    };

    RunArgs {
        config,
        db,
        quarantine_mode: "off".to_string(),
        trace_file,
        refresh_cache: true,
        no_cache: true,
        judge,
        replay_strict,
        exit_codes,
        ..RunArgs::default()
    }
}

fn write_missing_dependency(
    args: &ReplayArgs,
    bundle_digest: &str,
    replay_mode: &str,
    source_run_id: Option<String>,
    message: String,
) -> anyhow::Result<i32> {
    write_replay_failure(
        args,
        bundle_digest,
        replay_mode,
        source_run_id,
        ReasonCode::EReplayMissingDependency,
        message,
        Some("assay replay --bundle <path> --live"),
    )
}

fn write_replay_failure(
    args: &ReplayArgs,
    bundle_digest: &str,
    replay_mode: &str,
    source_run_id: Option<String>,
    reason: ReasonCode,
    message: String,
    next_step_override: Option<&str>,
) -> anyhow::Result<i32> {
    let mut outcome = RunOutcome::from_reason(reason, Some(message), None);
    if let Some(next_step) = next_step_override {
        outcome.next_step = Some(next_step.to_string());
    }
    outcome.exit_code = reason.exit_code_for(args.exit_codes);

    let run_json_path = PathBuf::from("run.json");
    if let Err(err) = super::run_output::write_run_json_minimal(&outcome, &run_json_path) {
        eprintln!("warning: failed to write run.json: {}", err);
    }
    if let Err(err) = annotate_run_json_provenance(
        &run_json_path,
        bundle_digest,
        replay_mode,
        source_run_id.as_deref(),
    ) {
        eprintln!("warning: failed to annotate run.json provenance: {}", err);
    }

    let summary_path = PathBuf::from("summary.json");
    // Explicit early-exit seed policy: null seeds because replay run did not execute.
    let summary = super::run_output::summary_from_outcome(&outcome, true)
        .with_seeds(None, None)
        .with_replay_provenance(bundle_digest.to_string(), replay_mode, source_run_id);
    if let Err(err) = assay_core::report::summary::write_summary(&summary, &summary_path) {
        eprintln!("warning: failed to write summary.json: {}", err);
    }

    Ok(outcome.exit_code)
}

fn offline_dependency_message(manifest: &ReplayManifest) -> Option<String> {
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

fn resolve_config_path(
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

fn resolve_trace_path(
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

fn apply_seed_override(config_path: &Path, seed: u64) -> anyhow::Result<()> {
    let ext = config_path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if ext == "json" {
        let raw = std::fs::read(config_path)?;
        let mut root: serde_json::Value = serde_json::from_slice(&raw)
            .context("failed to parse JSON config for seed override")?;
        let Some(obj) = root.as_object_mut() else {
            anyhow::bail!("JSON config root must be object for seed override");
        };
        let settings = obj
            .entry("settings".to_string())
            .or_insert_with(|| serde_json::json!({}));
        let Some(settings_obj) = settings.as_object_mut() else {
            anyhow::bail!("JSON config settings must be object for seed override");
        };
        settings_obj.insert("seed".to_string(), serde_json::json!(seed));
        write_file_atomic(config_path, serde_json::to_string_pretty(&root)?.as_bytes())?;
        return Ok(());
    }

    let raw = std::fs::read_to_string(config_path)?;
    let mut root: serde_yaml::Value =
        serde_yaml::from_str(&raw).context("failed to parse YAML config for seed override")?;
    let Some(root_map) = root.as_mapping_mut() else {
        anyhow::bail!("YAML config root must be mapping for seed override");
    };

    let settings_key = serde_yaml::Value::String("settings".to_string());
    if !root_map.contains_key(&settings_key) {
        root_map.insert(
            settings_key.clone(),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
    }

    if !root_map
        .get(&settings_key)
        .map(|v| v.is_mapping())
        .unwrap_or(false)
    {
        root_map.insert(
            settings_key.clone(),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
    }

    let Some(settings) = root_map
        .get_mut(&settings_key)
        .and_then(serde_yaml::Value::as_mapping_mut)
    else {
        anyhow::bail!("YAML config settings must be mapping for seed override");
    };
    settings.insert(
        serde_yaml::Value::String("seed".to_string()),
        serde_yaml::to_value(seed)?,
    );

    write_file_atomic(config_path, serde_yaml::to_string(&root)?.as_bytes())?;
    Ok(())
}

fn write_file_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent: {}", path.display()))?;
    let base = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("path has no filename: {}", path.display()))?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("clock before UNIX_EPOCH")?
        .as_nanos();
    let tmp_name = format!(".{}.tmp-{}-{}", base, std::process::id(), stamp);
    let tmp_path = parent.join(tmp_name);

    std::fs::write(&tmp_path, bytes)
        .with_context(|| format!("failed writing temp file {}", tmp_path.display()))?;
    if let Err(err) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err).with_context(|| {
            format!(
                "failed replacing {} with temp file {}",
                path.display(),
                tmp_path.display()
            )
        });
    }
    Ok(())
}

fn annotate_replay_outputs(
    bundle_digest: &str,
    replay_mode: &str,
    source_run_id: Option<String>,
) -> anyhow::Result<()> {
    let run_json_path = PathBuf::from("run.json");
    if run_json_path.exists() {
        annotate_run_json_provenance(
            &run_json_path,
            bundle_digest,
            replay_mode,
            source_run_id.as_deref(),
        )?;
    }

    let summary_path = PathBuf::from("summary.json");
    if summary_path.exists() {
        let raw = std::fs::read(&summary_path)?;
        let summary: assay_core::report::summary::Summary = serde_json::from_slice(&raw)
            .context("failed to parse summary.json for replay provenance")?;
        let updated =
            summary.with_replay_provenance(bundle_digest.to_string(), replay_mode, source_run_id);
        assay_core::report::summary::write_summary(&updated, &summary_path)?;
    }

    Ok(())
}

fn annotate_run_json_provenance(
    path: &Path,
    bundle_digest: &str,
    replay_mode: &str,
    source_run_id: Option<&str>,
) -> anyhow::Result<()> {
    let raw = std::fs::read(path)
        .with_context(|| format!("failed to read run json for provenance: {}", path.display()))?;
    let mut root: serde_json::Value = serde_json::from_slice(&raw).with_context(|| {
        format!(
            "failed to parse run json for provenance: {}",
            path.display()
        )
    })?;

    let Some(obj) = root.as_object_mut() else {
        anyhow::bail!("run json must be object for replay provenance");
    };

    let provenance = obj
        .entry("provenance".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let Some(prov_obj) = provenance.as_object_mut() else {
        anyhow::bail!("run json provenance must be object");
    };

    prov_obj.insert("replay".to_string(), serde_json::Value::Bool(true));
    prov_obj.insert(
        "bundle_digest".to_string(),
        serde_json::Value::String(bundle_digest.to_string()),
    );
    prov_obj.insert(
        "replay_mode".to_string(),
        serde_json::Value::String(replay_mode.to_string()),
    );
    match source_run_id {
        Some(id) => {
            prov_obj.insert(
                "source_run_id".to_string(),
                serde_json::Value::String(id.to_string()),
            );
        }
        None => {
            prov_obj.insert("source_run_id".to_string(), serde_json::Value::Null);
        }
    }

    std::fs::write(path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

fn source_run_id_from_bundle(
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

fn write_entries(workspace: &Path, entries: &[(String, Vec<u8>)]) -> anyhow::Result<()> {
    for (rel, data) in entries {
        let target = workspace.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, data)?;
    }
    Ok(())
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher)?;
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

struct ReplayWorkspace {
    path: PathBuf,
}

impl ReplayWorkspace {
    fn new() -> anyhow::Result<Self> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("clock before UNIX_EPOCH")?
            .as_nanos();
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("assay-replay-{}-{}", pid, now));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ReplayWorkspace {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_dir_all(&self.path) {
            if err.kind() != ErrorKind::NotFound {
                eprintln!(
                    "warning: failed to clean replay workspace {}: {}",
                    self.path.display(),
                    err
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit_codes::ExitCodeVersion;
    use assay_core::replay::{ReplayCoverage, ReplayManifest};

    #[test]
    fn offline_dependency_message_present_when_incomplete() {
        let mut manifest = ReplayManifest::minimal("2.15.0".to_string());
        manifest.replay_coverage = Some(ReplayCoverage {
            complete_tests: vec!["a".to_string()],
            incomplete_tests: vec!["b".to_string()],
            reason: Some(std::collections::BTreeMap::from([(
                "b".to_string(),
                "judge cache missing".to_string(),
            )])),
        });

        let msg = offline_dependency_message(&manifest).expect("message expected");
        assert!(msg.contains("incomplete"));
        assert!(msg.contains("b"));
    }

    #[test]
    fn annotate_run_json_provenance_adds_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.json");
        std::fs::write(&path, r#"{"exit_code":0,"reason_code":""}"#).unwrap();

        annotate_run_json_provenance(&path, "sha256:abc", "offline", Some("123")).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();

        assert_eq!(value["provenance"]["replay"], true);
        assert_eq!(value["provenance"]["bundle_digest"], "sha256:abc");
        assert_eq!(value["provenance"]["replay_mode"], "offline");
        assert_eq!(value["provenance"]["source_run_id"], "123");
    }

    #[test]
    fn replay_run_args_overrides_and_inherits_defaults() {
        let args = replay_run_args(
            PathBuf::from("custom/eval.yaml"),
            Some(PathBuf::from("custom/trace.jsonl")),
            PathBuf::from("custom/eval.db"),
            true,
            ExitCodeVersion::V1,
        );

        assert_eq!(args.config, PathBuf::from("custom/eval.yaml"));
        assert_eq!(args.trace_file, Some(PathBuf::from("custom/trace.jsonl")));
        assert_eq!(args.db, PathBuf::from("custom/eval.db"));
        assert_eq!(args.quarantine_mode, "off");
        assert!(args.refresh_cache);
        assert!(args.no_cache);
        assert!(args.judge.no_judge);
        assert!(args.replay_strict);
        assert_eq!(args.exit_codes, ExitCodeVersion::V1);

        // Inherited from RunArgs defaults.
        assert_eq!(args.embedder, "none");
        assert_eq!(args.embedding_model, "text-embedding-3-small");
        assert!(!args.strict);
        assert!(!args.redact_prompts);
        assert!(!args.no_verify);
    }
}
