use super::super::super::args::ReplayArgs;
use super::failure::{write_missing_dependency, write_replay_failure};
use super::fs_ops::{apply_seed_override, sha256_file, write_entries, ReplayWorkspace};
use super::manifest::{
    offline_dependency_message, resolve_config_path, resolve_trace_path, source_run_id_from_bundle,
};
use super::provenance::annotate_replay_outputs;
use super::run_args::replay_run_args;
use crate::exit_codes::ReasonCode;
use assay_core::replay::{read_bundle_tar_gz, verify_bundle};

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

    let exit_code = match super::super::run::run(run_args, legacy_mode).await {
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
