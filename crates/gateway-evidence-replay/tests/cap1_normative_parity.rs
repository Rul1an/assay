use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use gateway_evidence_replay::{
    verify_cap1_document, verify_cap1_rules_with, Cap1AdmissionLimits, Cap1Document,
    Cap1NormativeRule, Cap1Refusal, Cap1Stage, Cap1SyntaxFault,
};
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
const GENERATED_AT_LIMIT_CASE: &str = "generated:at-limit";
const GENERATED_NOT_UTF8_CASE: &str = "generated:not-utf8";
const GENERATED_MALFORMED_CASE: &str = "generated:malformed-json";
const GENERATED_DEPTH_64_CASE: &str = "generated:depth-64";
const GENERATED_DEPTH_65_CASE: &str = "generated:depth-65";
const GENERATED_ORDER_DEPTH_BEFORE_DUPLICATE_CASE: &str = "generated:order-depth-before-duplicate";
const GENERATED_KEYS_10000_CASE: &str = "generated:keys-10000";
const GENERATED_KEYS_10001_CASE: &str = "generated:keys-10001";
const GENERATED_LONE_SURROGATE_CASE: &str = "generated:lone-surrogate";
const GENERATED_BAD_ESCAPE_CASE: &str = "generated:bad-escape";
const PINNED_HARD_MAX_BYTES: usize = 1_048_576;
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

    let duplicate_sensitive_key = br#"{
      "profile": "cap/1",
      "subject": {"kind": "artefact", "ref": "s"},
      "strata": [{
        "id": "a",
        "population": "p",
        "basis": {"kind": "declared"},
        "eligible": 0,
        "examined": 0,
        "secret-key-name": 1,
        "secret-key-name": 2,
        "unexamined": []
      }],
      "integrity": {"complete": true, "statement": "s"}
    }"#;
    let err = verify_cap1_document(duplicate_sensitive_key, &Cap1AdmissionLimits::default())
        .expect_err("duplicate key refuses");
    assert!(!err.to_string().contains("secret-key-name"), "{err}");
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
    if is_generated_case(case) {
        assert_eq!(
            directory, LOCAL_DIR,
            "{case} must be scoped under {LOCAL_DIR}"
        );
    }
    match case {
        GENERATED_OVERSIZE_CASE => generated_oversize_case(),
        GENERATED_AT_LIMIT_CASE => generated_at_limit_case(),
        GENERATED_NOT_UTF8_CASE => generated_not_utf8_case(),
        GENERATED_MALFORMED_CASE => generated_malformed_json_case(),
        GENERATED_DEPTH_64_CASE => generated_depth_64_case(),
        GENERATED_DEPTH_65_CASE => generated_depth_65_case(),
        GENERATED_ORDER_DEPTH_BEFORE_DUPLICATE_CASE => {
            generated_order_depth_before_duplicate_case()
        }
        GENERATED_KEYS_10000_CASE => generated_keys_10000_case(),
        GENERATED_KEYS_10001_CASE => generated_keys_10001_case(),
        GENERATED_LONE_SURROGATE_CASE => generated_lone_surrogate_case(),
        GENERATED_BAD_ESCAPE_CASE => generated_bad_escape_case(),
        _ => fs::read(fixture_subdir(directory).join(case)).expect("fixture case readable"),
    }
}

fn generated_oversize_case() -> Vec<u8> {
    let mut bytes = fs::read(normative_dir().join("PV-01.json")).expect("PV-01 readable");
    bytes.resize(PINNED_HARD_MAX_BYTES + 1, b' ');
    bytes
}

fn generated_at_limit_case() -> Vec<u8> {
    let mut bytes = fs::read(normative_dir().join("PV-01.json")).expect("PV-01 readable");
    bytes.resize(PINNED_HARD_MAX_BYTES, b' ');
    bytes
}

fn generated_not_utf8_case() -> Vec<u8> {
    vec![0xff, b'{', b'}']
}

fn generated_malformed_json_case() -> Vec<u8> {
    b"{\"profile\": ".to_vec()
}

fn generated_nested_scalar_case(depth: usize) -> Vec<u8> {
    format!("{}0{}", "[".repeat(depth), "]".repeat(depth)).into_bytes()
}

fn generated_nested_duplicate_key_case(depth: usize) -> Vec<u8> {
    format!(
        "{}{{\"k\":1,\"k\":2}}{}",
        "[".repeat(depth),
        "]".repeat(depth)
    )
    .into_bytes()
}

fn generated_depth_64_case() -> Vec<u8> {
    generated_nested_scalar_case(64)
}

fn generated_depth_65_case() -> Vec<u8> {
    generated_nested_scalar_case(65)
}

fn generated_order_depth_before_duplicate_case() -> Vec<u8> {
    generated_nested_duplicate_key_case(64)
}

fn generated_keys_10000_case() -> Vec<u8> {
    generated_object_with_key_count(10_000)
}

fn generated_keys_10001_case() -> Vec<u8> {
    generated_object_with_key_count(10_001)
}

fn generated_object_with_key_count(count: usize) -> Vec<u8> {
    assert!(count >= 4, "count must include required CAP-1 root keys");

    let mut json = String::with_capacity(count * 12 + 256);
    json.push_str("{\"profile\":\"cap/1\",");
    json.push_str("\"subject\":{\"kind\":\"artefact\",\"ref\":\"s\"},");
    json.push_str("\"strata\":[{\"id\":\"a\",\"population\":\"p\",");
    json.push_str(
        "\"basis\":{\"kind\":\"declared\"},\"eligible\":0,\"examined\":0,\"unexamined\":[]}],",
    );
    json.push_str("\"integrity\":{\"complete\":true,\"statement\":\"s\"}");

    for i in 0..(count - 4) {
        json.push(',');
        json.push('"');
        json.push('k');
        json.push_str(&i.to_string());
        json.push_str("\":0");
    }
    json.push('}');
    json.into_bytes()
}

fn generated_lone_surrogate_case() -> Vec<u8> {
    b"{\"x\":\"\\uD800\"}".to_vec()
}

fn generated_bad_escape_case() -> Vec<u8> {
    b"{\"x\":\"\\u12G4\"}".to_vec()
}

fn is_generated_case(case: &str) -> bool {
    matches!(
        case,
        GENERATED_OVERSIZE_CASE
            | GENERATED_AT_LIMIT_CASE
            | GENERATED_NOT_UTF8_CASE
            | GENERATED_MALFORMED_CASE
            | GENERATED_DEPTH_64_CASE
            | GENERATED_DEPTH_65_CASE
            | GENERATED_ORDER_DEPTH_BEFORE_DUPLICATE_CASE
            | GENERATED_KEYS_10000_CASE
            | GENERATED_KEYS_10001_CASE
            | GENERATED_LONE_SURROGATE_CASE
            | GENERATED_BAD_ESCAPE_CASE
    )
}

fn assert_expected_fixture_covers_all_json_vectors(expected: &BTreeMap<String, ExpectedCase>) {
    assert_directory_inventory(expected, NORMATIVE_DIR);
    assert_directory_inventory(expected, LOCAL_DIR);
    for (case, expected_case) in expected {
        if is_generated_case(case) {
            assert_eq!(
                expected_case.directory, LOCAL_DIR,
                "{case} must be in {LOCAL_DIR}"
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

#[test]
fn duplicate_profile_keys_refuse_when_valid_value_is_first() {
    assert_duplicate_key_case("LC-13.json");
}

#[test]
fn duplicate_profile_keys_refuse_when_valid_value_is_last() {
    assert_duplicate_key_case("LC-14.json");
}

fn assert_duplicate_key_case(name: &str) {
    let bytes = fs::read(local_dir().join(name)).expect("duplicate-key fixture");
    let refusal = verify_cap1_document(&bytes, &Cap1AdmissionLimits::default())
        .expect_err("duplicate keys must refuse at admission");
    assert_eq!(
        refusal.admission_ground(),
        Some("duplicate-key"),
        "{name} refused as {refusal:?}"
    );
}

#[test]
fn local_r5_case_is_reachable_once_r1_is_silenced() {
    use Cap1NormativeRule as R;

    let bytes = fs::read(local_dir().join("LC-15.json")).expect("LC-15");
    let err =
        verify_cap1_document(&bytes, &Cap1AdmissionLimits::default()).expect_err("R1 shadows R5");
    match err {
        Cap1Refusal::Rule { rule, .. } => assert_eq!(rule, R::R1NoSilentRemainder),
        other => panic!("expected rules refusal, got {other:?}"),
    }

    let doc: Cap1Document = serde_json::from_slice(&bytes).expect("typed local case parses");
    let err = verify_cap1_rules_with(&doc, &[R::R1NoSilentRemainder]).expect_err("R5");
    match err {
        Cap1Refusal::Rule { rule, .. } => assert_eq!(rule, R::R5CountsWellFormed),
        other => panic!("expected rules refusal, got {other:?}"),
    }
}

/// Duplicate member names, the shapes of Konrad Gruszka's probes
/// (Certisyn-Inc/certisyn-drafts#11). CAP-1's reference readers keep the last value of a
/// repeated name, so the same bytes read differently under a first-wins parser, and three of
/// his five probes conform upstream. Each case is rebuilt here from a pinned vector, plus an
/// escaped spelling of `profile` that is not one of his. Every case must refuse at admission.
/// The control on each mutant is its last-wins reading: where that reading conforms, nothing
/// after admission would have refused the bytes, so the refusal is the duplicate check alone.
const DUPLICATE_NAME_CASES: &[(&str, &str, &str, &str, bool)] = &[
    (
        "same value twice",
        "PV-01",
        "\"eligible\": 6,",
        "\"eligible\": 6, \"eligible\": 6,",
        true,
    ),
    (
        "escaped spelling, value breaks R1",
        "PV-01",
        "\"eligible\": 6,",
        "\"eligible\": 6, \"\\u0065ligible\": 13,",
        false,
    ),
    (
        "top-level profile, cap/0 then cap/1",
        "PV-01",
        "\"profile\": \"cap/1\",",
        "\"profile\": \"cap/0\", \"profile\": \"cap/1\",",
        true,
    ),
    (
        "escaped profile spelling, cap/0 then cap/1",
        "PV-01",
        "\"profile\": \"cap/1\",",
        "\"profile\": \"cap/0\", \"\\u0070rofile\": \"cap/1\",",
        true,
    ),
    (
        "nested digest algorithm, SHA-256 then MD5",
        "PV-01",
        "\"algorithm\": \"SHA-256\",",
        "\"algorithm\": \"SHA-256\", \"algorithm\": \"MD5\",",
        true,
    ),
    (
        "integrity.complete three times, true false true",
        "PV-03",
        "\"complete\": false,",
        "\"complete\": true, \"complete\": false, \"complete\": true,",
        false,
    ),
];

#[test]
fn duplicate_member_names_refuse_at_admission_whatever_the_last_value_says() {
    for &(name, base, from, to, last_wins_conforms) in DUPLICATE_NAME_CASES {
        let text = String::from_utf8(normative_vector(base)).expect("vector is UTF-8");
        assert_eq!(
            text.matches(from).count(),
            1,
            "{name}: anchor must be unique"
        );
        leaf_verify(text.as_bytes()).unwrap_or_else(|e| panic!("{name}: base must conform: {e}"));

        let mutant = text.replacen(from, to, 1);
        let err = leaf_verify(mutant.as_bytes()).expect_err(name);
        assert_eq!(
            err,
            Cap1Refusal::Syntax(Cap1SyntaxFault::DuplicateKey),
            "{name}"
        );

        let last_wins: serde_json::Value =
            serde_json::from_str(&mutant).expect("a last-wins parser reads the mutant");
        let last_wins = serde_json::to_vec(&last_wins).expect("re-serialises");
        match leaf_verify(&last_wins) {
            Ok(_) => assert!(last_wins_conforms, "{name}: last-wins reading conforms"),
            Err(e) => {
                assert!(
                    !last_wins_conforms,
                    "{name}: last-wins reading refused: {e}"
                );
                assert_ne!(e.stage(), Cap1Stage::Admission, "{name}: {e}");
            }
        }
    }
}

fn normative_vector(id: &str) -> Vec<u8> {
    fs::read(normative_dir().join(format!("{id}.json"))).expect("vector readable")
}

fn leaf_verify(bytes: &[u8]) -> Result<Cap1Document, Cap1Refusal> {
    verify_cap1_document(bytes, &Cap1AdmissionLimits::default())
}
