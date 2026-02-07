use anyhow::Result;
use assay_core::config::{load_config, path_resolver::PathResolver};
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::cli::args::{JudgeArgs, RunArgs, WatchArgs};

const POLL_INTERVAL_MS: u64 = 250;
const MIN_DEBOUNCE_MS: u64 = 50;
const MAX_DEBOUNCE_MS: u64 = 60_000;
const MAX_SNAPSHOT_HASH_BYTES: u64 = 256 * 1024;

pub async fn run(args: WatchArgs, legacy_mode: bool) -> Result<i32> {
    use chrono::Local;

    let mut watch_targets = collect_watch_paths(&args, legacy_mode)?;
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

        coalesce_changed_paths(&mut changed);
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

fn normalize_debounce_ms(value: u64) -> u64 {
    value.clamp(MIN_DEBOUNCE_MS, MAX_DEBOUNCE_MS)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileSnapshot {
    exists: bool,
    len: Option<u64>,
    modified: Option<SystemTime>,
    content_hash: Option<u64>,
}

fn snapshot_paths(paths: &[PathBuf]) -> BTreeMap<PathBuf, FileSnapshot> {
    let mut out = BTreeMap::new();
    for path in paths {
        let snapshot = match std::fs::metadata(path) {
            Ok(meta) => {
                let len = meta.len();
                FileSnapshot {
                    exists: true,
                    len: Some(len),
                    modified: meta.modified().ok(),
                    content_hash: snapshot_content_hash(path, len),
                }
            }
            Err(_) => FileSnapshot {
                exists: false,
                len: None,
                modified: None,
                content_hash: None,
            },
        };
        out.insert(path.clone(), snapshot);
    }
    out
}

fn snapshot_content_hash(path: &PathBuf, len: u64) -> Option<u64> {
    if len > MAX_SNAPSHOT_HASH_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = DefaultHasher::new();
    hasher.write(&bytes);
    Some(hasher.finish())
}

fn diff_paths(
    prev: &BTreeMap<PathBuf, FileSnapshot>,
    curr: &BTreeMap<PathBuf, FileSnapshot>,
) -> Vec<PathBuf> {
    let mut changed = Vec::new();

    let all_paths: BTreeSet<PathBuf> = prev.keys().chain(curr.keys()).cloned().collect();
    for path in all_paths {
        let prev_state = prev.get(&path);
        let curr_state = curr.get(&path);
        if prev_state != curr_state {
            changed.push(path);
        }
    }

    changed
}

fn coalesce_changed_paths(changed: &mut Vec<PathBuf>) {
    changed.sort();
    changed.dedup();
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
                    "warning: failed to parse config while collecting watch paths: {}; keeping core watch targets (config/trace/baseline)",
                    err,
                );
            }
        }
    }

    Ok(paths.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::{
        coalesce_changed_paths, collect_watch_paths, diff_paths, normalize_debounce_ms,
        snapshot_paths, FileSnapshot, MAX_DEBOUNCE_MS, MAX_SNAPSHOT_HASH_BYTES, MIN_DEBOUNCE_MS,
    };
    use crate::cli::args::WatchArgs;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;

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

    #[test]
    fn normalize_debounce_ms_clamps_low_values() {
        assert_eq!(normalize_debounce_ms(0), MIN_DEBOUNCE_MS);
        assert_eq!(normalize_debounce_ms(1), MIN_DEBOUNCE_MS);
    }

    #[test]
    fn normalize_debounce_ms_clamps_high_values() {
        assert_eq!(normalize_debounce_ms(u64::MAX), MAX_DEBOUNCE_MS);
        assert_eq!(normalize_debounce_ms(MAX_DEBOUNCE_MS + 1), MAX_DEBOUNCE_MS);
    }

    #[test]
    fn normalize_debounce_ms_keeps_in_range_values() {
        assert_eq!(normalize_debounce_ms(MIN_DEBOUNCE_MS), MIN_DEBOUNCE_MS);
        assert_eq!(normalize_debounce_ms(350), 350);
        assert_eq!(normalize_debounce_ms(MAX_DEBOUNCE_MS), MAX_DEBOUNCE_MS);
    }

    #[test]
    fn diff_paths_is_order_independent() {
        let mut prev = BTreeMap::new();
        prev.insert(
            PathBuf::from("a.yaml"),
            FileSnapshot {
                exists: true,
                len: Some(10),
                modified: None,
                content_hash: None,
            },
        );
        prev.insert(
            PathBuf::from("b.yaml"),
            FileSnapshot {
                exists: true,
                len: Some(20),
                modified: None,
                content_hash: None,
            },
        );

        let mut curr = BTreeMap::new();
        curr.insert(
            PathBuf::from("b.yaml"),
            FileSnapshot {
                exists: true,
                len: Some(20),
                modified: None,
                content_hash: None,
            },
        );
        curr.insert(
            PathBuf::from("a.yaml"),
            FileSnapshot {
                exists: true,
                len: Some(10),
                modified: None,
                content_hash: None,
            },
        );

        let changed = diff_paths(&prev, &curr);
        assert!(changed.is_empty());
    }

    #[test]
    fn diff_paths_detects_added_removed_and_modified_paths() {
        let mut prev = BTreeMap::new();
        prev.insert(
            PathBuf::from("same.yaml"),
            FileSnapshot {
                exists: true,
                len: Some(1),
                modified: None,
                content_hash: None,
            },
        );
        prev.insert(
            PathBuf::from("removed.yaml"),
            FileSnapshot {
                exists: true,
                len: Some(2),
                modified: None,
                content_hash: None,
            },
        );
        prev.insert(
            PathBuf::from("changed.yaml"),
            FileSnapshot {
                exists: true,
                len: Some(3),
                modified: None,
                content_hash: None,
            },
        );

        let mut curr = BTreeMap::new();
        curr.insert(
            PathBuf::from("same.yaml"),
            FileSnapshot {
                exists: true,
                len: Some(1),
                modified: None,
                content_hash: None,
            },
        );
        curr.insert(
            PathBuf::from("changed.yaml"),
            FileSnapshot {
                exists: true,
                len: Some(99),
                modified: None,
                content_hash: None,
            },
        );
        curr.insert(
            PathBuf::from("added.yaml"),
            FileSnapshot {
                exists: true,
                len: Some(4),
                modified: None,
                content_hash: None,
            },
        );

        let mut changed = diff_paths(&prev, &curr);
        coalesce_changed_paths(&mut changed);
        assert_eq!(
            changed,
            vec![
                PathBuf::from("added.yaml"),
                PathBuf::from("changed.yaml"),
                PathBuf::from("removed.yaml"),
            ]
        );
    }

    #[test]
    fn coalesce_changed_paths_sorts_and_deduplicates() {
        let mut changed = vec![
            PathBuf::from("b.yaml"),
            PathBuf::from("a.yaml"),
            PathBuf::from("a.yaml"),
        ];
        coalesce_changed_paths(&mut changed);
        assert_eq!(
            changed,
            vec![PathBuf::from("a.yaml"), PathBuf::from("b.yaml")]
        );
    }

    #[test]
    fn collect_watch_paths_parse_error_keeps_core_targets() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = tmp.path().join("eval.yaml");
        let trace = tmp.path().join("trace.jsonl");
        let baseline = tmp.path().join("baseline.json");
        let db = tmp.path().join("eval.db");

        fs::write(&config, "version: [\n").expect("write invalid config");

        let args = WatchArgs {
            config: config.clone(),
            trace_file: Some(trace.clone()),
            baseline: Some(baseline.clone()),
            db,
            strict: false,
            replay_strict: false,
            clear: false,
            debounce_ms: 300,
        };

        let paths = collect_watch_paths(&args, false).expect("collect paths");
        assert!(paths.contains(&config));
        assert!(paths.contains(&trace));
        assert!(paths.contains(&baseline));
    }

    #[test]
    fn diff_paths_detects_same_length_change_via_content_hash() {
        let mut prev = BTreeMap::new();
        prev.insert(
            PathBuf::from("coarse.txt"),
            FileSnapshot {
                exists: true,
                len: Some(3),
                modified: None,
                content_hash: Some(1),
            },
        );
        let mut curr = BTreeMap::new();
        curr.insert(
            PathBuf::from("coarse.txt"),
            FileSnapshot {
                exists: true,
                len: Some(3),
                modified: None,
                content_hash: Some(2),
            },
        );

        let changed = diff_paths(&prev, &curr);
        assert_eq!(changed, vec![PathBuf::from("coarse.txt")]);
    }

    #[test]
    fn snapshot_paths_hashes_small_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("small.yaml");
        fs::write(&file, "abc").expect("write file");

        let snapshot = snapshot_paths(std::slice::from_ref(&file));
        let state = snapshot.get(&file).expect("snapshot for file");
        assert!(state.exists);
        assert_eq!(state.len, Some(3));
        assert!(state.content_hash.is_some());
    }

    #[test]
    fn snapshot_paths_skips_hash_for_large_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("large.jsonl");
        let data = vec![b'x'; (MAX_SNAPSHOT_HASH_BYTES as usize) + 1];
        fs::write(&file, data).expect("write file");

        let snapshot = snapshot_paths(std::slice::from_ref(&file));
        let state = snapshot.get(&file).expect("snapshot for file");
        assert!(state.exists);
        assert!(state.content_hash.is_none());
    }
}
