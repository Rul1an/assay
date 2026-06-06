use super::paths::collect_watch_paths;
use super::snapshot::{coalesce_changed_paths, diff_paths, snapshot_paths, FileSnapshot};
use super::{
    normalize_debounce_ms, run_args_from_watch, MAX_DEBOUNCE_MS, MAX_SNAPSHOT_HASH_BYTES,
    MIN_DEBOUNCE_MS,
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
fn run_args_from_watch_uses_run_defaults() {
    let args = WatchArgs {
        config: PathBuf::from("eval.yaml"),
        trace_file: Some(PathBuf::from("traces/dev.jsonl")),
        baseline: Some(PathBuf::from(".assay/baseline.json")),
        db: PathBuf::from(".eval/eval.db"),
        strict: true,
        replay_strict: true,
        clear: false,
        debounce_ms: 350,
    };

    let run_args = run_args_from_watch(&args);
    assert_eq!(run_args.config, args.config);
    assert_eq!(run_args.db, args.db);
    assert_eq!(run_args.trace_file, args.trace_file);
    assert_eq!(run_args.baseline, args.baseline);
    assert!(run_args.strict);
    assert!(run_args.replay_strict);

    // Defaults stay inherited from RunArgs::default().
    assert_eq!(run_args.quarantine_mode, "warn");
    assert_eq!(run_args.embedder, "none");
    assert_eq!(run_args.embedding_model, "text-embedding-3-small");
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
