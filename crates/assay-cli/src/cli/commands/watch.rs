use anyhow::Result;
use assay_core::config::{load_config, path_resolver::PathResolver};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::cli::args::{JudgeArgs, RunArgs, WatchArgs};

pub async fn run(args: WatchArgs, legacy_mode: bool) -> Result<i32> {
    use chrono::Local;

    let mut watch_targets = collect_watch_paths(&args, legacy_mode)?;
    if watch_targets.is_empty() {
        anyhow::bail!("no watch targets resolved");
    }

    eprintln!("Watching paths:");
    for path in &watch_targets {
        eprintln!("  - {}", path.display());
    }
    eprintln!("Press Ctrl+C to stop.\n");

    let initial_time = Local::now().format("%H:%M:%S");
    eprintln!("[{}] Running... (initial)", initial_time);
    if let Err(e) = run_once(&args, legacy_mode).await {
        eprintln!("watch run failed: {}", e);
    }
    eprintln!("---");
    eprintln!(
        "[{}] Waiting for changes...",
        Local::now().format("%H:%M:%S")
    );

    let poll_interval = Duration::from_millis(250);
    let debounce = Duration::from_millis(args.debounce_ms);
    let mut state = snapshot_paths(&watch_targets);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                eprintln!("Stopping watch loop.");
                break;
            }
            _ = tokio::time::sleep(poll_interval) => {}
        }

        let current = snapshot_paths(&watch_targets);
        let mut changed = diff_paths(&state, &current);
        if changed.is_empty() {
            continue;
        }

        // Debounce bursty changes by waiting for stability.
        let mut stable_snapshot = current;
        loop {
            tokio::time::sleep(debounce).await;
            let next_snapshot = snapshot_paths(&watch_targets);
            let next_changed = diff_paths(&stable_snapshot, &next_snapshot);
            if next_changed.is_empty() {
                state = next_snapshot;
                break;
            }
            changed.extend(next_changed);
            stable_snapshot = next_snapshot;
        }

        if args.clear {
            print!("\x1B[2J\x1B[H");
            let _ = std::io::stdout().flush();
        }

        changed.sort();
        changed.dedup();
        let trigger = changed
            .first()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "filesystem change".to_string());

        let timestamp = Local::now().format("%H:%M:%S");
        eprintln!(
            "[{}] Running... (triggered by {}{}{})",
            timestamp,
            trigger,
            if changed.len() > 1 { ", " } else { "" },
            if changed.len() > 1 {
                format!("{} paths", changed.len())
            } else {
                String::new()
            }
        );

        if let Err(e) = run_once(&args, legacy_mode).await {
            eprintln!("watch run failed: {}", e);
        }

        match refresh_watch_targets(&args, legacy_mode, &mut watch_targets) {
            Ok(true) => {
                state = snapshot_paths(&watch_targets);
            }
            Ok(false) => {}
            Err(err) => {
                eprintln!("warning: failed to refresh watch targets: {}", err);
            }
        }

        eprintln!("---");
        eprintln!(
            "[{}] Waiting for changes...",
            Local::now().format("%H:%M:%S")
        );
    }

    Ok(0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileSnapshot {
    exists: bool,
    len: Option<u64>,
    modified: Option<SystemTime>,
}

fn snapshot_paths(paths: &[PathBuf]) -> Vec<(PathBuf, FileSnapshot)> {
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let snapshot = match std::fs::metadata(path) {
            Ok(meta) => FileSnapshot {
                exists: true,
                len: Some(meta.len()),
                modified: meta.modified().ok(),
            },
            Err(_) => FileSnapshot {
                exists: false,
                len: None,
                modified: None,
            },
        };
        out.push((path.clone(), snapshot));
    }
    out
}

fn diff_paths(prev: &[(PathBuf, FileSnapshot)], curr: &[(PathBuf, FileSnapshot)]) -> Vec<PathBuf> {
    let mut changed = Vec::new();

    for ((prev_path, prev_state), (curr_path, curr_state)) in prev.iter().zip(curr.iter()) {
        if prev_path != curr_path {
            changed.push(curr_path.clone());
            continue;
        }
        if prev_state != curr_state {
            changed.push(curr_path.clone());
        }
    }

    changed
}

fn refresh_watch_targets(
    args: &WatchArgs,
    legacy_mode: bool,
    watch_targets: &mut Vec<PathBuf>,
) -> Result<bool> {
    let next = collect_watch_paths(args, legacy_mode)?;
    if *watch_targets == next {
        return Ok(false);
    }

    let previous: BTreeSet<PathBuf> = watch_targets.iter().cloned().collect();
    let current: BTreeSet<PathBuf> = next.iter().cloned().collect();
    eprintln!("Updated watch paths:");
    for path in current.difference(&previous) {
        eprintln!("  + {}", path.display());
    }
    for path in previous.difference(&current) {
        eprintln!("  - {}", path.display());
    }

    *watch_targets = next;
    Ok(true)
}

async fn run_once(args: &WatchArgs, legacy_mode: bool) -> Result<i32> {
    let run_args = RunArgs {
        config: args.config.clone(),
        db: args.db.clone(),
        rerun_failures: 0,
        quarantine_mode: "warn".to_string(),
        trace_file: args.trace_file.clone(),
        redact_prompts: false,
        baseline: args.baseline.clone(),
        export_baseline: None,
        strict: args.strict,
        embedder: "none".to_string(),
        embedding_model: "text-embedding-3-small".to_string(),
        refresh_embeddings: false,
        incremental: false,
        refresh_cache: false,
        no_cache: false,
        explain_skip: false,
        judge: JudgeArgs {
            judge: "none".to_string(),
            no_judge: false,
            judge_model: None,
            judge_samples: 3,
            judge_refresh: false,
            judge_temperature: 0.0,
            judge_max_tokens: 800,
            judge_api_key: None,
        },
        replay_strict: args.replay_strict,
        deny_deprecations: false,
        exit_codes: crate::exit_codes::ExitCodeVersion::default(),
        no_verify: false,
    };

    let code = super::run::run(run_args, legacy_mode).await?;
    eprintln!("Result: exit {}", code);
    Ok(code)
}

pub(crate) fn collect_watch_paths(args: &WatchArgs, legacy_mode: bool) -> Result<Vec<PathBuf>> {
    let mut paths = BTreeSet::new();

    paths.insert(args.config.clone());
    if let Some(trace) = &args.trace_file {
        paths.insert(trace.clone());
    }
    if let Some(baseline) = &args.baseline {
        paths.insert(baseline.clone());
    }

    if args.config.exists() {
        match load_config(&args.config, legacy_mode, false) {
            Ok(cfg) => {
                let resolver = PathResolver::new(&args.config);
                for test in &cfg.tests {
                    if let Some(policy_path) = test.expected.get_policy_path() {
                        let mut resolved = policy_path.to_string();
                        resolver.resolve_str(&mut resolved);
                        paths.insert(PathBuf::from(resolved));
                    }
                }
            }
            Err(err) => {
                eprintln!(
                    "warning: failed to parse config while collecting watch paths: {}",
                    err
                );
            }
        }
    }

    Ok(paths.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::collect_watch_paths;
    use crate::cli::args::WatchArgs;
    use std::fs;
    use std::path::Path;

    #[test]
    fn collect_watch_paths_includes_policy() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = tmp.path().join("eval.yaml");
        let trace = tmp.path().join("trace.jsonl");
        let db = tmp.path().join("eval.db");
        let policy_dir = tmp.path().join("policies");
        let policy = policy_dir.join("default.yaml");

        fs::create_dir_all(&policy_dir).expect("create policy dir");
        fs::write(
            &policy,
            "version: \"2.0\"\nname: \"test\"\ntools:\n  allow:\n    - read_file\n",
        )
        .expect("write policy");

        fs::write(
            &config,
            "version: 1\nsuite: watch\nmodel: trace\ntests:\n  - id: t1\n    input:\n      prompt: \"p\"\n    expected:\n      type: args_valid\n      policy: policies/default.yaml\n",
        )
        .expect("write config");

        let args = WatchArgs {
            config: config.clone(),
            trace_file: Some(trace.clone()),
            baseline: None,
            db,
            strict: false,
            replay_strict: false,
            clear: false,
            debounce_ms: 300,
        };

        let paths = collect_watch_paths(&args, false).expect("collect paths");

        assert!(paths.contains(&config));
        assert!(paths.contains(&trace));
        assert!(paths
            .iter()
            .any(|p| p.ends_with(Path::new("policies/default.yaml"))));
    }
}
