use super::super::super::args::ReplayArgs;
use super::failure::{write_missing_dependency, write_replay_failure};
use super::fs_ops::{apply_seed_override, write_entries, ReplayWorkspace};
use super::manifest::{
    offline_dependency_message, resolve_config_path, resolve_trace_path, source_run_id_from_bundle,
};
use super::provenance::annotate_replay_outputs;
use super::run_args::replay_run_args;
use crate::exit_codes::ReasonCode;
use assay_core::replay::bundle::ReplayLimits;
use assay_core::replay::read_verify_bounded;

pub async fn run(args: ReplayArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    let replay_mode = if args.live { "live" } else { "offline" };

    // One bounded snapshot for all three: the digest published as provenance, the bundle that is
    // parsed, and the verdict. Digesting the path and then opening it again read the file twice
    // and left a window in which the published digest could describe different bytes than the
    // ones actually verified and replayed.
    let file = match std::fs::File::open(&args.bundle) {
        Ok(file) => file,
        Err(err) => {
            return write_replay_failure(
                &args,
                "sha256:unknown",
                replay_mode,
                None,
                ReasonCode::ECfgParse,
                format!("failed to open bundle {}: {}", args.bundle.display(), err),
                None,
            );
        }
    };
    let snapshot = match read_verify_bounded(file, ReplayLimits::default()) {
        Ok(snapshot) => snapshot,
        Err(failure) => {
            // Match the typed refusal before rendering. Formatting first and reporting everything
            // as a parse error told an operator to fix the producer when the answer may be to
            // raise a budget, and discarded the digest we already held for the exact bytes that
            // failed.
            let (reason, detail) = match failure.ingest_refusal() {
                Some(refusal) => (
                    ReasonCode::EReplayLimitExceeded,
                    format!("replay bundle refused by an ingest ceiling: {refusal}"),
                ),
                None => (
                    ReasonCode::ECfgParse,
                    format!("failed to read replay bundle: {}", failure.error),
                ),
            };
            let digest = failure
                .source_digest
                .clone()
                .unwrap_or_else(|| "sha256:unknown".to_string());
            return write_replay_failure(&args, &digest, replay_mode, None, reason, detail, None);
        }
    };
    let bundle_digest = snapshot.source_digest.clone();
    let read = snapshot.read;
    let verify = snapshot.verify;
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
