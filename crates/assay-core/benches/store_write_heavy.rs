//! Criterion benchmark: store write-heavy (insert/txn/batching).
//! For P0.3 regression and median/p95; run with: cargo bench -p assay-core --bench store_write_heavy

use assay_core::model::{AttemptRow, EvalConfig, LlmResponse, TestResultRow, TestStatus};
use assay_core::storage::Store;
use criterion::{black_box, criterion_group, criterion_main, Bencher, Criterion};
use std::time::Duration;
use tempfile::NamedTempFile;

fn make_store() -> (Store, NamedTempFile) {
    let f = NamedTempFile::new().unwrap();
    let path = f.path();
    let store = Store::open(path).unwrap();
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

fn bench_store_write_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("store_write_heavy");
    if std::env::var("QUICK").is_ok() {
        group
            .sample_size(10)
            .measurement_time(Duration::from_secs(2));
    } else {
        group.sample_size(20);
    }

    // Many result rows per run (insert/txn stress)
    group.bench_function(
        "create_run_plus_50_results_400b_payload",
        |b: &mut Bencher<'_>| {
            b.iter(|| {
                let (store, _f) = make_store();
                let cfg = minimal_config("bench_50");
                let run_id = store.create_run(&cfg).unwrap();
                let attempts = minimal_attempts();
                let output = minimal_llm_response(400);
                for i in 0..50 {
                    let row = minimal_result_row(&format!("t{}", i), 400);
                    store
                        .insert_result_embedded(run_id, &row, &attempts, &output)
                        .unwrap();
                }
                store.finalize_run(run_id, "completed").unwrap();
                black_box(run_id);
            });
        },
    );

    group.bench_function(
        "create_run_plus_12_results_large_payload",
        |b: &mut Bencher<'_>| {
            b.iter(|| {
                let (store, _f) = make_store();
                let cfg = minimal_config("bench_12");
                let run_id = store.create_run(&cfg).unwrap();
                let attempts = minimal_attempts();
                let output = minimal_llm_response(2000);
                for i in 0..12 {
                    let row = minimal_result_row(&format!("w{}", i), 800);
                    store
                        .insert_result_embedded(run_id, &row, &attempts, &output)
                        .unwrap();
                }
                store.finalize_run(run_id, "completed").unwrap();
                black_box(run_id);
            });
        },
    );

    group.finish();
}

criterion_group!(benches, bench_store_write_heavy);
criterion_main!(benches);
