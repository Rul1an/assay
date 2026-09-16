use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use gateway_evidence_replay::{verify_cap1_document, Cap1AdmissionLimits, Cap1Refusal, Cap1Stage};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum ExpectedOutcome {
    Pass,
    Refusal {
        stage: String,
        #[serde(default)]
        rule: Option<String>,
        #[serde(default)]
        schema_ground: Option<String>,
        #[serde(default)]
        admission_ground: Option<String>,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum ActualOutcome {
    Pass,
    Refusal {
        stage: String,
        rule: Option<String>,
        schema_ground: Option<String>,
        admission_ground: Option<String>,
    },
}

#[test]
fn cap1_normative_parity_fixture_is_enforced_for_the_leaf_validator() {
    let expected: BTreeMap<String, ExpectedOutcome> = serde_json::from_slice(
        &fs::read(expected_fixture_path()).expect("expected fixture readable"),
    )
    .expect("expected fixture parses");

    for (vector_file, expected_outcome) in expected {
        let bytes = fs::read(vectors_dir().join(&vector_file)).expect("vector readable");
        let actual = verify_with_leaf(&bytes);
        assert_eq!(
            actual,
            expected_to_actual(expected_outcome),
            "{vector_file}"
        );
    }
}

#[test]
fn leaf_refusal_display_is_value_free() {
    let err = verify_cap1_document(
        &fs::read(vectors_dir().join("NC-02.json")).expect("NC-02"),
        &Cap1AdmissionLimits::default(),
    )
    .expect_err("NC-02 refuses");
    assert!(!err.to_string().contains("other"), "{err}");

    let err = verify_cap1_document(
        &fs::read(vectors_dir().join("NC-06.json")).expect("NC-06"),
        &Cap1AdmissionLimits::default(),
    )
    .expect_err("NC-06 refuses");
    assert!(!err.to_string().contains("does-not-exist"), "{err}");
}

fn expected_to_actual(expected: ExpectedOutcome) -> ActualOutcome {
    match expected {
        ExpectedOutcome::Pass => ActualOutcome::Pass,
        ExpectedOutcome::Refusal {
            stage,
            rule,
            schema_ground,
            admission_ground,
        } => ActualOutcome::Refusal {
            stage,
            rule,
            schema_ground,
            admission_ground,
        },
    }
}

fn verify_with_leaf(bytes: &[u8]) -> ActualOutcome {
    match verify_cap1_document(bytes, &Cap1AdmissionLimits::default()) {
        Ok(_) => ActualOutcome::Pass,
        Err(err) => refusal_to_actual(&err),
    }
}

fn refusal_to_actual(refusal: &Cap1Refusal) -> ActualOutcome {
    let stage = match refusal.stage() {
        Cap1Stage::Admission => "admission",
        Cap1Stage::Schema => "schema",
        Cap1Stage::Rules => "rules",
    }
    .to_string();
    ActualOutcome::Refusal {
        stage,
        rule: refusal.rule().map(|rule| rule.as_str().to_string()),
        schema_ground: refusal.instance_path().map(str::to_string),
        admission_ground: refusal.admission_ground().map(str::to_string),
    }
}

fn shared_cap1_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../assay-evidence/tests/fixtures/cap1/normative")
}

fn vectors_dir() -> PathBuf {
    shared_cap1_dir()
}

fn expected_fixture_path() -> PathBuf {
    shared_cap1_dir().join("expected-first-failure.json")
}
