//! CLI qualification of source assertions and carrier/row binding.
//! Synthetic carriers exercise import -> verification -> projection. No source identity,
//! runtime execution or native AAC compatibility follows from a successful comparison.
use assay_core::mcp::policy::McpPolicy;
use assay_core::mcp::tool_decision_truth::{self as tdt, DecisionEvidence};
use assay_evidence::bundle::{BundleReader, BundleWriter};
use assert_cmd::Command;
use serde_json::{json, Value};
use std::{fs, path::Path};

fn carrier(source: &str) -> Value {
    let policy: McpPolicy = serde_json::from_value(json!({
        "version": "1", "tools": {"allow": ["deploy"]},
        "schemas": {"deploy": {"type": "object"}},
        "enforcement": {"unconstrained_tools": "warn"}
    }))
    .unwrap();
    // The builder supplies admissible input only. Expected claim levels below are literals.
    tdt::build_classified_record(
        &policy,
        "deploy",
        &json!({}),
        0,
        b"qualification-test-key",
        "fixture",
        source,
        "c0",
        "ok",
        "present",
        &DecisionEvidence::default(),
    )
    .unwrap()
}

fn import(input: &Path, bundle: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "import", "tool-decision-truth", "--carrier"])
        .arg(input)
        .arg("--bundle-out")
        .arg(bundle)
        .args(["--import-time", "2026-01-01T00:00:00Z"])
        .assert()
}

fn verify(bundle: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "verify-tool-decision-truth"])
        .arg(bundle)
        .args(["--format", "json"])
        .assert()
}

#[test]
fn accepted_source_classes_remain_assertions_through_the_cli() {
    // Break caught: dropping or upgrading the basis for any admitted source class.
    for source in ["authoritative_boundary", "reported_trace", "inferred"] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("carrier.json");
        let bundle = dir.path().join("bundle.tar.gz");
        fs::write(&input, serde_json::to_vec(&carrier(source)).unwrap()).unwrap();
        import(&input, &bundle).success();
        let v = verify(&bundle).success();
        let report: Value = serde_json::from_slice(&v.get_output().stdout).unwrap();
        assert_eq!(report["verified_rows"], 1);
        assert_eq!(
            report["claims_not_made"],
            json!([
                "policy_correctness",
                "intent_or_maliciousness",
                "runtime_enforcement",
                "tool_result_truth"
            ])
        );
        let out = Command::cargo_bin("assay")
            .unwrap()
            .args(["project-otel", "--evidence-bundle"])
            .arg(&bundle)
            .assert()
            .success();
        let projection: Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
        assert_eq!(projection["lossy"], true);
        let spans = projection["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 1);
        let attrs = &spans[0]["attributes"];
        assert_eq!(attrs["assay.tdt.source_class"], source);
        assert_eq!(attrs["assay.tdt.source_class_basis"], "asserted");
        assert_eq!(attrs["assay.claim_class"], "derived");
        assert_eq!(attrs["assay.tdt.run_verdict"], "match");
    }
}

#[test]
fn absent_and_foreign_source_labels_are_not_coerced_at_import() {
    // Break caught: accepting a missing/unknown source or translating foreign grades implicitly.
    for source in [
        None,
        Some(Value::Null),
        Some(json!("computed")),
        Some(json!("attested")),
        Some(json!("unknown")),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("carrier.json");
        let bundle = dir.path().join("bundle.tar.gz");
        let mut value = carrier("reported_trace");
        match source {
            None => {
                value.as_object_mut().unwrap().remove("source_class");
            }
            Some(source) => {
                value["source_class"] = source;
            }
        }
        fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
        let result = import(&input, &bundle).failure();
        let error = String::from_utf8_lossy(&result.get_output().stderr);
        assert!(error.contains("source_class"), "wrong refusal: {error}");
        assert!(!bundle.exists(), "refused source must not publish a bundle");
    }
}

#[test]
fn changed_carrier_bytes_cannot_reuse_a_rows_prior_binding_or_output() {
    // Break caught: bypassing semantic verification because container integrity still passes.
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("carrier.json");
    let original = dir.path().join("original.tar.gz");
    fs::write(
        &input,
        serde_json::to_vec(&carrier("reported_trace")).unwrap(),
    )
    .unwrap();
    import(&input, &original).success();
    verify(&original).success();
    let reader = BundleReader::open(fs::File::open(&original).unwrap()).unwrap();
    let mut events = reader.events_vec().unwrap();
    let c = events
        .iter_mut()
        .find(|e| e.type_ == "assay.tool_decision_truth.v0")
        .unwrap();
    c.payload["source_class"] = json!("authoritative_boundary");
    let changed = dir.path().join("changed.tar.gz");
    let mut writer = BundleWriter::new(fs::File::create(&changed).unwrap());
    for event in events {
        writer.add_event(event);
    }
    writer.finish().unwrap();
    // A newly sealed container is valid; its old recipe row still cites the original carrier.
    BundleReader::open(fs::File::open(&changed).unwrap()).unwrap();
    let v = verify(&changed).failure().code(2);
    let report: Value = serde_json::from_slice(&v.get_output().stdout).unwrap();
    assert_eq!(report["ok"], false);
    assert_eq!(report["verified_rows"], 0);
    for existing in [false, true] {
        let out = dir.path().join(if existing {
            "sentinel.json"
        } else {
            "fresh.json"
        });
        if existing {
            fs::write(&out, b"prior-output-sentinel").unwrap();
        }
        let result = Command::cargo_bin("assay")
            .unwrap()
            .args(["project-otel", "--evidence-bundle"])
            .arg(&changed)
            .arg("--out")
            .arg(&out)
            .assert()
            .failure()
            .code(2);
        assert!(result.get_output().stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.get_output().stderr)
            .contains("tool-decision-truth verification failed"));
        if existing {
            assert_eq!(fs::read(&out).unwrap(), b"prior-output-sentinel");
        } else {
            assert!(!out.exists());
        }
    }
}
