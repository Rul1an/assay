//! Criterion benchmark: store write-heavy (insert/txn/batching).
//! For P0.3 regression and median/p95; run with: cargo bench -p assay-core --bench store_write_heavy
//!
//! # Two groups, because one number was answering two questions
//!
//! `Store::open` sets only `PRAGMA foreign_keys=ON`, so the shipped store runs in SQLite's
//! default `journal_mode=delete` + `synchronous=FULL`. `insert_result_embedded` issues two
//! implicit transactions per row, so one `50x400b` iteration commits ~110 times, and each
//! rollback-journal commit creates *and unlinks* a journal file. That makes the workload's
//! wall time overwhelmingly a function of the host's write-barrier and directory-metadata
//! latency rather than of anything in this repository.
//!
//! Measured on this workload (methodology and provenance in docs/PERFORMANCE-ASSESSMENT.md,
//! "Why `sw/*` is not a gate"): the file-backed store spends **98.2%** of wall time in
//! filesystem work. The quantity a Bencher threshold actually sees is run-to-run spread,
//! and on one idle machine that is **1.31x** for `sw/*` against **1.03x** once the journal
//! moves to memory. A percentage-of-mean model over the shipped number cannot separate a
//! code regression from an unlucky runner: PR #2119 changed a crate `assay-core` does not
//! depend on and alerted at +1,782%.
//!
//! So the workload is measured twice, under IDs that say which question is being asked:
//!
//! * `sw/*` — shipped durability (`delete` + `FULL`). What a real run costs on this host.
//!   Host-dominated by construction; belongs on a variance-robust trend, not a gate.
//!   Shapes unchanged, because these IDs carry an existing Bencher series.
//! * `swc/*` — same code path with the journal in memory, so no commit waits on the device
//!   and none creates or unlinks a journal file. What our code costs: serialization,
//!   statement preparation, SQL round-trips, btree work. This is the group a pull request
//!   can actually regress, and the only one compared per-PR.
//!
//! `swc/*` is not a claim about how the store ships. It is the shipped code path with one
//! host-owned cost held constant, so the remaining variance is ours.

use assay_core::model::{AttemptRow, EvalConfig, LlmResponse, TestResultRow, TestStatus};
use assay_core::storage::Store;
use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use std::hint::black_box;
use std::time::Duration;
use tempfile::NamedTempFile;

/// Which durability configuration the store under measurement runs with.
#[derive(Clone, Copy)]
enum Durability {
    /// Exactly what `Store::open` gives a user today.
    Shipped,
    /// Shipped code path with the per-commit write barrier removed.
    NoWriteBarrier,
}

impl Durability {
    /// Expected `(journal_mode, synchronous)` after configuration.
    ///
    /// Asserted rather than assumed: if a pragma silently fails to apply, `swc/*` quietly
    /// becomes a second copy of `sw/*` and starts measuring the disk again while still
    /// being read as a code-cost signal. A benchmark that fails open into noise is worse
    /// than one that fails loudly.
    fn expected_pragmas(self) -> (&'static str, i64) {
        match self {
            Durability::Shipped => ("delete", 2),
            Durability::NoWriteBarrier => ("memory", 0),
        }
    }
}

fn make_store(durability: Durability) -> (Store, NamedTempFile) {
    let f = NamedTempFile::new().unwrap();
    let store = Store::open(f.path()).unwrap();

    {
        let conn = store.conn.lock().unwrap();
        if matches!(durability, Durability::NoWriteBarrier) {
            // `journal_mode=MEMORY` rather than WAL: this benchmark opens a fresh database
            // per iteration, and WAL's `-wal`/`-shm` sidecars are not tracked by
            // `NamedTempFile`, so they accumulate (37k pairs and 12 GB in one local sweep)
            // until SQLite fails with an xShmMap I/O error. MEMORY holds the rollback
            // journal in RAM, which removes both the fsync and the per-commit journal
            // create/unlink — the directory-metadata work that varies most across hosts —
            // and creates no files of its own.
            //
            // `journal_mode` returns a row, so it is queried rather than executed.
            let _: String = conn
                .query_row("PRAGMA journal_mode=MEMORY", [], |r| r.get(0))
                .expect("set journal_mode=MEMORY");
            conn.execute_batch("PRAGMA synchronous=OFF")
                .expect("set synchronous=OFF");
        }

        let (want_journal, want_sync) = durability.expected_pragmas();
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        let synchronous: i64 = conn
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            (journal_mode.as_str(), synchronous),
            (want_journal, want_sync),
            "store opened with unexpected durability pragmas; this benchmark's ID would \
             misdescribe what it measures",
        );
    }

    store.init_schema().unwrap();
    (store, f)
}

fn minimal_config(suite: &str) -> EvalConfig {
    EvalConfig {
        version: 1,
        suite: suite.to_string(),
        model: "trace".to_string(),
        settings: Default::default(),
        thresholds: Default::default(),
        otel: Default::default(),
        tests: vec![],
    }
}

fn minimal_result_row(test_id: &str, payload_size: usize) -> TestResultRow {
    let payload = "x".repeat(payload_size);
    TestResultRow {
        test_id: test_id.to_string(),
        status: TestStatus::Pass,
        score: Some(1.0),
        cached: false,
        message: payload.clone(),
        details: serde_json::json!({}),
        duration_ms: Some(10),
        fingerprint: Some(format!("fp_{}", test_id)),
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }
}

fn minimal_attempts() -> Vec<AttemptRow> {
    vec![AttemptRow {
        attempt_no: 1,
        status: TestStatus::Pass,
        message: "ok".to_string(),
        duration_ms: Some(5),
        details: serde_json::json!({}),
    }]
}

fn minimal_llm_response(payload_size: usize) -> LlmResponse {
    LlmResponse {
        text: "x".repeat(payload_size),
        provider: "bench".to_string(),
        model: "bench".to_string(),
        cached: false,
        meta: serde_json::json!({}),
    }
}

/// Many result rows per run (insert/txn stress).
fn workload_rows(durability: Durability, rows: usize, payload: usize) {
    let (store, _f) = make_store(durability);
    let cfg = minimal_config("bench_50");
    let run_id = store.create_run(&cfg).unwrap();
    let attempts = minimal_attempts();
    let output = minimal_llm_response(payload);
    for i in 0..rows {
        let row = minimal_result_row(&format!("t{}", i), payload);
        store
            .insert_result_embedded(run_id, &row, &attempts, &output)
            .unwrap();
    }
    store.finalize_run(run_id, "completed").unwrap();
    black_box(run_id);
}

/// Fewer rows, larger payloads (serialization stress).
fn workload_large(durability: Durability, rows: usize) {
    let (store, _f) = make_store(durability);
    let cfg = minimal_config("bench_12");
    let run_id = store.create_run(&cfg).unwrap();
    let attempts = minimal_attempts();
    let output = minimal_llm_response(2000);
    for i in 0..rows {
        let row = minimal_result_row(&format!("w{}", i), 800);
        store
            .insert_result_embedded(run_id, &row, &attempts, &output)
            .unwrap();
    }
    store.finalize_run(run_id, "completed").unwrap();
    black_box(run_id);
}

/// Short group names ("sw" = store_write, "swc" = store_write code-cost) so the Criterion ID
/// fits on one line; the Bencher `rust_criterion` adapter expects `id time: [...]` unwrapped.
fn bench_group(c: &mut Criterion, group_name: &str, durability: Durability) {
    let mut group = c.benchmark_group(group_name);
    if std::env::var("QUICK").is_ok() {
        group
            .sample_size(10)
            .measurement_time(Duration::from_secs(2));
    } else {
        group.sample_size(20);
    }

    match durability {
        // Unchanged shapes: these IDs carry an existing Bencher series.
        Durability::Shipped => {
            group.bench_function("50x400b", |b: &mut Bencher<'_>| {
                b.iter(|| workload_rows(durability, 50, 400));
            });
            group.bench_function("12xlarge", |b: &mut Bencher<'_>| {
                b.iter(|| workload_large(durability, 12));
            });
        }
        // Scaled 10x. With the journal in memory the same shapes land at 2.5 ms and
        // 1.4 ms, close enough to a scheduler quantum that a contended runner would be
        // timing its own scheduling. Locally the run-to-run spread plateaus at 1.03x by
        // ~500 rows, so 10x buys headroom on a noisier host at negligible CI cost.
        Durability::NoWriteBarrier => {
            group.bench_function("500x400b", |b: &mut Bencher<'_>| {
                b.iter(|| workload_rows(durability, 500, 400));
            });
            group.bench_function("120xlarge", |b: &mut Bencher<'_>| {
                b.iter(|| workload_large(durability, 120));
            });
        }
    }

    group.finish();
}

fn bench_store_write_heavy(c: &mut Criterion) {
    bench_group(c, "swc", Durability::NoWriteBarrier);
    bench_group(c, "sw", Durability::Shipped);
}

criterion_group!(benches, bench_store_write_heavy);
criterion_main!(benches);
