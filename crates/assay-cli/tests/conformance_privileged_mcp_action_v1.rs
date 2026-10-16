//! Focused conformance for privileged-mcp-action/v1.
//!
//! Default invocation stays v0. v1 requires `--profile-version v1`.
//! Historical v0 corpus digest remains byte-exact.

use assay_evidence::bundle::BundleReader;
use assay_evidence::{BundleWriter, EvidenceEvent};
use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs::File;
use std::path::{Path, PathBuf};

const REPORT_SCHEMA: &str = "assay.privileged_mcp_action.verify.report.v0";
const PROFILE_V0: &str = "privileged-mcp-action/v0";
const PROFILE_V1: &str = "privileged-mcp-action/v1";
const V0_CORPUS_DIGEST: &str =
    "sha256:cb58ce91863f52e0568742b977f0642158453ec11bbcd25821f9171dccd03342";

const REPORT_NON_CLAIMS: [&str; 4] = [
    "allow does not prove upstream delivery",
    "deny does not establish maliciousness",
    "caller-visible denial does not prove external side-effect absence",
    "bundle integrity does not upgrade source class",
];

const OBSERVATION_V0: &str = "assay.denied_call_observation.v0";
const OBSERVATION_V1: &str = "assay.denied_call_observation.v1";

// Exact published remediation strings. These are literals on purpose: the tests below must not
// ask the verifier which string applies.
const PROFILE_INVALID_NEXT_STEP: &str = "Obtain or reissue evidence whose records satisfy the named evidence profile; per-violation details are in findings";
const INTEGRITY_NEXT_STEP: &str = "Obtain an undamaged bundle from its producer; the content this bundle carries does not match what it records";
const INTERPRETER_CHECK_NEXT_STEP: &str = "Check the selected interpreter against the producer contract or your required profile. Do not switch profiles solely to obtain a passing result. If the selection is intended, resolve the incompatible records with the producer.";
const INTERPRETER_CHECK_DEFAULT_NEXT_STEP: &str = "No --profile-version was given; v0 was applied by default. Check the selected interpreter against the producer contract or your required profile. Do not switch profiles solely to obtain a passing result. If the selection is intended, resolve the incompatible records with the producer.";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn v0_corpus() -> PathBuf {
    repo_root().join("conformance/privileged-mcp-action-v0")
}

fn v1_corpus() -> PathBuf {
    repo_root().join("conformance/privileged-mcp-action-v1")
}

fn verify(bundle: &Path, profile_version: Option<&str>) -> (Value, i32) {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.args(["evidence", "verify-privileged-mcp-action"])
        .arg(bundle)
        .args(["--format", "json"]);
    if let Some(version) = profile_version {
        cmd.args(["--profile-version", version]);
    }
    let output = cmd.output().expect("run verifier");
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("report for {} is not JSON: {e}", bundle.display()));
    (report, output.status.code().expect("exit code"))
}

#[test]
fn v0_corpus_digest_remains_pinned() {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(v0_corpus().join("MANIFEST.json")).expect("v0 MANIFEST"),
    )
    .expect("parse v0 MANIFEST");
    assert_eq!(manifest["corpus_digest"], V0_CORPUS_DIGEST);
    assert_eq!(manifest["vectors"].as_array().map(Vec::len), Some(14));
}

#[test]
fn default_invocation_pins_v0_on_v1_corpus_accept() {
    let bundle = v1_corpus().join("vectors/ok-002-deny-observation-missing.bundle.tar.gz");
    let (report, exit) = verify(&bundle, None);
    assert_eq!(exit, 0);
    assert_eq!(report["schema"], REPORT_SCHEMA);
    assert_eq!(report["profile"], PROFILE_V0);
    assert_ne!(report["profile"], PROFILE_V1);
}

#[test]
fn explicit_v1_never_falls_back_to_v0() {
    let bundle = v1_corpus().join("vectors/ok-002-deny-observation-missing.bundle.tar.gz");
    let (report, exit) = verify(&bundle, Some("v1"));
    assert_eq!(exit, 0);
    assert_eq!(report["schema"], REPORT_SCHEMA);
    assert_eq!(report["profile"], PROFILE_V1);
}

#[test]
fn v1_corpus_matches_manifest_under_explicit_v1() {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(v1_corpus().join("MANIFEST.json")).expect("v1 MANIFEST"),
    )
    .expect("parse v1 MANIFEST");
    let vectors = manifest["vectors"].as_array().expect("vectors");
    assert_eq!(vectors.len(), 7, "focused v1 corpus");

    for vector in vectors {
        let id = vector["id"].as_str().expect("id");
        let file = v1_corpus().join(vector["file"].as_str().expect("file"));
        let expected = &vector["expected"];
        let (report, exit) = verify(&file, Some("v1"));

        assert_eq!(report["schema"], REPORT_SCHEMA, "{id}");
        assert_eq!(report["profile"], PROFILE_V1, "{id}: explicit v1");
        assert_eq!(
            report["bundle_integrity"], expected["bundle_integrity"],
            "{id}: integrity"
        );
        if expected["bundle_integrity"] == "fail" {
            assert!(report.get("verdict").is_none(), "{id}");
            assert_eq!(exit, 2, "{id}");
            continue;
        }
        assert_eq!(report["verdict"], expected["verdict"], "{id}: verdict");
        if expected["verdict"] == "valid" {
            assert_eq!(&report["claims"], &expected["claims"], "{id}: claims");
            assert_eq!(exit, 0, "{id}");
        } else {
            assert!(report.get("claims").is_none(), "{id}");
            assert_eq!(exit, 2, "{id}");
        }
        match expected_next_step_under_v1(id) {
            None => {
                assert!(report.get("reason_code").is_none(), "{id}: no reason_code");
                assert!(report.get("next_step").is_none(), "{id}: no next_step");
            }
            Some(next_step) => {
                assert_eq!(report["reason_code"], "E_EVIDENCE_PROFILE_INVALID", "{id}");
                assert_eq!(report["next_step"], next_step, "{id}: next_step");
            }
        }
    }
}

/// Pin every committed v1 vector by exact id so a new corpus member cannot inherit a class.
fn expected_next_step_under_v1(id: &str) -> Option<&'static str> {
    match id {
        "ok-001-deny-bound-v1-observation"
        | "ok-002-deny-observation-missing"
        | "ok-003-cross-pair-inert"
        | "ok-004-allow-contradicted-by-v1-denial" => None,
        // The only violation is a v0 observation, which the v0 interpreter recognizes.
        "bad-201-v0-observation-under-v1" => Some(INTERPRETER_CHECK_NEXT_STEP),
        // A v1 observation is present too, so no selection accepts this bundle.
        "bad-202-mixed-marker-versions" => Some(PROFILE_INVALID_NEXT_STEP),
        "bad-203-v1-marker-without-decision" => Some(PROFILE_INVALID_NEXT_STEP),
        other => panic!("unmapped privileged-mcp-action v1 vector {other}"),
    }
}

#[test]
fn default_v0_rejects_v1_observation_bundle() {
    let bundle = v1_corpus().join("vectors/ok-001-deny-bound-v1-observation.bundle.tar.gz");
    let (report, exit) = verify(&bundle, None);
    assert_eq!(exit, 2);
    assert_eq!(report["profile"], PROFILE_V0);
    assert_eq!(report["verdict"], "invalid");
    assert!(report["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|f| f["id"] == "unknown_profile_schema"));
}

// ---------------------------------------------------------------------------------------------
// Remediation for a rejection whose only cause is another interpreter's observation record.
//
// The verdict, the findings and the exit code do not change. Only `next_step` does, and only
// when every violation is an unrecognized observation that another shipped interpreter of this
// profile recognizes and the selected interpreter's own observation is absent. The guidance
// never names another profile version: the bundle carries no profile id, so which interpreter
// is intended is known to the producer contract or the caller, not to the verifier.
// ---------------------------------------------------------------------------------------------

fn v1_vector(name: &str) -> PathBuf {
    v1_corpus().join(format!("vectors/{name}.bundle.tar.gz"))
}

fn v0_vector(name: &str) -> PathBuf {
    v0_corpus().join(format!("vectors/{name}.bundle.tar.gz"))
}

fn verify_table(bundle: &Path, profile_version: Option<&str>) -> (String, i32) {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.args(["evidence", "verify-privileged-mcp-action"])
        .arg(bundle)
        .args(["--format", "table"]);
    if let Some(version) = profile_version {
        cmd.args(["--profile-version", version]);
    }
    let output = cmd.output().expect("run verifier table");
    (
        String::from_utf8(output.stdout).expect("table stdout utf-8"),
        output.status.code().expect("exit code"),
    )
}

fn table_next_step(table: &str) -> Option<&str> {
    table
        .lines()
        .find_map(|line| line.strip_prefix("Next step:        "))
}

fn finding_ids(report: &Value) -> Vec<&str> {
    report["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .map(|f| f["id"].as_str().expect("finding id"))
        .collect()
}

fn observed_schemas(report: &Value) -> Vec<Option<&str>> {
    report["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .map(|f| f.get("observed_schema").and_then(Value::as_str))
        .collect()
}

/// The whole rejection report for one unrecognized observation, minus `next_step`. Written out
/// from the report contract rather than read back from the verifier.
fn single_unrecognized_observation_report(profile: &str, selection: &str, observed: &str) -> Value {
    json!({
        "schema": REPORT_SCHEMA,
        "profile": profile,
        "profile_selection": selection,
        "input_profile": null,
        "input_profile_status": "undeclared_legacy",
        "bundle_integrity": "pass",
        "verdict": "invalid",
        "findings": [{
            "id": "unknown_profile_schema",
            "detail": format!(
                "payload schema \"{observed}\" is inside the profile namespace but is not a recognized {profile} record; unknown fails closed"
            ),
            "observed_schema": observed,
        }],
        "non_claims": REPORT_NON_CLAIMS,
        "reason_code": "E_EVIDENCE_PROFILE_INVALID",
        "evaluator": expected_evaluator(),
    })
}

struct ApplicableCase {
    label: &'static str,
    bundle: PathBuf,
    profile_version: Option<&'static str>,
    profile: &'static str,
    selection: &'static str,
    observed: &'static str,
    next_step: &'static str,
}

fn applicable_cases() -> Vec<ApplicableCase> {
    vec![
        ApplicableCase {
            label: "v1 observation, no flag",
            bundle: v1_vector("ok-001-deny-bound-v1-observation"),
            profile_version: None,
            profile: PROFILE_V0,
            selection: "default",
            observed: OBSERVATION_V1,
            next_step: INTERPRETER_CHECK_DEFAULT_NEXT_STEP,
        },
        ApplicableCase {
            label: "v1 observation, explicit v0",
            bundle: v1_vector("ok-001-deny-bound-v1-observation"),
            profile_version: Some("v0"),
            profile: PROFILE_V0,
            selection: "explicit",
            observed: OBSERVATION_V1,
            next_step: INTERPRETER_CHECK_NEXT_STEP,
        },
        ApplicableCase {
            label: "bad-201, explicit v1",
            bundle: v1_vector("bad-201-v0-observation-under-v1"),
            profile_version: Some("v1"),
            profile: PROFILE_V1,
            selection: "explicit",
            observed: OBSERVATION_V0,
            next_step: INTERPRETER_CHECK_NEXT_STEP,
        },
        ApplicableCase {
            label: "v0 corpus accept vector, explicit v1",
            bundle: v0_vector("ok-001-deny-bound-observation"),
            profile_version: Some("v1"),
            profile: PROFILE_V1,
            selection: "explicit",
            observed: OBSERVATION_V0,
            next_step: INTERPRETER_CHECK_NEXT_STEP,
        },
    ]
}

#[test]
fn default_v0_on_a_v1_observation_says_the_default_applied_and_asks_for_an_interpreter_check() {
    let (report, exit) = verify(&v1_vector("ok-001-deny-bound-v1-observation"), None);
    assert_eq!(exit, 2);
    assert_eq!(report["profile_selection"], "default");
    assert_eq!(report["next_step"], INTERPRETER_CHECK_DEFAULT_NEXT_STEP);
}

#[test]
fn explicit_v0_on_a_v1_observation_asks_for_an_interpreter_check_without_the_default_prefix() {
    let (report, exit) = verify(&v1_vector("ok-001-deny-bound-v1-observation"), Some("v0"));
    assert_eq!(exit, 2);
    assert_eq!(report["profile_selection"], "explicit");
    assert_eq!(report["next_step"], INTERPRETER_CHECK_NEXT_STEP);
}

#[test]
fn explicit_v1_on_a_v0_observation_asks_for_an_interpreter_check() {
    for bundle in [
        v1_vector("bad-201-v0-observation-under-v1"),
        v0_vector("ok-001-deny-bound-observation"),
    ] {
        let (report, exit) = verify(&bundle, Some("v1"));
        assert_eq!(exit, 2, "{}", bundle.display());
        assert_eq!(report["profile_selection"], "explicit");
        assert_eq!(
            report["next_step"],
            INTERPRETER_CHECK_NEXT_STEP,
            "{}",
            bundle.display()
        );
    }
}

/// Parity: everything except `next_step` is what the report contract already said.
#[test]
fn interpreter_check_changes_no_other_report_field() {
    for case in applicable_cases() {
        let (mut report, exit) = verify(&case.bundle, case.profile_version);
        assert_eq!(exit, 2, "{}: exit code", case.label);
        assert!(
            report.get("claims").is_none(),
            "{}: claims stay absent",
            case.label
        );
        let next_step = report
            .as_object_mut()
            .expect("report object")
            .remove("next_step");
        assert!(
            next_step.is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty())),
            "{}: a rejection carries a next_step",
            case.label
        );
        assert_eq!(
            report,
            single_unrecognized_observation_report(case.profile, case.selection, case.observed),
            "{}: report without next_step",
            case.label
        );
    }
}

#[test]
fn table_and_json_publish_the_same_interpreter_check() {
    for case in applicable_cases() {
        let (report, _) = verify(&case.bundle, case.profile_version);
        let (table, exit) = verify_table(&case.bundle, case.profile_version);
        assert_eq!(exit, 2, "{}", case.label);
        assert_eq!(
            table_next_step(&table),
            Some(case.next_step),
            "{}: table next step, got:\n{table}",
            case.label
        );
        assert_eq!(
            report["next_step"].as_str(),
            table_next_step(&table),
            "{}: JSON and table disagree",
            case.label
        );
        assert!(
            table.contains("Reason code:      E_EVIDENCE_PROFILE_INVALID"),
            "{}: reason code unchanged, got:\n{table}",
            case.label
        );
    }
}

/// The guidance may state what was selected. It may not name another profile version, promise a
/// passing rerun, or publish an invocation.
#[test]
fn interpreter_check_names_no_alternative_and_publishes_no_invocation() {
    for case in applicable_cases() {
        let (report, _) = verify(&case.bundle, case.profile_version);
        let next_step = report["next_step"].as_str().expect("next_step string");
        let (selected, other) = match case.profile {
            PROFILE_V0 => ("v0", "v1"),
            PROFILE_V1 => ("v1", "v0"),
            unknown => panic!("unmapped profile {unknown}"),
        };
        assert!(
            !next_step.contains(other),
            "{}: names the unselected version {other}: {next_step}",
            case.label
        );
        assert!(
            !next_step.contains("privileged-mcp-action/"),
            "{}: names a profile id: {next_step}",
            case.label
        );
        assert!(
            !next_step.contains("Run argv") && !next_step.contains("Run:"),
            "{}: publishes an invocation: {next_step}",
            case.label
        );
        // The same prose-only properties the reissue text is held to.
        assert!(
            !next_step.contains("verify-privileged-mcp-action")
                && !next_step.contains('/')
                && !next_step.contains('\\'),
            "{}: interpolates a command or path: {next_step}",
            case.label
        );
        assert!(
            !next_step.contains(case.observed),
            "{}: repeats the observed schema as advice: {next_step}",
            case.label
        );
        let states_default = next_step.starts_with("No --profile-version was given");
        assert_eq!(
            states_default,
            case.selection == "default",
            "{}: the default sentence describes argv and appears only when no flag was given",
            case.label
        );
        if states_default {
            assert!(
                next_step.contains(&format!("{selected} was applied by default")),
                "{}: default sentence names the interpreter that ran",
                case.label
            );
        } else {
            assert!(
                !next_step.contains(selected),
                "{}: explicit guidance needs no version at all: {next_step}",
                case.label
            );
        }
    }
}

fn fixture_payloads(bundle: &Path) -> Vec<Value> {
    let file = File::open(bundle).expect("open fixture bundle");
    BundleReader::open(file)
        .expect("fixture bundle verifies")
        .events_vec()
        .expect("fixture events")
        .into_iter()
        .map(|event| event.payload)
        .collect()
}

/// A decision record every interpreter accepts, lifted from a committed accept vector.
fn accepted_decision() -> Value {
    let mut payloads = fixture_payloads(&v1_vector("ok-002-deny-observation-missing"));
    assert_eq!(payloads.len(), 1, "ok-002 carries the decision only");
    payloads.remove(0)
}

fn observation_v1() -> Value {
    fixture_payloads(&v1_vector("ok-001-deny-bound-v1-observation"))
        .into_iter()
        .find(|p| p["schema"] == OBSERVATION_V1)
        .expect("v1 observation payload")
}

fn observation_v0() -> Value {
    fixture_payloads(&v1_vector("bad-201-v0-observation-under-v1"))
        .into_iter()
        .find(|p| p["schema"] == OBSERVATION_V0)
        .expect("v0 observation payload")
}

fn with_schema(mut payload: Value, schema: &str) -> Value {
    payload["schema"] = json!(schema);
    payload
}

fn write_bundle(path: &Path, records: &[(&str, Value)]) {
    let file = File::create(path).expect("create bundle");
    let mut writer = BundleWriter::new(file);
    for (seq, (type_, payload)) in records.iter().enumerate() {
        writer.add_event(EvidenceEvent::new(
            *type_,
            "urn:assay:test:profile-remediation",
            "run",
            seq as u64,
            payload.clone(),
        ));
    }
    writer.finish().expect("finish bundle");
}

struct GenericCase {
    label: &'static str,
    bundle: PathBuf,
    profile_version: Option<&'static str>,
    finding_ids: &'static [&'static str],
    observed_schemas: &'static [Option<&'static str>],
}

/// Rejections that keep the reissue guidance. Each row pins the findings as well, so the row
/// cannot silently turn into a different case.
#[test]
fn rejections_outside_the_narrow_case_keep_the_reissue_guidance() {
    let dir = tempfile::tempdir().expect("tempdir");
    let synthetic = |name: &str, records: &[(&str, Value)]| {
        let path = dir.path().join(format!("{name}.bundle.tar.gz"));
        write_bundle(&path, records);
        path
    };
    const FUTURE_OBSERVATION: &str = "assay.denied_call_observation.v2";

    let missing_payload_schema = synthetic(
        "missing-payload-schema",
        &[
            ("assay.enforcement_decision.v0", accepted_decision()),
            (OBSERVATION_V1, json!({"note": "no schema member"})),
        ],
    );
    let future_observation = synthetic(
        "future-observation",
        &[
            ("assay.enforcement_decision.v0", accepted_decision()),
            (
                FUTURE_OBSERVATION,
                with_schema(observation_v1(), FUTURE_OBSERVATION),
            ),
        ],
    );
    let other_interpreter_plus_future = synthetic(
        "other-interpreter-plus-future",
        &[
            ("assay.enforcement_decision.v0", accepted_decision()),
            (OBSERVATION_V1, observation_v1()),
            (
                FUTURE_OBSERVATION,
                with_schema(observation_v1(), FUTURE_OBSERVATION),
            ),
        ],
    );
    let envelope_type_mismatch = synthetic(
        "envelope-type-mismatch",
        &[
            ("assay.enforcement_decision.v0", accepted_decision()),
            (OBSERVATION_V0, observation_v1()),
        ],
    );
    let mixed_without_decision = synthetic(
        "mixed-without-decision",
        &[
            (OBSERVATION_V0, observation_v0()),
            (OBSERVATION_V1, observation_v1()),
        ],
    );

    let cases = [
        GenericCase {
            label: "mixed observation versions plus unrelated violations, default v0",
            bundle: mixed_without_decision.clone(),
            profile_version: None,
            finding_ids: &[
                "unknown_profile_schema",
                "decision_cardinality",
                "marker_not_backed",
            ],
            observed_schemas: &[Some(OBSERVATION_V1), None, None],
        },
        GenericCase {
            label: "mixed observation versions plus unrelated violations, explicit v1",
            bundle: mixed_without_decision,
            profile_version: Some("v1"),
            finding_ids: &[
                "unknown_profile_schema",
                "decision_cardinality",
                "marker_not_backed",
            ],
            observed_schemas: &[Some(OBSERVATION_V0), None, None],
        },
        GenericCase {
            label: "bad-104: unrecognized decision schema no interpreter ships",
            bundle: v0_vector("bad-104-unknown-schema"),
            profile_version: None,
            finding_ids: &["unknown_profile_schema"],
            observed_schemas: &[Some("assay.enforcement_decision.v1")],
        },
        GenericCase {
            label: "bad-202 under v1: mixed observation versions",
            bundle: v1_vector("bad-202-mixed-marker-versions"),
            profile_version: Some("v1"),
            finding_ids: &["unknown_profile_schema"],
            observed_schemas: &[Some(OBSERVATION_V0)],
        },
        GenericCase {
            label: "bad-202 under default v0: mixed observation versions",
            bundle: v1_vector("bad-202-mixed-marker-versions"),
            profile_version: None,
            finding_ids: &["unknown_profile_schema"],
            observed_schemas: &[Some(OBSERVATION_V1)],
        },
        GenericCase {
            label: "bad-203 under default v0: other interpreter's observation plus a cardinality violation",
            bundle: v1_vector("bad-203-v1-marker-without-decision"),
            profile_version: None,
            finding_ids: &["unknown_profile_schema", "decision_cardinality"],
            observed_schemas: &[Some(OBSERVATION_V1), None],
        },
        GenericCase {
            label: "bad-102 under v1: malformed recognized record, no unknown schema",
            bundle: v0_vector("bad-102-missing-target-digest"),
            profile_version: Some("v1"),
            finding_ids: &["target_digest_missing"],
            observed_schemas: &[None],
        },
        GenericCase {
            label: "in-namespace event type whose payload declares no schema",
            bundle: missing_payload_schema,
            profile_version: None,
            finding_ids: &["unknown_profile_schema"],
            observed_schemas: &[None],
        },
        GenericCase {
            label: "observation schema no shipped interpreter recognizes, default v0",
            bundle: future_observation.clone(),
            profile_version: None,
            finding_ids: &["unknown_profile_schema"],
            observed_schemas: &[Some(FUTURE_OBSERVATION)],
        },
        GenericCase {
            label: "observation schema no shipped interpreter recognizes, explicit v1",
            bundle: future_observation,
            profile_version: Some("v1"),
            finding_ids: &["unknown_profile_schema"],
            observed_schemas: &[Some(FUTURE_OBSERVATION)],
        },
        GenericCase {
            label: "other interpreter's observation next to an unshipped one",
            bundle: other_interpreter_plus_future,
            profile_version: None,
            finding_ids: &["unknown_profile_schema", "unknown_profile_schema"],
            observed_schemas: &[Some(OBSERVATION_V1), Some(FUTURE_OBSERVATION)],
        },
        GenericCase {
            label: "other interpreter's observation under a mismatched envelope type",
            bundle: envelope_type_mismatch,
            profile_version: None,
            finding_ids: &["event_type_schema_mismatch", "unknown_profile_schema"],
            observed_schemas: &[None, Some(OBSERVATION_V1)],
        },
    ];

    for case in cases {
        let (report, exit) = verify(&case.bundle, case.profile_version);
        assert_eq!(exit, 2, "{}: exit code", case.label);
        assert_eq!(report["bundle_integrity"], "pass", "{}", case.label);
        assert_eq!(report["verdict"], "invalid", "{}", case.label);
        assert!(report.get("claims").is_none(), "{}", case.label);
        assert_eq!(
            finding_ids(&report),
            case.finding_ids,
            "{}: findings",
            case.label
        );
        assert_eq!(
            observed_schemas(&report),
            case.observed_schemas,
            "{}: observed schemas",
            case.label
        );
        assert_eq!(
            report["reason_code"], "E_EVIDENCE_PROFILE_INVALID",
            "{}",
            case.label
        );
        assert_eq!(
            report["next_step"], PROFILE_INVALID_NEXT_STEP,
            "{}: next_step",
            case.label
        );
        let (table, table_exit) = verify_table(&case.bundle, case.profile_version);
        assert_eq!(table_exit, 2, "{}: table exit code", case.label);
        assert_eq!(
            table_next_step(&table),
            Some(PROFILE_INVALID_NEXT_STEP),
            "{}: table next step",
            case.label
        );
    }
}

/// The rule is "every violation", not "exactly one": two observations of the other interpreter
/// still get the check, and both findings stay listed. The guidance promises nothing about what
/// that other interpreter would say, which here would be a cardinality rejection.
#[test]
fn several_observations_of_the_other_interpreter_still_get_the_check_and_keep_every_finding() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bundle = dir.path().join("two-v1-observations.bundle.tar.gz");
    write_bundle(
        &bundle,
        &[
            ("assay.enforcement_decision.v0", accepted_decision()),
            (OBSERVATION_V1, observation_v1()),
            (OBSERVATION_V1, observation_v1()),
        ],
    );

    let (report, exit) = verify(&bundle, None);
    assert_eq!(exit, 2);
    assert_eq!(report["verdict"], "invalid");
    assert_eq!(
        finding_ids(&report),
        ["unknown_profile_schema", "unknown_profile_schema"]
    );
    assert_eq!(
        observed_schemas(&report),
        [Some(OBSERVATION_V1), Some(OBSERVATION_V1)]
    );
    assert_eq!(report["next_step"], INTERPRETER_CHECK_DEFAULT_NEXT_STEP);

    let (explicit_v1, explicit_v1_exit) = verify(&bundle, Some("v1"));
    assert_eq!(explicit_v1_exit, 2, "no selection accepts this bundle");
    assert_eq!(finding_ids(&explicit_v1), ["observation_cardinality"]);
    assert_eq!(explicit_v1["next_step"], PROFILE_INVALID_NEXT_STEP);
}

#[test]
fn valid_reports_carry_no_diagnosis_including_refuted_and_diagnostic_only() {
    let cases: [(PathBuf, Option<&str>, &str); 6] = [
        (
            v0_vector("ok-001-deny-bound-observation"),
            None,
            "confirmed",
        ),
        (
            v1_vector("ok-001-deny-bound-v1-observation"),
            Some("v1"),
            "confirmed",
        ),
        // Refuted cell: exit 0 and a contradiction note, never a failure diagnosis.
        (
            v1_vector("ok-004-allow-contradicted-by-v1-denial"),
            Some("v1"),
            "refuted",
        ),
        (
            v0_vector("ok-005-allow-contradicted-by-denial"),
            None,
            "refuted",
        ),
        // Diagnostic-only establish record on a valid report.
        (
            v0_vector("ok-004-allow-with-diagnostic-establish"),
            None,
            "incomplete",
        ),
        (
            v0_vector("ok-004-allow-with-diagnostic-establish"),
            Some("v1"),
            "incomplete",
        ),
    ];
    for (bundle, profile_version, caller_visible_denial) in cases {
        let label = format!("{} under {profile_version:?}", bundle.display());
        let (report, exit) = verify(&bundle, profile_version);
        assert_eq!(exit, 0, "{label}");
        assert_eq!(report["verdict"], "valid", "{label}");
        assert_eq!(
            report["claims"]["caller_visible_denial"]["status"], caller_visible_denial,
            "{label}"
        );
        assert!(report.get("reason_code").is_none(), "{label}: reason_code");
        assert!(report.get("next_step").is_none(), "{label}: next_step");
        let (table, table_exit) = verify_table(&bundle, profile_version);
        assert_eq!(table_exit, 0, "{label}");
        assert!(
            !table.contains("Next step:") && !table.contains("Reason code:"),
            "{label}: table carries a diagnosis:\n{table}"
        );
    }
    let (refuted, _) = verify(
        &v1_vector("ok-004-allow-contradicted-by-v1-denial"),
        Some("v1"),
    );
    assert_eq!(
        finding_ids(&refuted),
        ["caller_visible_outcome_contradiction"]
    );
}

/// Stage 1 runs before any record is read, so a damaged bundle keeps its own diagnosis even when
/// its records would otherwise meet the narrow case.
#[test]
fn stage1_failures_keep_their_own_diagnosis_and_precedence() {
    for profile_version in [None, Some("v1")] {
        let (report, exit) = verify(&v0_vector("bad-101-tampered-bundle"), profile_version);
        assert_eq!(exit, 2);
        assert_eq!(report["bundle_integrity"], "fail");
        assert!(report.get("verdict").is_none());
        assert_eq!(report["reason_code"], "E_EVIDENCE_INTEGRITY");
        assert_eq!(report["next_step"], INTEGRITY_NEXT_STEP);
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let truncated = dir.path().join("truncated.bundle.tar.gz");
    let bytes = std::fs::read(v1_vector("ok-001-deny-bound-v1-observation")).expect("read fixture");
    std::fs::write(&truncated, &bytes[..80]).expect("write truncated bundle");
    let (report, exit) = verify(&truncated, None);
    assert_eq!(exit, 2);
    assert_eq!(report["bundle_integrity"], "fail");
    assert!(report.get("verdict").is_none());
    assert_eq!(report["reason_code"], "E_EVIDENCE_UNREADABLE");
    assert_eq!(finding_ids(&report), ["bundle_integrity"]);
    let next_step = report["next_step"].as_str().expect("next_step");
    assert!(
        next_step.starts_with("Run argv: "),
        "unreadable keeps its caller-argv form, got {next_step}"
    );
}

// ---------------------------------------------------------------------------------------------
// Per-bundle report schema (#2497). The schema describes the report the current verifier emits.
// It is closed per the #2167 emit-validator convention (`docs/architecture/CLI-JSON-IDENTITIES.md`)
// and does not claim that output of older releases under the same `.v0` identity validates; the
// historical byte pins in `conformance_privileged_mcp_action.rs` keep that question.

/// The implementation identification this verifier reports about itself: a self-reported
/// diagnostic naming the release, not authentication of the code that ran.
fn expected_evaluator() -> Value {
    json!({"name": "assay", "version": env!("CARGO_PKG_VERSION")})
}

const REPORT_SCHEMA_PATH: &str = "schemas/privileged_mcp_action_verify_report_v0.schema.json";
const CONFORMANCE_RUN_SCHEMA_PATH: &str = "conformance/privileged-mcp-action-v0/report.schema.json";

struct LocalOnlyRetriever;

impl jsonschema::Retrieve for LocalOnlyRetriever {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err(format!("external schema retrieval is disabled: {uri}").into())
    }
}

fn read_schema(relative: &str) -> Value {
    let path = repo_root().join(relative);
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("read schema {}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn compile(schema: &Value, context: &str) -> jsonschema::Validator {
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .with_retriever(LocalOnlyRetriever)
        .build(schema)
        .unwrap_or_else(|e| panic!("{context} must compile offline: {e}"))
}

/// A validator for one `$defs` entry, resolved against the document's own `$defs`.
fn compile_def(document: &Value, def: &str, context: &str) -> jsonschema::Validator {
    compile(
        &json!({"$defs": document["$defs"].clone(), "$ref": format!("#/$defs/{def}")}),
        context,
    )
}

fn report_validator() -> jsonschema::Validator {
    compile(&read_schema(REPORT_SCHEMA_PATH), "per-bundle report schema")
}

fn schema_errors(validator: &jsonschema::Validator, instance: &Value) -> Vec<String> {
    validator
        .iter_errors(instance)
        .map(|e| e.to_string())
        .collect()
}

#[test]
fn report_schema_compiles_offline_and_names_the_report_identity() {
    let schema = read_schema(REPORT_SCHEMA_PATH);
    report_validator();
    assert_eq!(
        schema["properties"]["schema"]["const"], REPORT_SCHEMA,
        "the schema must describe the current `.v0` report identity"
    );
}

struct ReportRow {
    label: &'static str,
    bundle: PathBuf,
    selection: Option<&'static str>,
    profile: &'static str,
    profile_selection: &'static str,
    bundle_integrity: &'static str,
    verdict: Option<&'static str>,
}

/// One row per emitted branch and selection that matters, not a corpus x selection product:
/// corpus outcomes are compared elsewhere; this table checks the shape of what is printed.
#[test]
fn every_emitted_report_branch_validates_against_the_schema() {
    let validator = report_validator();
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("missing.bundle.tar.gz");
    let rows = [
        ReportRow {
            label: "valid, default selection",
            bundle: v0_vector("ok-001-deny-bound-observation"),
            selection: None,
            profile: PROFILE_V0,
            profile_selection: "default",
            bundle_integrity: "pass",
            verdict: Some("valid"),
        },
        ReportRow {
            label: "valid with a refuted cell, explicit v0",
            bundle: v0_vector("ok-005-allow-contradicted-by-denial"),
            selection: Some("v0"),
            profile: PROFILE_V0,
            profile_selection: "explicit",
            bundle_integrity: "pass",
            verdict: Some("valid"),
        },
        ReportRow {
            label: "valid under v1",
            bundle: v1_vector("ok-001-deny-bound-v1-observation"),
            selection: Some("v1"),
            profile: PROFILE_V1,
            profile_selection: "explicit",
            bundle_integrity: "pass",
            verdict: Some("valid"),
        },
        ReportRow {
            label: "invalid, reissue guidance",
            bundle: v0_vector("bad-103-two-decisions"),
            selection: None,
            profile: PROFILE_V0,
            profile_selection: "default",
            bundle_integrity: "pass",
            verdict: Some("invalid"),
        },
        ReportRow {
            label: "invalid, observed_schema and interpreter check",
            bundle: v1_vector("ok-001-deny-bound-v1-observation"),
            selection: None,
            profile: PROFILE_V0,
            profile_selection: "default",
            bundle_integrity: "pass",
            verdict: Some("invalid"),
        },
        ReportRow {
            label: "stage-1 integrity failure, explicit v1",
            bundle: v0_vector("bad-101-tampered-bundle"),
            selection: Some("v1"),
            profile: PROFILE_V1,
            profile_selection: "explicit",
            bundle_integrity: "fail",
            verdict: None,
        },
        ReportRow {
            label: "stage-1 unreadable path",
            bundle: missing,
            selection: None,
            profile: PROFILE_V0,
            profile_selection: "default",
            bundle_integrity: "fail",
            verdict: None,
        },
    ];
    for row in rows {
        let ReportRow {
            label,
            bundle,
            selection,
            profile,
            profile_selection,
            bundle_integrity,
            verdict,
        } = row;
        let (report, exit) = verify(&bundle, selection);
        // Pin the branch so a row cannot silently turn into a different case.
        assert_eq!(report["profile"], profile, "{label}");
        assert_eq!(report["profile_selection"], profile_selection, "{label}");
        assert_eq!(report["bundle_integrity"], bundle_integrity, "{label}");
        assert_eq!(report["verdict"].as_str(), verdict, "{label}");
        let expected_exit = if bundle_integrity == "pass" && verdict == Some("valid") {
            0
        } else {
            2
        };
        assert_eq!(exit, expected_exit, "{label}: exit rule");
        assert_eq!(
            report["evaluator"],
            expected_evaluator(),
            "{label}: evaluator"
        );
        let errors = schema_errors(&validator, &report);
        assert!(
            errors.is_empty(),
            "{label}: emitted report rejected: {errors:?}\n{report:#}"
        );
    }
}

fn emitted(bundle: &Path, selection: Option<&str>) -> Value {
    verify(bundle, selection).0
}

/// Each negative starts from an emitted report that validates, changes one thing, and must be
/// rejected. The base is checked first, so a rejection is caused by the change.
#[test]
fn report_schema_rejects_shapes_the_verifier_does_not_emit() {
    let validator = report_validator();
    let valid = emitted(&v0_vector("ok-001-deny-bound-observation"), None);
    let invalid = emitted(&v0_vector("bad-103-two-decisions"), None);
    let failed = emitted(&v0_vector("bad-101-tampered-bundle"), None);
    for (label, base) in [("valid", &valid), ("invalid", &invalid), ("fail", &failed)] {
        let errors = schema_errors(&validator, base);
        assert!(
            errors.is_empty(),
            "control {label} must validate: {errors:?}"
        );
    }
    for (label, base) in [("valid", &valid), ("invalid", &invalid), ("fail", &failed)] {
        assert_eq!(
            base["evaluator"],
            expected_evaluator(),
            "control {label} carries the evaluator the mutations below change"
        );
    }
    assert_eq!(
        valid["claims"]["upstream_delivery"]["status"], "incomplete",
        "control: the valid base carries an incomplete cell"
    );
    assert_eq!(
        valid["claims"]["policy_decision_recorded"]["status"], "confirmed",
        "control: the valid base carries a confirmed cell"
    );

    type Mutation = fn(&mut Value);
    let cases: [(&str, &Value, Mutation); 14] = [
        ("missing required non_claims", &valid, |r| {
            r.as_object_mut().unwrap().remove("non_claims");
        }),
        ("input_profile absent instead of null", &valid, |r| {
            r.as_object_mut().unwrap().remove("input_profile");
        }),
        ("unknown top-level output member", &valid, |r| {
            r["unrelated_member"] = json!({"name": "assay"});
        }),
        ("evaluator absent", &valid, |r| {
            r.as_object_mut().unwrap().remove("evaluator");
        }),
        ("evaluator without version", &invalid, |r| {
            r["evaluator"].as_object_mut().unwrap().remove("version");
        }),
        ("evaluator version not a string", &failed, |r| {
            r["evaluator"]["version"] = json!(1);
        }),
        ("evaluator with an empty name", &valid, |r| {
            r["evaluator"]["name"] = json!("");
        }),
        ("evaluator with an extra member", &valid, |r| {
            r["evaluator"]["commit"] = json!("0000000000000000000000000000000000000000");
        }),
        ("unknown member on a finding", &invalid, |r| {
            r["findings"][0]["severity"] = json!("high");
        }),
        ("claims on an invalid report", &invalid, |r| {
            r["claims"] = json!({
                "policy_decision_recorded": {"status": "incomplete"},
                "caller_visible_denial": {"status": "incomplete"},
                "upstream_delivery": {"status": "incomplete"},
                "external_side_effect": {"status": "incomplete"},
            });
        }),
        ("verdict on an integrity failure", &failed, |r| {
            r["verdict"] = json!("invalid");
        }),
        ("source_class on an incomplete cell", &valid, |r| {
            r["claims"]["upstream_delivery"]["source_class"] = json!("producer_reported");
        }),
        (
            "issuer_attested, which this verifier never emits",
            &valid,
            |r| {
                r["claims"]["policy_decision_recorded"]["source_class"] = json!("issuer_attested");
            },
        ),
        ("reason_code on a valid report", &valid, |r| {
            r["reason_code"] = json!("E_EVIDENCE_PROFILE_INVALID");
        }),
    ];
    for (label, base, mutate) in cases {
        let mut report = base.clone();
        mutate(&mut report);
        assert!(
            !validator.is_valid(&report),
            "schema must reject: {label}\n{report:#}"
        );
    }

    // The schema is language-neutral: another implementation names itself, and only Assay's own
    // output is pinned to "assay" and its package version.
    let mut other = valid.clone();
    other["evaluator"] = json!({"name": "independent-verifier", "version": "2026.10-rc.1"});
    let errors = schema_errors(&validator, &other);
    assert!(
        errors.is_empty(),
        "another implementation's evaluator must validate: {errors:?}"
    );
}

/// The table prints the evaluator from the same report object the JSON carries.
#[test]
fn table_prints_the_self_reported_evaluator_from_the_report() {
    for (label, bundle) in [
        ("valid", v0_vector("ok-001-deny-bound-observation")),
        ("invalid", v0_vector("bad-103-two-decisions")),
        ("integrity failure", v0_vector("bad-101-tampered-bundle")),
    ] {
        let (report, json_exit) = verify(&bundle, None);
        let (table, table_exit) = verify_table(&bundle, None);
        assert_eq!(table_exit, json_exit, "{label}: exit is format-independent");
        assert_eq!(report["evaluator"], expected_evaluator(), "{label}");
        let line = format!(
            "Evaluator (self-reported): {} {}",
            report["evaluator"]["name"]
                .as_str()
                .expect("evaluator name"),
            report["evaluator"]["version"]
                .as_str()
                .expect("evaluator version"),
        );
        let matching = table.lines().filter(|l| l.trim_end() == line).count();
        assert_eq!(
            matching, 1,
            "{label}: table must print {line:?} once:\n{table}"
        );
    }
}

/// Every `source_class` value the conformance-run schema admits, read from that schema.
fn conformance_source_classes(conformance: &Value) -> Vec<String> {
    let mut found = Vec::new();
    fn walk(node: &Value, found: &mut Vec<String>) {
        match node {
            Value::Object(map) => {
                if let Some(values) = map
                    .get("properties")
                    .and_then(|p| p.get("source_class"))
                    .and_then(|s| s.get("enum"))
                    .and_then(Value::as_array)
                {
                    found.extend(values.iter().filter_map(Value::as_str).map(str::to_string));
                }
                map.values().for_each(|v| walk(v, found));
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, found)),
            _ => {}
        }
    }
    walk(&conformance["$defs"]["claimResult"], &mut found);
    assert!(
        !found.is_empty(),
        "conformance claimResult must name its source classes"
    );
    found
}

/// The per-bundle schema and the conformance-run schema share the claim-cell and claim-matrix
/// structure; they deliberately differ on `source_class`. Parity is behavioural: the same
/// instances go through both, shared cases must agree, and the differences are listed.
#[test]
fn claim_structure_agrees_with_the_conformance_run_schema_except_the_source_class_vocabulary() {
    let report = read_schema(REPORT_SCHEMA_PATH);
    let conformance = read_schema(CONFORMANCE_RUN_SCHEMA_PATH);
    let report_cell = compile_def(&report, "claimCell", "report claimCell");
    let conformance_cell = compile_def(&conformance, "claimResult", "conformance claimResult");
    let report_matrix = compile_def(&report, "claimMatrix", "report claimMatrix");
    let conformance_matrix = compile_def(&conformance, "claimMatrix", "conformance claimMatrix");

    let shared_cells: [(Value, bool); 8] = [
        (json!({"status": "incomplete"}), true),
        (
            json!({"status": "confirmed", "source_class": "producer_reported"}),
            true,
        ),
        (
            json!({"status": "refuted", "source_class": "producer_reported"}),
            true,
        ),
        (
            json!({"status": "incomplete", "source_class": "producer_reported"}),
            false,
        ),
        (json!({"status": "confirmed"}), false),
        (json!({"status": "refuted"}), false),
        (
            json!({"status": "unknown", "source_class": "producer_reported"}),
            false,
        ),
        (
            json!({"status": "confirmed", "source_class": "producer_reported", "x": 1}),
            false,
        ),
    ];
    for (cell, accepted) in &shared_cells {
        assert_eq!(report_cell.is_valid(cell), *accepted, "report cell {cell}");
        assert_eq!(
            conformance_cell.is_valid(cell),
            *accepted,
            "conformance cell {cell}"
        );
    }

    // The explicit difference: the general vocabulary admits classes this verifier never emits.
    let classes = conformance_source_classes(&conformance);
    assert!(classes.iter().any(|c| c == "producer_reported"));
    let others: Vec<&String> = classes
        .iter()
        .filter(|c| *c != "producer_reported")
        .collect();
    assert!(
        !others.is_empty(),
        "the general vocabulary is wider than this report"
    );
    for class in others {
        for status in ["confirmed", "refuted"] {
            let cell = json!({"status": status, "source_class": class});
            assert!(
                conformance_cell.is_valid(&cell),
                "conformance admits {cell}"
            );
            assert!(!report_cell.is_valid(&cell), "report must reject {cell}");
        }
    }

    let cell = json!({"status": "incomplete"});
    let all_four = json!({
        "policy_decision_recorded": cell,
        "caller_visible_denial": cell,
        "upstream_delivery": cell,
        "external_side_effect": cell,
    });
    let mut missing_one = all_four.clone();
    missing_one
        .as_object_mut()
        .unwrap()
        .remove("external_side_effect");
    let mut extra_one = all_four.clone();
    extra_one["provider_outcome"] = cell.clone();
    for (matrix, accepted) in [
        (&all_four, true),
        (&missing_one, false),
        (&extra_one, false),
    ] {
        assert_eq!(
            report_matrix.is_valid(matrix),
            accepted,
            "report matrix {matrix}"
        );
        assert_eq!(
            conformance_matrix.is_valid(matrix),
            accepted,
            "conformance matrix {matrix}"
        );
    }
}

/// The registry strings from `evidence_verify_reason.rs`, extracted the way
/// `agent_golden_path_contract.rs` does it, so no test keeps a hand-written list.
fn rust_owned_profile_evidence_reason_codes() -> Vec<&'static str> {
    let src = include_str!("../src/evidence_verify_reason.rs");
    let start = src
        .find("const PROFILE_EVIDENCE_REASON_CODES")
        .expect("PROFILE_EVIDENCE_REASON_CODES must exist in evidence_verify_reason.rs");
    let block = src[start..]
        .split("];")
        .next()
        .expect("PROFILE_EVIDENCE_REASON_CODES must terminate");
    let mut codes = Vec::new();
    let mut rest = block;
    while let Some(offset) = rest.find("\"E_EVIDENCE_") {
        let quoted = &rest[offset + 1..];
        let end = quoted.find('"').expect("reason code must be quoted");
        codes.push(&quoted[..end]);
        rest = &quoted[end + 1..];
    }
    assert!(
        !codes.is_empty(),
        "failed to extract PROFILE_EVIDENCE_REASON_CODES"
    );
    codes
}

#[test]
fn report_schema_reason_codes_equal_the_binary_registry() {
    let schema = read_schema(REPORT_SCHEMA_PATH);
    let mut in_schema: Vec<&str> = schema["$defs"]["reasonCode"]["enum"]
        .as_array()
        .expect("$defs.reasonCode.enum")
        .iter()
        .map(|v| v.as_str().expect("reason code string"))
        .collect();
    let mut owned = rust_owned_profile_evidence_reason_codes();
    in_schema.sort_unstable();
    owned.sort_unstable();
    assert_eq!(
        in_schema, owned,
        "schema reason codes must equal the Rust registry"
    );
}
