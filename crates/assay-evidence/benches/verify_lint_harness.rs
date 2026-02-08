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

const ENTROPY_ALPHABET: &[u8; 64] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";

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

    for seq in 0..workload.events {
        let payload_blob = low_compressibility_payload(workload.payload_bytes, seq as u64);
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

fn bench_verify_and_lint(c: &mut Criterion) {
    for workload in selected_workloads() {
        let bundle = build_bundle(workload);
        c.bench_function(&format!("verify+lint/{}", workload.name), |b| {
            b.iter(|| {
                let verified = verify_bundle_with_limits(
                    Cursor::new(black_box(bundle.as_slice())),
                    VerifyLimits::default(),
                )
                .expect("verify must succeed");
                black_box(verified.manifest.event_count);

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

fn low_compressibility_payload(payload_bytes: usize, seed: u64) -> String {
    let mut s = String::with_capacity(payload_bytes);
    let mut x = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(0xD1B5_4A32_D192_ED03);
    while s.len() < payload_bytes {
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        let idx = (x.wrapping_mul(0x2545_F491_4F6C_DD1D) & 63) as usize;
        s.push(ENTROPY_ALPHABET[idx] as char);
    }
    s
}

criterion_group!(
    verify_lint_harness,
    bench_verify,
    bench_lint,
    bench_verify_and_lint
);
criterion_main!(verify_lint_harness);
