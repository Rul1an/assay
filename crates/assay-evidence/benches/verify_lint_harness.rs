use assay_evidence::bundle::writer::{verify_bundle_with_limits, BundleWriter, VerifyLimits};
use assay_evidence::lint::engine::lint_bundle;
use assay_evidence::types::EvidenceEvent;
use chrono::{TimeZone, Utc};
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use std::hint::black_box;
use std::io::Cursor;

#[derive(Clone, Copy)]
struct Workload {
    name: &'static str,
    events: usize,
    payload_bytes: usize,
}

const SMALL: Workload = Workload {
    name: "small",
    events: 1_000,
    payload_bytes: 128,
};

const TYPICAL_PR: Workload = Workload {
    name: "typical-pr",
    events: 10_000,
    payload_bytes: 256,
};

const LARGE: Workload = Workload {
    name: "large",
    events: 100_000,
    payload_bytes: 512,
};

fn selected_workloads() -> Vec<Workload> {
    match std::env::var("ASSAY_PERF_WORKLOAD").ok().as_deref() {
        Some("small") => vec![SMALL],
        Some("typical-pr") => vec![TYPICAL_PR],
        Some("large") => vec![LARGE],
        Some("small,typical-pr,large") => vec![SMALL, TYPICAL_PR, LARGE],
        _ => vec![SMALL, TYPICAL_PR],
    }
}

fn build_bundle(workload: Workload) -> Vec<u8> {
    let mut bundle = Vec::new();
    let mut writer = BundleWriter::new(&mut bundle);
    let payload_blob = "x".repeat(workload.payload_bytes);

    for seq in 0..workload.events {
        let time = Utc
            .timestamp_opt(1_700_000_000_i64 + seq as i64, 0)
            .unwrap();
        let event = EvidenceEvent::new(
            "assay.tool.decision",
            "urn:assay:perf-harness",
            format!("run-{}", workload.name),
            seq as u64,
            json!({
                "tool": "fs.read",
                "args": { "path": "/workspace/demo.txt", "blob": payload_blob },
                "decision": "allow",
            }),
        )
        .with_time(time);
        writer.add_event(event);
    }

    writer.finish().expect("bundle generation must succeed");
    bundle
}

fn bench_verify(c: &mut Criterion) {
    for workload in selected_workloads() {
        let bundle = build_bundle(workload);
        c.bench_function(&format!("verify/{}", workload.name), |b| {
            b.iter(|| {
                let result = verify_bundle_with_limits(
                    Cursor::new(black_box(bundle.as_slice())),
                    VerifyLimits::default(),
                )
                .expect("verify must succeed");
                black_box(result.manifest.event_count);
            });
        });
    }
}

fn bench_lint(c: &mut Criterion) {
    for workload in selected_workloads() {
        let bundle = build_bundle(workload);
        c.bench_function(&format!("lint/{}", workload.name), |b| {
            b.iter(|| {
                let report = lint_bundle(
                    Cursor::new(black_box(bundle.as_slice())),
                    VerifyLimits::default(),
                )
                .expect("lint must succeed");
                black_box(report.summary.total);
            });
        });
    }
}

criterion_group!(verify_lint_harness, bench_verify, bench_lint);
criterion_main!(verify_lint_harness);
