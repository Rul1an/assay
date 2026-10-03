//! Actual-binary contract tests for `assay trust-basis verify-inputs` and
//! `assay trust-basis generate --output-dir`.
//!
//! Fixtures are written by hand here and expected digests are computed with `sha2`
//! directly, never with the production helpers, so a defect in the production
//! renderer or hasher cannot make its own expectation agree with it.

use assert_cmd::Command;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

const REPORT_FILE: &str = "trust-basis.json";
const INPUTS_FILE: &str = "trust-basis.inputs.json";

/// One claim, rendered exactly as the existing Trust Basis renderer does: serde_json
/// pretty output with two-space indentation and one trailing newline.
const REPORT_ONE_CLAIM: &str = "{\n  \"claims\": [\n    {\n      \"id\": \"bundle_verified\",\n      \"level\": \"verified\",\n      \"source\": \"bundle_verification\",\n      \"boundary\": \"bundle-wide\",\n      \"note\": null\n    }\n  ]\n}\n";

/// A different, equally well-formed report used for swap cases.
const REPORT_OTHER_CLAIM: &str = "{\n  \"claims\": [\n    {\n      \"id\": \"signing_evidence_present\",\n      \"level\": \"absent\",\n      \"source\": \"bundle_proof_surface\",\n      \"boundary\": \"proof-surfaces-only\",\n      \"note\": null\n    }\n  ]\n}\n";

const BUNDLE_BYTES: &[u8] = b"synthetic bundle bytes; the reader hashes them and never opens them";

const NOT_ESTABLISHED: [&str; 5] = [
    "claim_set_completeness",
    "environment_completeness",
    "freshness",
    "generation_authenticity",
    "pack_execution",
];

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[test]
fn independent_sha256_matches_the_published_abc_vector() {
    // FIPS 180-2 example: anchors the test-side hasher before it is trusted below.
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

const DEFAULT_LIMITS: &str = "  \"limits\": {\n    \"max_bundle_bytes\": 104857600,\n    \"max_decode_bytes\": 1073741824,\n    \"max_manifest_bytes\": 10485760,\n    \"max_events_bytes\": 104857600,\n    \"max_events\": 100000,\n    \"max_line_bytes\": 1048576,\n    \"max_path_len\": 256,\n    \"max_json_depth\": 64\n  },\n";

/// Render a sidecar by hand in the contract's field order.
fn sidecar(report: &[u8], bundle: &[u8], packs: &str, limits: &str, lint: &str) -> String {
    format!(
        "{{\n  \"schema\": \"assay.trust-basis.inputs.v0\",\n  \"report\": {{\n    \"sha256\": \"{}\",\n    \"bytes\": {}\n  }},\n  \"bundle\": {{\n    \"sha256\": \"{}\",\n    \"bytes\": {}\n  }},\n  \"pack_digest_domain\": \"assay.parsed-pack-definition.jcs.sha256\",\n{}{}{}  \"reported_assay_version\": \"{}\"\n}}\n",
        sha256_hex(report),
        report.len(),
        sha256_hex(bundle),
        bundle.len(),
        packs,
        limits,
        lint,
        env!("CARGO_PKG_VERSION"),
    )
}

const NO_PACKS: &str = "  \"packs\": [],\n";
const LINT_DISABLED: &str = "  \"lint\": {\n    \"enabled\": false\n  },\n";

fn valid_sidecar() -> String {
    sidecar(
        REPORT_ONE_CLAIM.as_bytes(),
        BUNDLE_BYTES,
        NO_PACKS,
        DEFAULT_LIMITS,
        LINT_DISABLED,
    )
}

fn one_pack() -> String {
    format!(
        "  \"packs\": [\n    {{\n      \"name\": \"synthetic-pack\",\n      \"version\": \"1.0.0\",\n      \"source_kind\": \"file\",\n      \"digest\": \"sha256:{}\"\n    }}\n  ],\n",
        sha256_hex(b"synthetic pack definition")
    )
}

fn write_set(dir: &Path, report: &str, inputs: &str) {
    fs::create_dir(dir).unwrap();
    fs::write(dir.join(REPORT_FILE), report).unwrap();
    fs::write(dir.join(INPUTS_FILE), inputs).unwrap();
}

struct Checked {
    code: Option<i32>,
    json: Value,
}

fn verify(dir: &Path, bundle: Option<&Path>) -> Checked {
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("trust-basis")
        .arg("verify-inputs")
        .arg(dir)
        .arg("--format")
        .arg("json");
    if let Some(bundle) = bundle {
        cmd.arg("--bundle").arg(bundle);
    }
    let output = cmd.output().unwrap();
    // A clap usage error also exits 2. Parsing stdout as the check record is what
    // makes a refusal assertion mean "the reader refused", not "the command is unknown".
    let json: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout is not a check record ({err}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(json["schema"], "assay.trust-basis.inputs-check.v0");
    Checked {
        code: output.status.code(),
        json,
    }
}

fn check<'a>(json: &'a Value, name: &str) -> (&'a str, &'a Value) {
    let found = json["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .find(|check| check["name"] == name)
        .unwrap_or_else(|| panic!("check {name} missing"));
    (found["status"].as_str().expect("status"), &found["reason"])
}

fn assert_refused(checked: &Checked, status: &str, failed_check: &str, reason: Option<&str>) {
    assert_eq!(checked.code, Some(2), "record: {}", checked.json);
    assert_eq!(checked.json["status"], status, "record: {}", checked.json);
    let (check_status, check_reason) = check(&checked.json, failed_check);
    assert_eq!(check_status, "failed", "record: {}", checked.json);
    if let Some(reason) = reason {
        assert_eq!(check_reason, reason, "record: {}", checked.json);
    }
}

#[test]
fn verify_inputs_binds_a_consistent_pair_and_states_what_it_does_not_establish() {
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    let inputs = valid_sidecar();
    write_set(&dir, REPORT_ONE_CLAIM, &inputs);

    let checked = verify(&dir, None);
    assert_eq!(checked.code, Some(0), "record: {}", checked.json);
    assert_eq!(checked.json["status"], "bound");
    for name in [
        "artifact_set",
        "sidecar_contract",
        "report_contract",
        "report_binding",
    ] {
        assert_eq!(check(&checked.json, name).0, "passed", "{name}");
    }
    assert_eq!(check(&checked.json, "bundle_binding").0, "not_requested");
    assert_eq!(
        checked.json["observed"]["sidecar_sha256"],
        sha256_hex(inputs.as_bytes())
    );
    assert_eq!(
        checked.json["observed"]["report_sha256"],
        sha256_hex(REPORT_ONE_CLAIM.as_bytes())
    );
    assert_eq!(checked.json["observed"]["bundle_sha256"], Value::Null);
    let not_established: Vec<&str> = checked.json["not_established"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(not_established, NOT_ESTABLISHED);
}

#[test]
fn verify_inputs_text_output_is_the_default() {
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_ONE_CLAIM, &valid_sidecar());

    let output = Command::cargo_bin("assay")
        .unwrap()
        .args(["trust-basis", "verify-inputs"])
        .arg(&dir)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("bound"), "text: {text}");
    assert!(
        text.contains("bundle_binding: not_requested"),
        "text: {text}"
    );
    assert!(text.contains("generation_authenticity"), "text: {text}");
}

#[test]
fn verify_inputs_binds_the_explicit_bundle_bytes() {
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_ONE_CLAIM, &valid_sidecar());
    let bundle = root.path().join("bundle.tar.gz");
    fs::write(&bundle, BUNDLE_BYTES).unwrap();

    let checked = verify(&dir, Some(&bundle));
    assert_eq!(checked.code, Some(0), "record: {}", checked.json);
    assert_eq!(check(&checked.json, "bundle_binding").0, "passed");
    assert_eq!(
        checked.json["observed"]["bundle_sha256"],
        sha256_hex(BUNDLE_BYTES)
    );
}

#[test]
fn verify_inputs_refuses_a_bundle_whose_bytes_differ() {
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_ONE_CLAIM, &valid_sidecar());
    let bundle = root.path().join("bundle.tar.gz");
    fs::write(&bundle, b"other bundle bytes").unwrap();

    let checked = verify(&dir, Some(&bundle));
    assert_refused(
        &checked,
        "mismatch",
        "bundle_binding",
        Some("digest_mismatch"),
    );
}

#[test]
fn verify_inputs_refuses_a_swapped_report() {
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_OTHER_CLAIM, &valid_sidecar());

    let checked = verify(&dir, None);
    assert_refused(
        &checked,
        "mismatch",
        "report_binding",
        Some("digest_mismatch"),
    );
    assert_eq!(check(&checked.json, "report_contract").0, "passed");
}

#[test]
fn verify_inputs_refuses_a_noncanonical_report_before_comparing_digests() {
    // Same claims, one extra space: the sidecar is rebuilt over these exact bytes, so
    // only canonical-form validation can refuse it.
    let report = REPORT_ONE_CLAIM.replace("\"level\": ", "\"level\":  ");
    let inputs = sidecar(
        report.as_bytes(),
        BUNDLE_BYTES,
        NO_PACKS,
        DEFAULT_LIMITS,
        LINT_DISABLED,
    );
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, &report, &inputs);

    let checked = verify(&dir, None);
    assert_refused(
        &checked,
        "invalid",
        "report_contract",
        Some("not_canonical"),
    );
    assert_eq!(check(&checked.json, "report_binding").0, "not_evaluated");
}

#[test]
fn verify_inputs_reports_an_incomplete_set_without_a_sidecar() {
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join(REPORT_FILE), REPORT_ONE_CLAIM).unwrap();

    let checked = verify(&dir, None);
    assert_refused(
        &checked,
        "incomplete",
        "artifact_set",
        Some("member_missing"),
    );
    assert_eq!(check(&checked.json, "report_binding").0, "not_evaluated");
}

#[test]
fn verify_inputs_reports_an_incomplete_set_without_a_report() {
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join(INPUTS_FILE), valid_sidecar()).unwrap();

    let checked = verify(&dir, None);
    assert_refused(
        &checked,
        "incomplete",
        "artifact_set",
        Some("member_missing"),
    );
}

#[test]
fn verify_inputs_reports_a_missing_directory_as_incomplete() {
    let root = tempdir().unwrap();
    let checked = verify(&root.path().join("absent"), None);
    assert_refused(
        &checked,
        "incomplete",
        "artifact_set",
        Some("member_missing"),
    );
}

#[test]
fn verify_inputs_refuses_a_truncated_sidecar() {
    let inputs = valid_sidecar();
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_ONE_CLAIM, &inputs[..inputs.len() / 2]);

    let checked = verify(&dir, None);
    assert_refused(
        &checked,
        "invalid",
        "sidecar_contract",
        Some("malformed_json"),
    );
}

#[test]
fn verify_inputs_refuses_a_duplicate_key_in_a_nested_sidecar_object() {
    let inputs = valid_sidecar().replace(
        "    \"max_path_len\": 256,\n",
        "    \"max_path_len\": 256,\n    \"max_path_len\": 512,\n",
    );
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_ONE_CLAIM, &inputs);

    let checked = verify(&dir, None);
    assert_refused(
        &checked,
        "invalid",
        "sidecar_contract",
        Some("duplicate_key"),
    );
}

#[test]
fn verify_inputs_refuses_a_duplicate_key_inside_a_report_claim() {
    let report = REPORT_ONE_CLAIM.replace(
        "      \"note\": null\n",
        "      \"note\": null,\n      \"level\": \"absent\"\n",
    );
    let inputs = sidecar(
        report.as_bytes(),
        BUNDLE_BYTES,
        NO_PACKS,
        DEFAULT_LIMITS,
        LINT_DISABLED,
    );
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, &report, &inputs);

    let checked = verify(&dir, None);
    assert_refused(
        &checked,
        "invalid",
        "report_contract",
        Some("duplicate_key"),
    );
}

#[test]
fn verify_inputs_refuses_unknown_sidecar_fields_and_schema() {
    let cases = [
        valid_sidecar().replace(
            "  \"pack_digest_domain\"",
            "  \"extra\": true,\n  \"pack_digest_domain\"",
        ),
        valid_sidecar().replace("assay.trust-basis.inputs.v0", "assay.trust-basis.inputs.v1"),
    ];
    for (index, inputs) in cases.iter().enumerate() {
        let root = tempdir().unwrap();
        let dir = root.path().join(format!("set-{index}"));
        write_set(&dir, REPORT_ONE_CLAIM, inputs);
        let checked = verify(&dir, None);
        assert_refused(&checked, "invalid", "sidecar_contract", None);
    }
}

#[test]
fn verify_inputs_refuses_inconsistent_lint_state() {
    let max_results_null =
        "  \"lint\": {\n    \"enabled\": true,\n    \"max_results\": null\n  },\n";
    let disabled_with_cap =
        "  \"lint\": {\n    \"enabled\": false,\n    \"max_results\": 500\n  },\n";
    let enabled = "  \"lint\": {\n    \"enabled\": true,\n    \"max_results\": 500\n  },\n";
    let cases = [
        // enabled with no packs
        (NO_PACKS.to_string(), enabled),
        // disabled with packs
        (one_pack(), LINT_DISABLED),
        // null is not omission
        (one_pack(), max_results_null),
        // a cap recorded for a run that did not lint
        (NO_PACKS.to_string(), disabled_with_cap),
    ];
    for (index, (packs, lint)) in cases.iter().enumerate() {
        let inputs = sidecar(
            REPORT_ONE_CLAIM.as_bytes(),
            BUNDLE_BYTES,
            packs,
            DEFAULT_LIMITS,
            lint,
        );
        let root = tempdir().unwrap();
        let dir = root.path().join(format!("set-{index}"));
        write_set(&dir, REPORT_ONE_CLAIM, &inputs);
        let checked = verify(&dir, None);
        assert_refused(&checked, "invalid", "sidecar_contract", None);
    }
}

#[test]
fn verify_inputs_accepts_a_recorded_zero_result_cap() {
    let lint = "  \"lint\": {\n    \"enabled\": true,\n    \"max_results\": 0\n  },\n";
    let inputs = sidecar(
        REPORT_ONE_CLAIM.as_bytes(),
        BUNDLE_BYTES,
        &one_pack(),
        DEFAULT_LIMITS,
        lint,
    );
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_ONE_CLAIM, &inputs);

    let checked = verify(&dir, None);
    assert_eq!(checked.code, Some(0), "record: {}", checked.json);
}

#[test]
fn verify_inputs_refuses_wire_integers_outside_the_contract_range() {
    let limit = |value: &str| DEFAULT_LIMITS.replace("\"max_path_len\": 256", value);
    let cases = [
        // 2^53 is one past the largest exactly representable wire integer
        limit("\"max_path_len\": 9007199254740992"),
        // limits are strictly positive
        limit("\"max_path_len\": 0"),
        // a float spelling of an integer is not an integer
        limit("\"max_path_len\": 256.0"),
        limit("\"max_path_len\": -1"),
        limit("\"max_path_len\": 2.56e2"),
    ];
    for (index, limits) in cases.iter().enumerate() {
        let inputs = sidecar(
            REPORT_ONE_CLAIM.as_bytes(),
            BUNDLE_BYTES,
            NO_PACKS,
            limits,
            LINT_DISABLED,
        );
        let root = tempdir().unwrap();
        let dir = root.path().join(format!("set-{index}"));
        write_set(&dir, REPORT_ONE_CLAIM, &inputs);
        let checked = verify(&dir, None);
        assert_refused(&checked, "invalid", "sidecar_contract", None);
    }
}

#[test]
fn verify_inputs_accepts_the_largest_wire_integer() {
    let limits = DEFAULT_LIMITS.replace(
        "\"max_bundle_bytes\": 104857600",
        "\"max_bundle_bytes\": 9007199254740991",
    );
    let inputs = sidecar(
        REPORT_ONE_CLAIM.as_bytes(),
        BUNDLE_BYTES,
        NO_PACKS,
        &limits,
        LINT_DISABLED,
    );
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_ONE_CLAIM, &inputs);
    assert_eq!(verify(&dir, None).code, Some(0));
}

#[test]
fn verify_inputs_refuses_malformed_digest_spellings() {
    let upper = valid_sidecar().replacen(
        &sha256_hex(BUNDLE_BYTES),
        &sha256_hex(BUNDLE_BYTES).to_uppercase(),
        1,
    );
    let pack_without_prefix = sidecar(
        REPORT_ONE_CLAIM.as_bytes(),
        BUNDLE_BYTES,
        &one_pack().replace("sha256:", ""),
        DEFAULT_LIMITS,
        "  \"lint\": {\n    \"enabled\": true,\n    \"max_results\": 500\n  },\n",
    );
    for (index, inputs) in [upper, pack_without_prefix].iter().enumerate() {
        let root = tempdir().unwrap();
        let dir = root.path().join(format!("set-{index}"));
        write_set(&dir, REPORT_ONE_CLAIM, inputs);
        let checked = verify(&dir, None);
        assert_refused(&checked, "invalid", "sidecar_contract", None);
    }
}

#[test]
fn verify_inputs_refuses_an_oversized_sidecar_before_parsing_it() {
    let mut inputs = valid_sidecar();
    inputs.push_str(&" ".repeat(64 * 1024));
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_ONE_CLAIM, &inputs);

    let checked = verify(&dir, None);
    assert_refused(
        &checked,
        "invalid",
        "artifact_set",
        Some("member_too_large"),
    );
    assert_eq!(checked.json["observed"]["sidecar_sha256"], Value::Null);
}

#[test]
fn verify_inputs_refuses_json_nested_beyond_the_reader_depth() {
    let deep = format!("{}1{}", "[".repeat(17), "]".repeat(17));
    let inputs = valid_sidecar().replace(
        "  \"pack_digest_domain\"",
        &format!("  \"deep\": {deep},\n  \"pack_digest_domain\""),
    );
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_ONE_CLAIM, &inputs);

    let checked = verify(&dir, None);
    assert_refused(
        &checked,
        "invalid",
        "sidecar_contract",
        Some("depth_exceeded"),
    );
}

#[test]
fn verify_inputs_refuses_unknown_and_duplicate_claim_ids() {
    let unknown = REPORT_ONE_CLAIM.replace("bundle_verified", "bundle_trusted");
    let duplicate = "{\n  \"claims\": [\n    {\n      \"id\": \"bundle_verified\",\n      \"level\": \"verified\",\n      \"source\": \"bundle_verification\",\n      \"boundary\": \"bundle-wide\",\n      \"note\": null\n    },\n    {\n      \"id\": \"bundle_verified\",\n      \"level\": \"verified\",\n      \"source\": \"bundle_verification\",\n      \"boundary\": \"bundle-wide\",\n      \"note\": null\n    }\n  ]\n}\n".to_string();
    for (index, report) in [unknown, duplicate].iter().enumerate() {
        let inputs = sidecar(
            report.as_bytes(),
            BUNDLE_BYTES,
            NO_PACKS,
            DEFAULT_LIMITS,
            LINT_DISABLED,
        );
        let root = tempdir().unwrap();
        let dir = root.path().join(format!("set-{index}"));
        write_set(&dir, report, &inputs);
        let checked = verify(&dir, None);
        assert_refused(&checked, "invalid", "report_contract", None);
    }
}

#[test]
fn verify_inputs_accepts_a_claim_subset_and_does_not_claim_completeness() {
    // One claim of ten: structurally valid, and completeness stays not established.
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(
        &dir,
        REPORT_OTHER_CLAIM,
        &sidecar(
            REPORT_OTHER_CLAIM.as_bytes(),
            BUNDLE_BYTES,
            NO_PACKS,
            DEFAULT_LIMITS,
            LINT_DISABLED,
        ),
    );
    let checked = verify(&dir, None);
    assert_eq!(checked.code, Some(0), "record: {}", checked.json);
    assert!(checked.json["not_established"]
        .as_array()
        .unwrap()
        .contains(&Value::from("claim_set_completeness")));
}

#[test]
fn verify_inputs_binds_a_coherent_pair_written_by_hand() {
    // Authenticity non-claim: nothing here came from `generate`, and the reader cannot
    // tell. This must stay `bound`; reporting it as forged would overclaim.
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    let bundle = root.path().join("bundle.tar.gz");
    fs::write(&bundle, BUNDLE_BYTES).unwrap();
    write_set(&dir, REPORT_ONE_CLAIM, &valid_sidecar());

    let checked = verify(&dir, Some(&bundle));
    assert_eq!(checked.code, Some(0));
    assert!(checked.json["not_established"]
        .as_array()
        .unwrap()
        .contains(&Value::from("generation_authenticity")));
}

#[cfg(unix)]
#[test]
fn verify_inputs_refuses_a_symlinked_member() {
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    fs::create_dir(&dir).unwrap();
    let elsewhere = root.path().join("elsewhere.json");
    fs::write(&elsewhere, REPORT_ONE_CLAIM).unwrap();
    std::os::unix::fs::symlink(&elsewhere, dir.join(REPORT_FILE)).unwrap();
    fs::write(dir.join(INPUTS_FILE), valid_sidecar()).unwrap();

    let checked = verify(&dir, None);
    assert_refused(
        &checked,
        "invalid",
        "artifact_set",
        Some("member_not_regular"),
    );
}

#[test]
fn verify_inputs_does_not_echo_hostile_payload_text() {
    let marker = "\u{1b}]52;c;payload-marker\u{7}";
    let inputs = valid_sidecar().replace(
        "  \"pack_digest_domain\"",
        &format!("  \"{marker}\": 1,\n  \"pack_digest_domain\""),
    );
    let root = tempdir().unwrap();
    let dir = root.path().join("set");
    write_set(&dir, REPORT_ONE_CLAIM, &inputs);

    let output = Command::cargo_bin("assay")
        .unwrap()
        .args(["trust-basis", "verify-inputs"])
        .arg(&dir)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    for stream in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(stream);
        assert!(!text.contains("payload-marker"), "echoed: {text}");
        assert!(!text.contains('\u{1b}'), "escape echoed: {text}");
    }
}

// ---------------------------------------------------------------- generate --output-dir

use assay_evidence::{BundleWriter, EvidenceEvent};
use chrono::{TimeZone, Utc};
use std::process::Output;

/// Effective limits of `generate` (`VerifyLimits::for_retained_events`), written out
/// by hand: the default limits with `max_events_bytes` lowered to the bundle ceiling.
const RETAINED_LIMITS: [(&str, u64); 8] = [
    ("max_bundle_bytes", 104_857_600),
    ("max_decode_bytes", 1_073_741_824),
    ("max_manifest_bytes", 10_485_760),
    ("max_events_bytes", 104_857_600),
    ("max_events", 100_000),
    ("max_line_bytes", 1_048_576),
    ("max_path_len", 256),
    ("max_json_depth", 64),
];

const FILE_PACK: &str = "name: synthetic-inputs-pack\nversion: \"1.0.0\"\nkind: quality\ndescription: Synthetic pack for Trust Basis input tests\nauthor: Assay Tests\nlicense: Apache-2.0\nrequires:\n  assay_min_version: \">=3.2.3\"\n  evidence_schema_version: \"1.0\"\nrules:\n  - id: SYN-001\n    severity: info\n    description: The bundle carries at least one event.\n    check:\n      type: event_count\n      min: 1\n";

fn decision_event(run_id: &str, seq: u64, second: i64) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        "assay.tool.decision",
        "urn:assay:test:trust-basis-inputs",
        run_id,
        seq,
        serde_json::json!({
            "tool": "tool.commit",
            "decision": "allow",
            "principal": "user:alice"
        }),
    );
    event.time = Utc.timestamp_opt(second, 0).unwrap();
    event
}

/// Write a one-event bundle. `second` changes the raw archive bytes without changing
/// anything a Trust Basis claim reads.
fn write_bundle_at(path: &Path, second: i64) {
    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    writer.add_event(decision_event("run_inputs", 0, second));
    writer.finish().unwrap();
}

fn assay(args: &[&std::ffi::OsStr]) -> Output {
    Command::cargo_bin("assay")
        .unwrap()
        .args(args)
        .output()
        .unwrap()
}

fn generate(bundle: &Path, extra: &[&str]) -> Output {
    let mut args: Vec<&std::ffi::OsStr> = vec!["trust-basis".as_ref(), "generate".as_ref()];
    args.push(bundle.as_os_str());
    args.extend(extra.iter().map(|arg| std::ffi::OsStr::new(*arg)));
    assay(&args)
}

fn generate_into(bundle: &Path, dir: &Path, extra: &[&str]) -> Output {
    let mut args: Vec<&std::ffi::OsStr> = vec!["trust-basis".as_ref(), "generate".as_ref()];
    args.push(bundle.as_os_str());
    args.push("--output-dir".as_ref());
    args.push(dir.as_os_str());
    args.extend(extra.iter().map(|arg| std::ffi::OsStr::new(*arg)));
    assay(&args)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A clap usage error also exits 2. Every refusal below asserts it is not one, so a
/// missing flag cannot pass for a refused destination.
fn assert_not_usage_error(output: &Output) {
    let text = stderr(output);
    assert!(
        !text.contains("Usage:") && !text.contains("unexpected argument"),
        "clap usage error, not a command result: {text}"
    );
}

fn assert_generated(output: &Output) {
    assert_not_usage_error(output);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(output));
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn dir_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn generate_output_dir_writes_a_pair_whose_bytes_bind() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let dir = root.path().join("out");

    let output = generate_into(&bundle, &dir, &[]);
    assert_generated(&output);
    assert!(output.stdout.is_empty(), "the report goes to the directory only");
    assert_eq!(dir_entries(&dir), [REPORT_FILE, INPUTS_FILE]);

    // The report is exactly what legacy stdout produces for the same bundle.
    let legacy = generate(&bundle, &[]);
    assert_eq!(legacy.status.code(), Some(0));
    let report = fs::read(dir.join(REPORT_FILE)).unwrap();
    assert_eq!(report, legacy.stdout);

    let bundle_bytes = fs::read(&bundle).unwrap();
    let sidecar = read_json(&dir.join(INPUTS_FILE));
    assert_eq!(sidecar["schema"], "assay.trust-basis.inputs.v0");
    assert_eq!(sidecar["report"]["sha256"], sha256_hex(&report));
    assert_eq!(sidecar["report"]["bytes"], report.len() as u64);
    assert_eq!(sidecar["bundle"]["sha256"], sha256_hex(&bundle_bytes));
    assert_eq!(sidecar["bundle"]["bytes"], bundle_bytes.len() as u64);
    assert_eq!(
        sidecar["pack_digest_domain"],
        "assay.parsed-pack-definition.jcs.sha256"
    );
    assert_eq!(sidecar["packs"], serde_json::json!([]));
    assert_eq!(sidecar["lint"], serde_json::json!({ "enabled": false }));
    assert_eq!(sidecar["reported_assay_version"], env!("CARGO_PKG_VERSION"));
    let limits = sidecar["limits"].as_object().unwrap();
    assert_eq!(limits.len(), RETAINED_LIMITS.len());
    for (name, value) in RETAINED_LIMITS {
        assert_eq!(limits[name], value, "{name}");
    }

    let checked = verify(&dir, Some(&bundle));
    assert_eq!(checked.code, Some(0), "record: {}", checked.json);
    assert_eq!(checked.json["status"], "bound");
    assert_eq!(check(&checked.json, "bundle_binding").0, "passed");
}

#[test]
fn generate_output_dir_records_packs_in_load_order_with_duplicates() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let pack = root.path().join("pack.yaml");
    fs::write(&pack, FILE_PACK).unwrap();
    let dir = root.path().join("out");
    let refs = format!(
        "owasp-agentic-a3-a5-signal-followup,{},owasp-agentic-a3-a5-signal-followup",
        pack.display()
    );

    let output = generate_into(&bundle, &dir, &["--pack", &refs]);
    assert_generated(&output);
    let sidecar = read_json(&dir.join(INPUTS_FILE));
    let packs = sidecar["packs"].as_array().unwrap();
    let summary: Vec<(&str, &str, &str)> = packs
        .iter()
        .map(|pack| {
            (
                pack["name"].as_str().unwrap(),
                pack["version"].as_str().unwrap(),
                pack["source_kind"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("owasp-agentic-a3-a5-signal-followup", "1.0.0", "builtin"),
            ("synthetic-inputs-pack", "1.0.0", "file"),
            ("owasp-agentic-a3-a5-signal-followup", "1.0.0", "builtin"),
        ]
    );
    assert_eq!(packs[0]["digest"], packs[2]["digest"]);
    assert_ne!(packs[0]["digest"], packs[1]["digest"]);
    for pack in packs {
        let digest = pack["digest"].as_str().unwrap();
        let hex = digest.strip_prefix("sha256:").expect("sha256: prefix");
        assert_eq!(hex.len(), 64);
        assert!(hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    }
    assert_eq!(
        sidecar["lint"],
        serde_json::json!({ "enabled": true, "max_results": 500 })
    );
    // No path is recorded, only what the pack parsed to.
    let text = fs::read_to_string(dir.join(INPUTS_FILE)).unwrap();
    assert!(!text.contains(&root.path().display().to_string()));

    let legacy = generate(&bundle, &["--pack", &refs]);
    assert_eq!(fs::read(dir.join(REPORT_FILE)).unwrap(), legacy.stdout);
    assert_eq!(verify(&dir, Some(&bundle)).code, Some(0));
}

#[test]
fn generate_output_dir_records_a_zero_result_cap_as_enabled_lint() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let dir = root.path().join("out");

    let output = generate_into(
        &bundle,
        &dir,
        &[
            "--pack",
            "owasp-agentic-a3-a5-signal-followup",
            "--max-results",
            "0",
        ],
    );
    assert_generated(&output);
    let sidecar = read_json(&dir.join(INPUTS_FILE));
    assert_eq!(
        sidecar["lint"],
        serde_json::json!({ "enabled": true, "max_results": 0 })
    );
    assert_eq!(verify(&dir, Some(&bundle)).code, Some(0));
}

#[test]
fn generate_output_dir_pack_digest_follows_parsed_content_not_yaml_text() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let variants = [
        ("base", FILE_PACK.to_string()),
        ("comment", format!("# a comment changes no parsed field\n{FILE_PACK}")),
        (
            "content",
            FILE_PACK.replace("at least one event", "one event or more"),
        ),
    ];
    let mut digests = Vec::new();
    for (label, yaml) in &variants {
        let pack = root.path().join(format!("{label}.yaml"));
        fs::write(&pack, yaml).unwrap();
        let dir = root.path().join(format!("out-{label}"));
        let output = generate_into(&bundle, &dir, &["--pack", pack.to_str().unwrap()]);
        assert_generated(&output);
        let sidecar = read_json(&dir.join(INPUTS_FILE));
        digests.push(sidecar["packs"][0]["digest"].as_str().unwrap().to_string());
    }
    assert_eq!(digests[0], digests[1], "a YAML comment is not pack content");
    assert_ne!(digests[0], digests[2], "a changed description is pack content");
}

/// Characterisation, not repair: a pack file with a repeated YAML key is handled the
/// same way with and without `--output-dir`, and a refusal creates no directory.
#[test]
fn generate_output_dir_matches_legacy_on_a_duplicate_yaml_key() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let pack = root.path().join("dup.yaml");
    fs::write(
        &pack,
        FILE_PACK.replace("license: Apache-2.0\n", "license: Apache-2.0\nlicense: MIT\n"),
    )
    .unwrap();
    let dir = root.path().join("out");

    let legacy = generate(&bundle, &["--pack", pack.to_str().unwrap()]);
    let output = generate_into(&bundle, &dir, &["--pack", pack.to_str().unwrap()]);
    assert_not_usage_error(&output);
    assert_eq!(output.status.code(), legacy.status.code());
    // Observed today: serde_yaml refuses the repeated key, so both exit 2.
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(!dir.exists(), "a refused generation must not create the directory");
}

/// Characterisation: an empty pack reference names no pack, and the refusal happens
/// before any output exists.
#[test]
fn generate_output_dir_refuses_an_empty_pack_reference_without_creating_the_directory() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let dir = root.path().join("out");

    let legacy = generate(&bundle, &["--pack", ""]);
    let output = generate_into(&bundle, &dir, &["--pack", ""]);
    assert_not_usage_error(&output);
    assert_eq!(output.status.code(), legacy.status.code());
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(!dir.exists());
}

#[test]
fn generate_output_dir_conflicts_with_out() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let dir = root.path().join("out");
    let out = root.path().join("report.json");

    let output = generate_into(&bundle, &dir, &["--out", out.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("cannot be used with"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!dir.exists());
    assert!(!out.exists());
}

fn assert_destination_refused(output: &Output) {
    assert_not_usage_error(output);
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(output));
    assert!(
        stderr(output).contains("trust basis output directory"),
        "stderr: {}",
        stderr(output)
    );
}

#[test]
fn generate_output_dir_refuses_an_existing_directory_and_leaves_it_untouched() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let dir = root.path().join("out");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join(REPORT_FILE), "earlier bytes").unwrap();

    let output = generate_into(&bundle, &dir, &[]);
    assert_destination_refused(&output);
    assert_eq!(dir_entries(&dir), [REPORT_FILE]);
    assert_eq!(fs::read(dir.join(REPORT_FILE)).unwrap(), b"earlier bytes");
}

#[test]
fn generate_output_dir_refuses_an_existing_empty_directory() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let dir = root.path().join("out");
    fs::create_dir(&dir).unwrap();

    let output = generate_into(&bundle, &dir, &[]);
    assert_destination_refused(&output);
    assert!(dir_entries(&dir).is_empty());
}

#[cfg(unix)]
#[test]
fn generate_output_dir_refuses_a_symlink_even_to_an_empty_directory() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    let link = root.path().join("out");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let output = generate_into(&bundle, &link, &[]);
    assert_destination_refused(&output);
    assert!(dir_entries(&target).is_empty());
}

#[test]
fn generate_output_dir_refuses_a_missing_parent() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let dir = root.path().join("absent").join("out");

    let output = generate_into(&bundle, &dir, &[]);
    assert_destination_refused(&output);
    assert!(!root.path().join("absent").exists(), "no parent is created");
}

#[test]
fn generate_output_dir_refuses_a_parent_that_is_a_file() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let parent = root.path().join("file");
    fs::write(&parent, "not a directory").unwrap();

    let output = generate_into(&bundle, &parent.join("out"), &[]);
    assert_destination_refused(&output);
    assert_eq!(fs::read(&parent).unwrap(), b"not a directory");
}

#[test]
fn generate_output_dir_creates_nothing_when_generation_fails() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    fs::write(&bundle, b"not a gzip archive").unwrap();
    let dir = root.path().join("out");

    let output = generate_into(&bundle, &dir, &[]);
    assert_not_usage_error(&output);
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("failed to generate trust basis"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!dir.exists());
}

#[test]
fn generate_out_and_stdout_keep_their_bytes_and_write_no_sidecar() {
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let out = root.path().join("nested").join("report.json");

    let stdout = generate(&bundle, &[]);
    let to_file = generate(&bundle, &["--out", out.to_str().unwrap()]);
    assert_eq!(to_file.status.code(), Some(0));
    assert!(to_file.stdout.is_empty());
    assert_eq!(fs::read(&out).unwrap(), stdout.stdout);
    assert_eq!(dir_entries(&root.path().join("nested")), ["report.json"]);
    assert_eq!(dir_entries(root.path()), ["bundle.tar.gz", "nested"]);
}

#[test]
fn a_changed_raw_bundle_with_the_same_claims_does_not_bind() {
    let root = tempdir().unwrap();
    let original = root.path().join("original.tar.gz");
    let changed = root.path().join("changed.tar.gz");
    write_bundle_at(&original, 1_700_000_000);
    write_bundle_at(&changed, 1_700_000_001);
    assert_ne!(fs::read(&original).unwrap(), fs::read(&changed).unwrap());
    // Same claims from both: the report alone cannot tell the bundles apart.
    assert_eq!(generate(&original, &[]).stdout, generate(&changed, &[]).stdout);

    let dir = root.path().join("out");
    assert_generated(&generate_into(&original, &dir, &[]));
    assert_eq!(verify(&dir, Some(&original)).code, Some(0));
    let checked = verify(&dir, Some(&changed));
    assert_refused(
        &checked,
        "mismatch",
        "bundle_binding",
        Some("digest_mismatch"),
    );
}

#[test]
fn an_old_coherent_pair_still_binds_and_freshness_stays_unestablished() {
    // Generated from one bundle, then the bundle moves on. Without --bundle the old
    // pair is consistent with itself, which is all the reader can say.
    let root = tempdir().unwrap();
    let bundle = root.path().join("bundle.tar.gz");
    write_bundle_at(&bundle, 1_700_000_000);
    let dir = root.path().join("out");
    assert_generated(&generate_into(&bundle, &dir, &[]));
    write_bundle_at(&bundle, 1_700_000_100);

    let checked = verify(&dir, None);
    assert_eq!(checked.code, Some(0), "record: {}", checked.json);
    assert!(checked.json["not_established"]
        .as_array()
        .unwrap()
        .contains(&Value::from("freshness")));
    let checked = verify(&dir, Some(&bundle));
    assert_refused(&checked, "mismatch", "bundle_binding", None);
}

#[test]
fn generated_pairs_refuse_a_swapped_whitespace_or_truncated_member() {
    let root = tempdir().unwrap();
    let first_bundle = root.path().join("first.tar.gz");
    write_bundle_at(&first_bundle, 1_700_000_000);
    let pack = root.path().join("pack.yaml");
    fs::write(&pack, FILE_PACK).unwrap();

    let fresh = |name: &str, extra: &[&str]| {
        let dir = root.path().join(name);
        assert_generated(&generate_into(&first_bundle, &dir, extra));
        dir
    };

    // A report from a run with packs, under the sidecar of a run without them.
    let plain = fresh("plain", &[]);
    let with_pack = fresh("with-pack", &["--pack", pack.to_str().unwrap()]);
    assert_ne!(
        fs::read(plain.join(REPORT_FILE)).unwrap(),
        fs::read(with_pack.join(REPORT_FILE)).unwrap()
    );
    fs::copy(with_pack.join(REPORT_FILE), plain.join(REPORT_FILE)).unwrap();
    assert_refused(
        &verify(&plain, None),
        "mismatch",
        "report_binding",
        Some("digest_mismatch"),
    );

    let spaced = fresh("spaced", &[]);
    let mut sidecar = fs::read(spaced.join(INPUTS_FILE)).unwrap();
    sidecar.insert(1, b' ');
    fs::write(spaced.join(INPUTS_FILE), sidecar).unwrap();
    assert_refused(
        &verify(&spaced, None),
        "invalid",
        "sidecar_contract",
        Some("not_canonical"),
    );

    let truncated = fresh("truncated", &[]);
    let report = fs::read(truncated.join(REPORT_FILE)).unwrap();
    fs::write(truncated.join(REPORT_FILE), &report[..report.len() - 1]).unwrap();
    assert_refused(&verify(&truncated, None), "invalid", "report_contract", None);

    let missing = fresh("missing", &[]);
    fs::remove_file(missing.join(INPUTS_FILE)).unwrap();
    assert_refused(
        &verify(&missing, None),
        "incomplete",
        "artifact_set",
        Some("member_missing"),
    );
}
