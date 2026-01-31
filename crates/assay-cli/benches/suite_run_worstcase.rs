//! Criterion benchmark: suite run worstcase (runner → store → report, file-backed WAL).
//! Generates enough writes so WAL/checkpointing can occur; for P0.3 regression.
//! Run with: cargo bench -p assay-cli --bench suite_run_worstcase
//! Requires: assay binary (cargo build --release or CARGO_BIN_EXE_assay when run via cargo bench).
//!
//! **Duration:** Each iteration runs a full `assay run` subprocess (12 episodes, file-backed DB).
//! With QUICK=1 (10 samples, 300ms warm-up, 1s measurement): expect ~20–40s total. Not a hang.

use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

fn assay_bin() -> PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_assay") {
        return PathBuf::from(p);
    }
    // Fallback when not invoked by cargo bench: workspace target dir
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let release = manifest.join("../../target/release/assay");
    let debug = manifest.join("../../target/debug/assay");
    if release.exists() {
        release
    } else {
        debug
    }
}

/// Write minimal worstcase eval (12 tests, deterministic-only) and trace (12 episodes × 8 tool_calls, ~400B payload).
fn write_worstcase_fixtures(dir: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
    let eval_path = dir.path().join("eval_worst.yaml");
    let trace_path = dir.path().join("trace_worst.jsonl");
    let db_path = dir.path().join("bench.db");

    let mut eval = String::from(
        "configVersion: 1\nsuite: perf_worst\nmodel: trace\nsettings:\n  cache: false\n  parallel: 4\ntests:\n",
    );
    for i in 1..=12 {
        eval.push_str(&format!(
            "  - id: w{}\n    input: {{ prompt: \"w{}\" }}\n    expected: {{ type: sequence_valid, rules: [{{ type: require, tool: tc_a }}] }}\n",
            i, i
        ));
    }
    fs::write(&eval_path, eval).unwrap();

    let payload = format!("{{\"data\":\"{}\"}}", "x".repeat(380));
    let mut trace = Vec::new();
    for i in 1..=12 {
        let t0 = i * 10_000u64;
        trace.push(format!(
            "{{\"type\":\"episode_start\",\"episode_id\":\"ew{}\",\"timestamp\":{},\"input\":{{\"prompt\":\"w{}\"}}}}",
            i, t0, i
        ));
        trace.push(format!(
            "{{\"type\":\"step\",\"episode_id\":\"ew{}\",\"step_id\":\"s1\",\"idx\":0,\"timestamp\":{},\"kind\":\"llm\",\"content\":\"call\"}}",
            i, t0 + 50
        ));
        for j in 0..8 {
            let ts = t0 + 55 + j * 5;
            trace.push(format!(
                "{{\"type\":\"tool_call\",\"episode_id\":\"ew{}\",\"step_id\":\"s1\",\"timestamp\":{},\"tool_name\":\"tc_a\",\"call_index\":{},\"args\":{},\"result\":{}}}",
                i, ts, j, payload, payload
            ));
        }
        trace.push(format!(
            "{{\"type\":\"episode_end\",\"episode_id\":\"ew{}\",\"timestamp\":{},\"final_output\":\"ok\"}}",
            i, t0 + 100
        ));
    }
    fs::write(&trace_path, trace.join("\n")).unwrap();

    (eval_path, trace_path, db_path)
}

fn bench_suite_run_worstcase(c: &mut Criterion) {
    let bin = assay_bin();
    if !bin.exists() {
        eprintln!(
            "assay binary not found at {:?}; run cargo build (or cargo build --release) first",
            bin
        );
        return;
    }

    // Short group name "sr" (suite_run) so Criterion ID fits on one line for Bencher parsing.
    let mut group = c.benchmark_group("sr");
    if std::env::var("QUICK").is_ok() {
        // Criterion 0.5.x requires sample_size >= 10 (assertion n >= 10).
        // Short timing so CI/local finish in ~20–40s; each iteration = full assay run.
        group
            .sample_size(10)
            .warm_up_time(Duration::from_millis(300))
            .measurement_time(Duration::from_secs(1));
    } else {
        group.sample_size(20);
    }

    // Short ID so Criterion doesn't wrap; Bencher rust_criterion expects "id time: [...]" on one line.
    group.bench_function("wc", |b: &mut Bencher<'_>| {
        b.iter(|| {
            let dir = TempDir::new().unwrap();
            let (eval, trace, db) = write_worstcase_fixtures(&dir);
            let out = Command::new(&bin)
                .args([
                    "run",
                    "--config",
                    eval.as_os_str().to_str().unwrap(),
                    "--trace-file",
                    trace.as_os_str().to_str().unwrap(),
                    "--db",
                    db.as_os_str().to_str().unwrap(),
                ])
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert!(out.status.success(), "assay run failed: {:?}", out);
            black_box(out);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_suite_run_worstcase);
criterion_main!(benches);
