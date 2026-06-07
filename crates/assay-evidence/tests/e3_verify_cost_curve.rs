//! Verification + signing cost curve for signed evidence bundles.
//!
//! This is a COST MEASUREMENT, not a security claim. It records, as a function of bundle size:
//!   - verify time (median),
//!   - compressed bundle bytes and bytes-per-event,
//!   - gzip ratio (compressed / uncompressed events),
//!   - Merkle inclusion-proof size = ceil(log2(N)) hashes,
//!   - DSSE sign / verify time over the run anchor.
//!
//! Rationale: signed/attested artifact lines are widening, but the cost of verification is rarely
//! published as a curve. This fills that measurement gap with a reproducible harness.
//!
//! Honesty: cost numbers are workload- and machine-dependent. The full sweep runs only when
//! `E3_OUT_DIR` is set (and emits a JSON + BMF artifact); CI always runs a fast smoke that keeps
//! the cost path exercised and sound. Tamper-evidence itself is measured separately in the
//! mutation-detection matrix, not here.

use assay_evidence::mandate::types::{
    AuthMethod, Constraints, Context, MandateContent, MandateKind, Principal, Scope, Validity,
};
use assay_evidence::mandate::{sign_mandate, verify_mandate};
use assay_evidence::types::EvidenceEvent;
use assay_evidence::{verify_bundle_with_limits, BundleWriter, VerifyLimits};
use chrono::{TimeZone, Utc};
use ed25519_dalek::SigningKey;
use serde_json::json;
use std::io::Cursor;
use std::time::Instant;

const ENTROPY_ALPHABET: &[u8; 64] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";

/// Deterministic, low-compressibility payload so gzip ratio reflects realistic content.
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

fn build_bundle(events: usize, payload_bytes: usize) -> Vec<u8> {
    let mut bundle = Vec::new();
    let mut writer = BundleWriter::new(&mut bundle);
    for seq in 0..events {
        let blob = low_compressibility_payload(payload_bytes, seq as u64);
        let time = Utc
            .timestamp_opt(1_700_000_000_i64 + seq as i64, 0)
            .unwrap();
        let event = EvidenceEvent::new(
            "assay.tool.decision",
            "urn:assay:e3-cost",
            "run-cost".to_string(),
            seq as u64,
            json!({
                "tool": "fs.read",
                "args": { "path": "/workspace/demo.txt", "blob": blob },
                "decision": "allow",
            }),
        )
        .with_time(time);
        writer.add_event(event);
    }
    writer.finish().expect("bundle generation must succeed");
    bundle
}

/// Cost-sweep verification uses raised limits so large-N bundles are measurable; the default-limit
/// path (incl. the 100k event cap) is exercised by the smoke and by other tests.
fn cost_limits() -> VerifyLimits {
    VerifyLimits {
        max_events: 2_000_000,
        max_events_bytes: 4 * 1024 * 1024 * 1024,
        max_decode_bytes: 8 * 1024 * 1024 * 1024,
        ..Default::default()
    }
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = xs.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        xs[n / 2]
    } else {
        (xs[n / 2 - 1] + xs[n / 2]) / 2.0
    }
}

fn time_verify_ms(bundle: &[u8], limits: &VerifyLimits, reps: usize) -> f64 {
    let mut samples = Vec::with_capacity(reps);
    for _ in 0..reps {
        let start = Instant::now();
        let res =
            verify_bundle_with_limits(Cursor::new(bundle), *limits).expect("verify must succeed");
        std::hint::black_box(res.event_count);
        samples.push(start.elapsed().as_secs_f64() * 1_000.0);
    }
    median(samples)
}

/// ceil(log2(n)) — number of sibling hashes in an inclusion proof for N leaves.
fn inclusion_proof_hashes(n: u64) -> u32 {
    if n <= 1 {
        0
    } else {
        u64::BITS - (n - 1).leading_zeros()
    }
}

/// Ordinary least squares for verify_ms ~ a + b*events.
fn linear_fit(points: &[(f64, f64)]) -> (f64, f64, f64) {
    let n = points.len() as f64;
    if n < 2.0 {
        return (0.0, 0.0, 0.0);
    }
    let sx: f64 = points.iter().map(|p| p.0).sum();
    let sy: f64 = points.iter().map(|p| p.1).sum();
    let sxx: f64 = points.iter().map(|p| p.0 * p.0).sum();
    let sxy: f64 = points.iter().map(|p| p.0 * p.1).sum();
    let denom = n * sxx - sx * sx;
    if denom.abs() < f64::EPSILON {
        return (0.0, sy / n, 0.0);
    }
    let slope = (n * sxy - sx * sy) / denom;
    let intercept = (sy - slope * sx) / n;
    let mean_y = sy / n;
    let ss_tot: f64 = points.iter().map(|p| (p.1 - mean_y).powi(2)).sum();
    let ss_res: f64 = points
        .iter()
        .map(|p| (p.1 - (intercept + slope * p.0)).powi(2))
        .sum();
    let r2 = if ss_tot.abs() < f64::EPSILON {
        1.0
    } else {
        1.0 - ss_res / ss_tot
    };
    (slope, intercept, r2)
}

fn dsse_content() -> MandateContent {
    MandateContent {
        mandate_kind: MandateKind::Intent,
        principal: Principal::new("user-123", AuthMethod::Oidc),
        scope: Scope::new(vec!["search_*".to_string()]),
        validity: Validity::at(Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 0).unwrap()),
        constraints: Constraints::default(),
        context: Context::new("myorg/app", "auth.myorg.com"),
    }
}

/// Median DSSE sign and verify time in ms over the run anchor.
fn dsse_sign_verify_ms(reps: usize) -> (f64, f64) {
    let key = SigningKey::from_bytes(&[7u8; 32]);
    let vk = key.verifying_key();
    let content = dsse_content();

    let mut sign = Vec::with_capacity(reps);
    let mut verify = Vec::with_capacity(reps);
    for _ in 0..reps {
        let s = Instant::now();
        let signed = sign_mandate(&content, &key).expect("sign must succeed");
        sign.push(s.elapsed().as_secs_f64() * 1_000.0);

        let v = Instant::now();
        let res = verify_mandate(&signed, &vk).expect("verify must succeed");
        std::hint::black_box(&res.mandate_id);
        verify.push(v.elapsed().as_secs_f64() * 1_000.0);
    }
    (median(sign), median(verify))
}

#[test]
fn e3_verify_cost_curve() {
    // Smoke: always exercise the cost path on a small bundle so it cannot rot.
    let smoke = build_bundle(256, 128);
    let smoke_ms = time_verify_ms(&smoke, &VerifyLimits::default(), 3);
    assert!(smoke_ms >= 0.0, "verify timing must be non-negative");
    let (sign_ms, verify_ms) = dsse_sign_verify_ms(3);
    assert!(
        sign_ms >= 0.0 && verify_ms >= 0.0,
        "DSSE timing must be non-negative"
    );

    let out_dir = match std::env::var("E3_OUT_DIR") {
        Ok(d) if !d.is_empty() => d,
        _ => return, // CI fast path: smoke only.
    };

    // Full sweep: payload fixed at 256 bytes/event; reps shrink as N grows.
    let payload_bytes = 256usize;
    let plan: &[(usize, usize)] = &[
        (1_000, 11),
        (5_000, 9),
        (10_000, 7),
        (50_000, 5),
        (100_000, 3),
    ];
    let limits = cost_limits();
    let mut rows = Vec::new();
    let mut fit_points = Vec::new();

    for &(events, reps) in plan {
        let bundle = build_bundle(events, payload_bytes);
        let verified = verify_bundle_with_limits(Cursor::new(bundle.as_slice()), limits)
            .expect("verify must succeed");
        let events_bytes = verified
            .manifest
            .files
            .get("events.ndjson")
            .map(|f| f.bytes)
            .unwrap_or(0);
        let verify_ms = time_verify_ms(&bundle, &limits, reps);
        let compressed = bundle.len() as u64;
        let gzip_ratio = if events_bytes > 0 {
            compressed as f64 / events_bytes as f64
        } else {
            0.0
        };
        rows.push(json!({
            "events": events,
            "verify_ms_median": verify_ms,
            "verify_reps": reps,
            "compressed_bytes": compressed,
            "events_bytes": events_bytes,
            "bytes_per_event_compressed": compressed as f64 / events as f64,
            "gzip_ratio": gzip_ratio,
            "inclusion_proof_hashes": inclusion_proof_hashes(events as u64),
        }));
        fit_points.push((events as f64, verify_ms));
    }

    let (slope, intercept, r2) = linear_fit(&fit_points);
    let (sign_full, verify_full) = dsse_sign_verify_ms(101);

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let cost = json!({
        "schema": "assay.experiment.evidence_verify_cost.v0",
        "profile": profile,
        "payload_bytes_per_event": payload_bytes,
        "rows": rows,
        "fit": {
            "slope_ms_per_event": slope,
            "intercept_ms": intercept,
            "ms_per_1k_events": slope * 1_000.0,
            "r2": r2,
        },
        "dsse": {
            "sign_ms_median": sign_full,
            "verify_ms_median": verify_full,
            "reps": 101,
        },
    });

    std::fs::create_dir_all(&out_dir).expect("create out dir");
    std::fs::write(
        format!("{out_dir}/cost.json"),
        serde_json::to_string_pretty(&cost).unwrap(),
    )
    .expect("write cost.json");

    // Bencher Metric Format for trend tracking.
    let mut bmf = serde_json::Map::new();
    for row in cost["rows"].as_array().unwrap() {
        let events = row["events"].as_u64().unwrap();
        bmf.insert(
            format!("evidence_verify_cost/{events}"),
            json!({ "verify_ms_median": { "value": row["verify_ms_median"] } }),
        );
    }
    bmf.insert(
        "evidence_verify_cost/fit".to_string(),
        json!({ "ms_per_1k_events": { "value": slope * 1_000.0 } }),
    );
    bmf.insert(
        "evidence_dsse".to_string(),
        json!({
            "sign_ms_median": { "value": sign_full },
            "verify_ms_median": { "value": verify_full },
        }),
    );
    std::fs::write(
        format!("{out_dir}/cost.bmf.json"),
        serde_json::to_string_pretty(&serde_json::Value::Object(bmf)).unwrap(),
    )
    .expect("write cost.bmf.json");
}
