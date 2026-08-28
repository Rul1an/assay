//! #2511: project one existing enforcement-health document.
//!
//! `assay project-enforcement-health --format json --input PATH`
//!
//! Success bytes are exact. Fail-closed paths are nonzero with empty stdout.
//! Valid `active` fixtures are constructor-legal (v0 attach+strong; committed
//! v1 `active_no_probe`). Each near-miss violates exactly one producer arm.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use assay_evidence::{BundleReader, BundleWriter, EvidenceEvent, VerifyLimits};
use assert_cmd::Command;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::json;
use tar::{Archive, Builder, Header};

const SCHEMA: &str = "assay.enforcement_health_projection.v0";
const V0: &str = "assay.enforcement_health.v0";
const V1: &str = "assay.enforcement_health.v1";
/// Same cap the CLI must apply before parse. A syntactically valid document
/// larger than this must fail closed.
const MAX_INPUT_BYTES: usize = 65_536;

fn assay() -> Command {
    Command::cargo_bin("assay").expect("binary")
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).expect("write fixture");
    path
}

fn v0(status: &str) -> String {
    let (attach, class) = if status == "active" {
        ("true", "strong")
    } else {
        ("false", "basic")
    };
    format!(
        r#"{{"schema":"{V0}","scope":"ipv4_tcp_connect","network_enforcement":"{status}","attach_confirmed":{attach},"blocked_count":0,"allowed_count":0,"enforcement_class":"{class}"}}"#
    )
}

fn v0_active_near(attach: bool, class: &str) -> String {
    format!(
        r#"{{"schema":"{V0}","scope":"ipv4_tcp_connect","network_enforcement":"active","attach_confirmed":{attach},"blocked_count":0,"allowed_count":0,"enforcement_class":"{class}"}}"#
    )
}

fn v1(status: &str) -> String {
    let (nnp, restrict, class) = if status == "active" {
        ("true", "true", "strong")
    } else {
        ("false", "false", "basic")
    };
    format!(
        r#"{{"schema":"{V1}","status":"{status}","mechanism":"landlock","scope":"tcp_connect_landlock_port","policy_semantics":"allowlist","enforcement_class":"{class}","landlock":{{"abi":4,"no_new_privs_confirmed":{nnp},"restrict_self_confirmed":{restrict}}},"probe":null,"non_claims":[]}}"#
    )
}

/// Producer-legal v1 `active` except the named arm. Remaining fields match
/// the committed `active_no_probe` shape plus an optional typed `failure`.
fn v1_active_near(class: &str, nnp: bool, restrict: bool, with_failure: bool) -> String {
    let failure = if with_failure {
        r#","failure":{"reason_code":"landlock_abi_too_old","detail":"Landlock ABI 4 is required for TCP connect port allowlists"}"#
    } else {
        ""
    };
    format!(
        r#"{{"schema":"{V1}","status":"active","mechanism":"landlock","scope":"tcp_connect_landlock_port","policy_semantics":"allowlist","enforcement_class":"{class}"{failure},"landlock":{{"abi":4,"handled_access_net":["connect_tcp"],"allowed_connect_tcp_ports":[443],"no_new_privs_confirmed":{nnp},"restrict_self_confirmed":{restrict}}},"probe":null,"non_claims":["no ip or cidr enforcement","no hostname enforcement","no destination identity enforcement","no udp or quic enforcement","no http or tls route policy","not a replacement for cgroup/connect4 endpoint enforcement"]}}"#
    )
}

fn committed_v1_active_no_probe() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/enforcement_health/v1/active_no_probe.json")
}

fn expected(source_schema: &str, observation: &str) -> String {
    format!(
        r#"{{"schema":"{SCHEMA}","lossy":true,"source_schema":"{source_schema}","observation":"{observation}"}}"#
    )
}

fn project(path: &Path) -> assert_cmd::assert::Assert {
    assay()
        .args([
            "project-enforcement-health",
            "--format",
            "json",
            "--input",
            path.to_str().expect("utf8"),
        ])
        .assert()
}

fn write_bundle(dir: &Path, name: &str, events: Vec<EvidenceEvent>) -> PathBuf {
    let path = dir.join(name);
    let file = fs::File::create(&path).expect("create bundle");
    let mut writer = BundleWriter::new(file);
    for event in events {
        writer.add_event(event);
    }
    writer.finish().expect("finish bundle");
    path
}

fn bundle_event(type_: &str, seq: u64, payload: serde_json::Value) -> EvidenceEvent {
    EvidenceEvent::new(type_, "urn:assay:test:sandbox", "sandbox-run", seq, payload)
}

fn tamper_events_member_without_updating_manifest(path: &Path) {
    let file = fs::File::open(path).unwrap();
    let mut archive = Archive::new(GzDecoder::new(file));
    let mut members = Vec::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let member_path = entry.path().unwrap().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if member_path == Path::new("events.ndjson") {
            let before = b"Landlock unavailable";
            let after = b"Landlock unavailablE";
            let offset = bytes
                .windows(before.len())
                .position(|window| window == before)
                .expect("fixture detail in events member");
            bytes[offset..offset + before.len()].copy_from_slice(after);
        }
        members.push((member_path, bytes));
    }

    let file = fs::File::create(path).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);
    for (member_path, bytes) in members {
        let mut header = Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(bytes.len() as u64);
        header.set_cksum();
        builder
            .append_data(&mut header, member_path, bytes.as_slice())
            .unwrap();
    }
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap();
}

fn degradation_payload() -> serde_json::Value {
    json!({
        "reason_code": "backend_unavailable",
        "degradation_mode": "audit_fallback",
        "component": "landlock",
        "detail": "Landlock unavailable"
    })
}

fn deterministic_noise(seed: u64, len: usize) -> String {
    let mut state = seed;
    let alphabet = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    (0..len)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            alphabet[(state as usize) % alphabet.len()] as char
        })
        .collect()
}

fn assert_exact_ok(path: &Path, source_schema: &str, observation: &str) {
    let out = project(path).success().get_output().clone();
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    let want = format!("{}\n", expected(source_schema, observation));
    assert_eq!(
        stdout, want,
        "stdout must be exact projection bytes plus newline"
    );
}

fn assert_fail_args(args: &[&str]) {
    let out = assay().args(args).assert().failure().get_output().clone();
    assert!(
        out.stdout.is_empty(),
        "fail-closed stdout must be empty, got {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

fn assert_fail_body(name: &str, body: &str) {
    let dir = tmp();
    let path = write(dir.path(), name, body);
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn v0_active_maps_to_applied() {
    let dir = tmp();
    let path = write(dir.path(), "v0-active.json", &v0("active"));
    assert_exact_ok(&path, V0, "applied");
}

#[test]
fn v0_failed_maps_to_degraded() {
    let dir = tmp();
    let path = write(dir.path(), "v0-failed.json", &v0("failed"));
    assert_exact_ok(&path, V0, "degraded");
}

#[test]
fn v0_absent_maps_to_not_requested() {
    let dir = tmp();
    let path = write(dir.path(), "v0-absent.json", &v0("absent"));
    assert_exact_ok(&path, V0, "not_requested");
}

#[test]
fn v1_active_maps_to_applied() {
    assert_exact_ok(&committed_v1_active_no_probe(), V1, "applied");
}

#[test]
fn v1_failed_maps_to_degraded() {
    let dir = tmp();
    let path = write(dir.path(), "v1-failed.json", &v1("failed"));
    assert_exact_ok(&path, V1, "degraded");
}

#[test]
fn source_schema_is_the_input_identity() {
    let dir = tmp();
    let v0_path = write(dir.path(), "src-v0.json", &v0("active"));
    let v1_path = write(dir.path(), "src-v1.json", &v1("failed"));
    assert_exact_ok(&v0_path, V0, "applied");
    assert_exact_ok(&v1_path, V1, "degraded");
}

#[test]
fn missing_input_flag_is_clap_nonzero_empty_stdout() {
    assert_fail_args(&["project-enforcement-health", "--format", "json"]);
}

#[test]
fn missing_path_is_nonzero_empty_stdout() {
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        "/no/such/enforcement-health.json",
    ]);
}

#[test]
fn unknown_schema_is_nonzero_empty_stdout() {
    let dir = tmp();
    let path = write(
        dir.path(),
        "unknown.json",
        r#"{"schema":"assay.not_enforcement_health.v0"}"#,
    );
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn malformed_json_is_nonzero_empty_stdout() {
    let dir = tmp();
    let path = write(dir.path(), "malformed.json", "{");
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn oversized_valid_json_is_nonzero_empty_stdout() {
    let dir = tmp();
    let mut body = v0("active");
    body.push_str(&" ".repeat(MAX_INPUT_BYTES));
    assert!(body.len() > MAX_INPUT_BYTES);
    let path = write(dir.path(), "oversized.json", &body);
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn forged_v1_absent_is_nonzero_empty_stdout() {
    let dir = tmp();
    let path = write(dir.path(), "forged-absent.json", &v1("absent"));
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn v0_not_applicable_is_nonzero_empty_stdout() {
    let dir = tmp();
    let path = write(dir.path(), "v0-na.json", &v0("not_applicable"));
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn v0_active_attach_false_strong_is_nonzero_empty_stdout() {
    assert_fail_body(
        "v0-attach-false-strong.json",
        &v0_active_near(false, "strong"),
    );
}

#[test]
fn v0_active_attach_true_basic_is_nonzero_empty_stdout() {
    assert_fail_body("v0-attach-true-basic.json", &v0_active_near(true, "basic"));
}

#[test]
fn v1_active_with_failure_is_nonzero_empty_stdout() {
    assert_fail_body(
        "v1-active-failure.json",
        &v1_active_near("strong", true, true, true),
    );
}

#[test]
fn v1_active_basic_class_is_nonzero_empty_stdout() {
    assert_fail_body(
        "v1-active-basic.json",
        &v1_active_near("basic", true, true, false),
    );
}

#[test]
fn v1_active_no_new_privs_false_is_nonzero_empty_stdout() {
    assert_fail_body(
        "v1-active-nnp-false.json",
        &v1_active_near("strong", false, true, false),
    );
}

#[test]
fn v1_active_restrict_self_false_is_nonzero_empty_stdout() {
    assert_fail_body(
        "v1-active-restrict-false.json",
        &v1_active_near("strong", true, false, false),
    );
}

#[test]
fn verified_sandbox_degradation_bundle_maps_to_degraded() {
    let dir = tmp();
    let path = write_bundle(
        dir.path(),
        "sandbox-degraded.tar.gz",
        vec![bundle_event(
            "assay.sandbox.degraded",
            0,
            degradation_payload(),
        )],
    );
    assert_exact_ok(&path, "assay.sandbox.degraded", "degraded");
}

#[cfg(all(unix, not(target_os = "linux")))]
#[test]
fn sandbox_audit_fallback_bundle_reaches_the_same_projection() {
    let dir = tmp();
    let profile = dir.path().join("sandbox-profile.yaml");
    let bundle = dir.path().join("sandbox-bundle.tar.gz");
    let data_home = dir.path().join("data-home");
    assay()
        .env("XDG_DATA_HOME", &data_home)
        .args([
            "sandbox",
            "--enforce",
            "--enforce-net",
            "--profile",
            profile.to_str().unwrap(),
            "--bundle",
            bundle.to_str().unwrap(),
            "--",
            "echo",
            "sandbox-degradation-control",
        ])
        .assert()
        .success();
    assert!(profile.is_file(), "sandbox must write the source profile");
    assert!(bundle.is_file(), "sandbox must write the canonical bundle");
    assert_exact_ok(&bundle, "assay.sandbox.degraded", "degraded");
}

#[test]
fn bundle_without_sandbox_degradation_is_no_claim() {
    let dir = tmp();
    let path = write_bundle(
        dir.path(),
        "sandbox-summary-only.tar.gz",
        vec![bundle_event(
            "assay.sandbox.summary",
            0,
            json!({"degradation_count": 0}),
        )],
    );
    let path = path.to_str().unwrap();
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path,
    ]);
}

#[test]
fn degradation_payload_under_another_event_type_is_no_claim() {
    let dir = tmp();
    let path = write_bundle(
        dir.path(),
        "wrong-event-type.tar.gz",
        vec![bundle_event(
            "assay.profile.finished",
            0,
            degradation_payload(),
        )],
    );
    let path = path.to_str().unwrap();
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path,
    ]);
}

#[test]
fn unknown_degradation_identity_fails_closed() {
    let dir = tmp();
    let path = write_bundle(
        dir.path(),
        "unknown-degradation.tar.gz",
        vec![bundle_event(
            "assay.sandbox.degraded",
            0,
            json!({
                "reason_code": "backend_unavailable",
                "degradation_mode": "best_effort",
                "component": "landlock"
            }),
        )],
    );
    let path = path.to_str().unwrap();
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path,
    ]);
}

#[test]
fn duplicate_degradation_events_fail_closed() {
    let dir = tmp();
    let path = write_bundle(
        dir.path(),
        "duplicate-degradation.tar.gz",
        vec![
            bundle_event("assay.sandbox.degraded", 0, degradation_payload()),
            bundle_event("assay.sandbox.degraded", 1, degradation_payload()),
        ],
    );
    let path = path.to_str().unwrap();
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path,
    ]);
}

#[test]
fn active_health_and_degradation_in_one_bundle_fail_closed() {
    let dir = tmp();
    let active: serde_json::Value = serde_json::from_str(&v0("active")).unwrap();
    let path = write_bundle(
        dir.path(),
        "contradictory-health.tar.gz",
        vec![
            bundle_event("assay.sandbox.degraded", 0, degradation_payload()),
            bundle_event(V0, 1, active),
        ],
    );
    let path = path.to_str().unwrap();
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path,
    ]);
}

#[test]
fn tampered_degradation_bundle_fails_closed() {
    let dir = tmp();
    let path = write_bundle(
        dir.path(),
        "tampered-degradation.tar.gz",
        vec![bundle_event(
            "assay.sandbox.degraded",
            0,
            degradation_payload(),
        )],
    );
    tamper_events_member_without_updating_manifest(&path);
    let path = path.to_str().unwrap();
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path,
    ]);
}

#[test]
fn oversized_degradation_bundle_fails_closed() {
    let dir = tmp();
    let mut events = vec![bundle_event(
        "assay.sandbox.degraded",
        0,
        degradation_payload(),
    )];
    for seq in 1..=240 {
        events.push(bundle_event(
            "assay.sandbox.summary",
            seq,
            json!({"notes": deterministic_noise(seq, 512)}),
        ));
    }
    let path = write_bundle(dir.path(), "oversized-degradation.tar.gz", events);
    let bundle_len = fs::metadata(&path).unwrap().len() as usize;
    assert!(
        bundle_len > MAX_INPUT_BYTES,
        "bundle was only {bundle_len} bytes"
    );
    assert!(
        bundle_len < MAX_INPUT_BYTES * 2,
        "fixture must isolate the 64 KiB source cap: {bundle_len} bytes"
    );
    BundleReader::open_with_limits(
        fs::File::open(&path).unwrap(),
        VerifyLimits {
            max_bundle_bytes: (MAX_INPUT_BYTES * 2) as u64,
            max_decode_bytes: 1024 * 1024,
            max_manifest_bytes: MAX_INPUT_BYTES as u64,
            max_events_bytes: 512 * 1024,
            max_events: 1_000,
            max_line_bytes: MAX_INPUT_BYTES,
            max_path_len: 256,
            max_json_depth: 64,
        },
    )
    .expect("fixture must verify when only the source cap is relaxed");
    let path = path.to_str().unwrap();
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path,
    ]);
}
