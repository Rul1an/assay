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
