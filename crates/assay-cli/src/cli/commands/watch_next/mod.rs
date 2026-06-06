mod paths;
mod snapshot;

#[cfg(test)]
mod tests;

use anyhow::Result;
use std::io::Write;
use std::time::Duration;

use crate::cli::args::{RunArgs, WatchArgs};

pub(super) const MIN_DEBOUNCE_MS: u64 = 50;
pub(super) const MAX_DEBOUNCE_MS: u64 = 60_000;
pub(super) const MAX_SNAPSHOT_HASH_BYTES: u64 = 256 * 1024;

const POLL_INTERVAL_MS: u64 = 250;

pub async fn run(args: WatchArgs, legacy_mode: bool) -> Result<i32> {
    use chrono::Local;

    let mut watch_targets = paths::collect_watch_paths(&args, legacy_mode)?;
    if watch_targets.is_empty() {
        anyhow::bail!("no watch targets resolved");
    }

    let debounce_ms = normalize_debounce_ms(args.debounce_ms);
    if debounce_ms != args.debounce_ms {
        eprintln!(
            "warning: --debounce-ms {} out of range; using {} (allowed: {}..={})",
            args.debounce_ms, debounce_ms, MIN_DEBOUNCE_MS, MAX_DEBOUNCE_MS
        );
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

    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);
    let debounce = Duration::from_millis(debounce_ms);
    let mut state = snapshot::snapshot_paths(&watch_targets);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                eprintln!("Stopping watch loop.");
                break;
            }
            _ = tokio::time::sleep(poll_interval) => {}
        }

        let current = snapshot::snapshot_paths(&watch_targets);
        let mut changed = snapshot::diff_paths(&state, &current);
        if changed.is_empty() {
            continue;
        }

        // Debounce bursty changes by waiting for stability.
        let mut stable_snapshot = current;
        loop {
            tokio::time::sleep(debounce).await;
            let next_snapshot = snapshot::snapshot_paths(&watch_targets);
            let next_changed = snapshot::diff_paths(&stable_snapshot, &next_snapshot);
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

        snapshot::coalesce_changed_paths(&mut changed);
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

        match paths::refresh_watch_targets(&args, legacy_mode, &mut watch_targets) {
            Ok(true) => {
                state = snapshot::snapshot_paths(&watch_targets);
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

fn normalize_debounce_ms(value: u64) -> u64 {
    value.clamp(MIN_DEBOUNCE_MS, MAX_DEBOUNCE_MS)
}

async fn run_once(args: &WatchArgs, legacy_mode: bool) -> Result<i32> {
    let run_args = run_args_from_watch(args);

    let code = crate::cli::commands::run::run(run_args, legacy_mode).await?;
    eprintln!("Result: exit {}", code);
    Ok(code)
}

fn run_args_from_watch(args: &WatchArgs) -> RunArgs {
    RunArgs {
        config: args.config.clone(),
        db: args.db.clone(),
        trace_file: args.trace_file.clone(),
        baseline: args.baseline.clone(),
        strict: args.strict,
        replay_strict: args.replay_strict,
        ..RunArgs::default()
    }
}
