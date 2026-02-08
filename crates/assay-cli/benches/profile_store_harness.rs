//! Criterion benchmark harness for profile store load/merge paths.
//! Run with:
//!   cargo bench -p assay-cli --bench profile_store_harness
//! Select workloads with:
//!   ASSAY_PROFILE_PERF_WORKLOAD=small|typical-pr|large|small,typical-pr,large

use criterion::{criterion_group, criterion_main, Criterion};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;

#[derive(Clone, Copy)]
struct Workload {
    name: &'static str,
    initial_entries: usize,
    delta_entries: usize,
}

const SMALL: Workload = Workload {
    name: "small",
    initial_entries: 1_000,
    delta_entries: 200,
};

const TYPICAL_PR: Workload = Workload {
    name: "typical-pr",
    initial_entries: 10_000,
    delta_entries: 1_000,
};

const LARGE: Workload = Workload {
    name: "large",
    initial_entries: 50_000,
    delta_entries: 5_000,
};

struct Fixture {
    _temp: TempDir,
    profile_path: PathBuf,
    delta_trace_path: PathBuf,
}

fn selected_workloads() -> Vec<Workload> {
    match std::env::var("ASSAY_PROFILE_PERF_WORKLOAD").ok().as_deref() {
        Some("small") => vec![SMALL],
        Some("typical-pr") => vec![TYPICAL_PR],
        Some("large") => vec![LARGE],
        Some("small,typical-pr,large") => vec![SMALL, TYPICAL_PR, LARGE],
        _ => vec![SMALL, TYPICAL_PR],
    }
}

fn assay_bin() -> PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_assay") {
        return PathBuf::from(p);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let release = manifest.join("../../target/release/assay");
    let debug = manifest.join("../../target/debug/assay");
    if release.exists() {
        release
    } else {
        debug
    }
}

fn run_cmd(bin: &Path, cwd: &Path, args: &[String]) {
    let mut cmd = Command::new(bin);
    cmd.current_dir(cwd).stdin(Stdio::null()).args(args);
    let out = cmd.output().expect("assay command must run");
    assert!(
        out.status.success(),
        "assay command failed: args={:?}\nstdout={}\nstderr={}",
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn build_initial_trace(path: &Path, entries: usize) {
    let mut lines = Vec::with_capacity(entries);
    for i in 0..entries {
        lines.push(event_line(i as u64, i as u64));
    }
    std::fs::write(path, lines.join("\n")).expect("write initial trace");
}

fn build_delta_trace(path: &Path, initial_entries: usize, delta_entries: usize) {
    let mut lines = Vec::with_capacity(delta_entries);
    for i in 0..delta_entries {
        let logical_idx = if i % 2 == 0 {
            i % initial_entries
        } else {
            initial_entries + i
        };
        lines.push(event_line(logical_idx as u64, (initial_entries + i) as u64));
    }
    std::fs::write(path, lines.join("\n")).expect("write delta trace");
}

fn event_line(logical_idx: u64, ts_idx: u64) -> String {
    let ts = 1_700_000_000_u64.saturating_add(ts_idx);
    match logical_idx % 3 {
        0 => format!(
            r#"{{"type":"file_open","path":"/workspace/file_{logical_idx}.txt","timestamp":{ts}}}"#
        ),
        1 => format!(
            r#"{{"type":"net_connect","dest":"https://svc-{logical_idx}.example.com:443","timestamp":{ts}}}"#
        ),
        _ => format!(r#"{{"type":"proc_exec","path":"/bin/tool_{logical_idx}","timestamp":{ts}}}"#),
    }
}

fn prepare_fixture(bin: &Path, workload: Workload) -> Fixture {
    let temp = TempDir::new().expect("temp dir");
    let profile_path = temp.path().join("profile.yaml");
    let initial_trace_path = temp.path().join("initial.jsonl");
    let delta_trace_path = temp.path().join("delta.jsonl");

    build_initial_trace(&initial_trace_path, workload.initial_entries);
    build_delta_trace(
        &delta_trace_path,
        workload.initial_entries,
        workload.delta_entries,
    );

    run_cmd(
        bin,
        temp.path(),
        &[
            "profile".to_string(),
            "init".to_string(),
            "--output".to_string(),
            profile_path.display().to_string(),
        ],
    );
    run_cmd(
        bin,
        temp.path(),
        &[
            "profile".to_string(),
            "update".to_string(),
            "--profile".to_string(),
            profile_path.display().to_string(),
            "--input".to_string(),
            initial_trace_path.display().to_string(),
            "--run-id".to_string(),
            "bootstrap-run".to_string(),
        ],
    );

    Fixture {
        _temp: temp,
        profile_path,
        delta_trace_path,
    }
}

fn bench_profile_load(c: &mut Criterion) {
    let bin = assay_bin();
    if !bin.exists() {
        eprintln!("assay binary not found at {:?}; run cargo build first", bin);
        return;
    }

    for workload in selected_workloads() {
        let fixture = prepare_fixture(&bin, workload);
        c.bench_function(&format!("profile/load/{}", workload.name), |b| {
            b.iter(|| {
                run_cmd(
                    &bin,
                    fixture._temp.path(),
                    &[
                        "profile".to_string(),
                        "show".to_string(),
                        "--profile".to_string(),
                        fixture.profile_path.display().to_string(),
                        "--format".to_string(),
                        "summary".to_string(),
                    ],
                );
            });
        });
    }
}

fn bench_profile_merge(c: &mut Criterion) {
    let bin = assay_bin();
    if !bin.exists() {
        eprintln!("assay binary not found at {:?}; run cargo build first", bin);
        return;
    }

    for workload in selected_workloads() {
        let fixture = prepare_fixture(&bin, workload);
        let run_seq = AtomicU64::new(1);
        c.bench_function(&format!("profile/merge/{}", workload.name), |b| {
            b.iter(|| {
                let run_id = format!("bench-run-{}", run_seq.fetch_add(1, Ordering::Relaxed));
                run_cmd(
                    &bin,
                    fixture._temp.path(),
                    &[
                        "profile".to_string(),
                        "update".to_string(),
                        "--profile".to_string(),
                        fixture.profile_path.display().to_string(),
                        "--input".to_string(),
                        fixture.delta_trace_path.display().to_string(),
                        "--run-id".to_string(),
                        run_id,
                    ],
                );
            });
        });
    }
}

criterion_group!(
    profile_store_harness,
    bench_profile_load,
    bench_profile_merge
);
criterion_main!(profile_store_harness);
