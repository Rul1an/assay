use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
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

#[derive(Debug, Deserialize)]
struct ExpectedCase {
    directory: String,
    #[serde(flatten)]
    outcome: ExpectedOutcome,
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

const NORMATIVE_DIR: &str = "normative/";
const LOCAL_DIR: &str = "local/";
const GENERATED_OVERSIZE_CASE: &str = "generated:oversize";
const EXPECTED_FIXTURE_NAME: &str = "expected-first-failure.json";

#[test]
fn cap1_normative_parity_fixture_is_enforced_for_the_leaf_validator() {
    let expected: BTreeMap<String, ExpectedCase> = serde_json::from_slice(
        &fs::read(expected_fixture_path()).expect("expected fixture readable"),
    )
    .expect("expected fixture parses");
    assert_expected_fixture_covers_all_json_vectors(&expected);

    for (case, expected_case) in expected {
        let bytes = fixture_case_bytes(&case, &expected_case.directory);
        let actual = verify_with_leaf(&bytes);
        assert_eq!(
            actual,
            expected_to_actual(expected_case.outcome),
            "{}/{}",
            expected_case.directory,
            case
        );
    }
}

#[test]
fn leaf_refusal_display_is_value_free() {
    let err = verify_cap1_document(
        &fs::read(normative_dir().join("NC-02.json")).expect("NC-02"),
        &Cap1AdmissionLimits::default(),
    )
    .expect_err("NC-02 refuses");
    assert!(!err.to_string().contains("other"), "{err}");

    let err = verify_cap1_document(
        &fs::read(normative_dir().join("NC-06.json")).expect("NC-06"),
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

fn fixture_case_bytes(case: &str, directory: &str) -> Vec<u8> {
    if case == GENERATED_OVERSIZE_CASE {
        assert_eq!(
            directory, LOCAL_DIR,
            "{GENERATED_OVERSIZE_CASE} must be scoped under {LOCAL_DIR}"
        );
        return generated_oversize_case();
    }
    fs::read(fixture_subdir(directory).join(case)).expect("fixture case readable")
}

fn generated_oversize_case() -> Vec<u8> {
    let mut bytes = fs::read(normative_dir().join("PV-01.json")).expect("PV-01 readable");
    bytes.resize(Cap1AdmissionLimits::HARD_MAX_BYTES + 1, b' ');
    bytes
}

fn assert_expected_fixture_covers_all_json_vectors(expected: &BTreeMap<String, ExpectedCase>) {
    assert_directory_inventory(expected, NORMATIVE_DIR);
    assert_directory_inventory(expected, LOCAL_DIR);
    for (case, expected_case) in expected {
        if case == GENERATED_OVERSIZE_CASE {
            assert_eq!(
                expected_case.directory, LOCAL_DIR,
                "{GENERATED_OVERSIZE_CASE} must be in {LOCAL_DIR}"
            );
            continue;
        }
        let path = fixture_subdir(&expected_case.directory).join(case);
        assert!(
            path.is_file(),
            "expected entry {}/{} has no file",
            expected_case.directory,
            case
        );
    }
}

fn assert_directory_inventory(expected: &BTreeMap<String, ExpectedCase>, directory: &str) {
    let files = fixture_json_names(&fixture_subdir(directory));
    for file in files {
        let entry = expected
            .get(&file)
            .unwrap_or_else(|| panic!("{directory}{file} is missing from expected fixture"));
        assert_eq!(
            entry.directory, directory,
            "expected entry {file} must name {directory}"
        );
    }
}

fn fixture_json_names(directory: &Path) -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    for entry in fs::read_dir(directory).expect("fixture directory readable") {
        let entry = entry.expect("fixture entry readable");
        if !entry
            .file_type()
            .expect("fixture entry metadata readable")
            .is_file()
        {
            continue;
        }
        let path = entry.path();
        if path.extension() == Some(OsStr::new("json")) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name != EXPECTED_FIXTURE_NAME {
                files.insert(name);
            }
        }
    }
    files
}

fn fixture_subdir(directory: &str) -> PathBuf {
    match directory {
        NORMATIVE_DIR => normative_dir(),
        LOCAL_DIR => local_dir(),
        other => panic!("unknown fixture directory {other:?}"),
    }
}

fn shared_cap1_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../assay-evidence/tests/fixtures/cap1")
}

fn normative_dir() -> PathBuf {
    shared_cap1_dir().join("normative")
}

fn local_dir() -> PathBuf {
    shared_cap1_dir().join("local")
}

fn expected_fixture_path() -> PathBuf {
    normative_dir().join(EXPECTED_FIXTURE_NAME)
}
