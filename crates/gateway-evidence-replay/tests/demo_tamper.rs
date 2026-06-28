use std::fs;
use std::path::Path;

use gateway_evidence_replay::schema::{Ceiling, Reason, Status};
use gateway_evidence_replay::verify_json_str;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const DEMO_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/gateway-path-v0/demo");
const DEMO_FILES: &[&str] = &[
    "README.md",
    "clean-route.json",
    "partial-route-substitution.json",
    "stale-attestation.json",
    "unknown-source.json",
    "expected.json",
    "manifest.json",
    "manifest-sha256.txt",
];

#[derive(Debug, Deserialize)]
struct Manifest {
    fixtures: Vec<ManifestFile>,
    expected_sha256: String,
}

#[derive(Debug, Deserialize)]
struct ManifestFile {
    file: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct ExpectedFile {
    cases: Vec<ExpectedCase>,
}

#[derive(Debug, Deserialize)]
struct ExpectedCase {
    file: String,
    status: Status,
    ceiling: Option<Ceiling>,
    reasons: Vec<Reason>,
}

#[test]
fn demo_manifest_pins_fixture_and_expected_bytes() {
    verify_demo_dir(Path::new(DEMO_DIR)).expect("demo manifest verifies");
}

#[test]
fn fixture_tamper_fails_manifest_check() {
    let temp = copy_demo_dir();
    fs::write(temp.path().join("clean-route.json"), "{}\n").expect("tamper fixture");

    let err = verify_demo_dir(temp.path()).expect_err("fixture tamper must fail");
    assert!(err.contains("clean-route.json"), "{err}");
}

#[test]
fn expected_tamper_fails_manifest_check() {
    let temp = copy_demo_dir();
    fs::write(temp.path().join("expected.json"), "{}\n").expect("tamper expected");

    let err = verify_demo_dir(temp.path()).expect_err("expected tamper must fail");
    assert!(err.contains("expected.json"), "{err}");
}

#[test]
fn manifest_tamper_fails_manifest_sha_check() {
    let temp = copy_demo_dir();
    let manifest_path = temp.path().join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["claims"] = Value::String("externally_reproduced".to_string());
    write_json(&manifest_path, &manifest);

    let err = verify_demo_dir(temp.path()).expect_err("manifest tamper must fail");
    assert!(err.contains("manifest.json"), "{err}");
}

#[test]
fn replay_mismatch_fails_after_digest_gate_passes() {
    let temp = copy_demo_dir();
    let expected_path = temp.path().join("expected.json");
    let manifest_path = temp.path().join("manifest.json");
    let manifest_sha_path = temp.path().join("manifest-sha256.txt");

    let mut expected: Value =
        serde_json::from_str(&fs::read_to_string(&expected_path).unwrap()).unwrap();
    expected["cases"][0]["status"] = Value::String("invalid".to_string());
    write_json(&expected_path, &expected);

    let mut manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["expected_sha256"] = Value::String(sha256_file(&expected_path));
    write_json(&manifest_path, &manifest);
    fs::write(
        &manifest_sha_path,
        format!("{}\n", sha256_file(&manifest_path)),
    )
    .unwrap();

    let err = verify_demo_dir(temp.path()).expect_err("replay mismatch must fail");
    assert!(err.contains("clean-route.json"), "{err}");
}

fn verify_demo_dir(dir: &Path) -> Result<(), String> {
    let manifest_path = dir.join("manifest.json");
    let manifest_sha_path = dir.join("manifest-sha256.txt");

    let pinned_manifest_sha = fs::read_to_string(&manifest_sha_path)
        .map_err(|err| format!("read manifest-sha256.txt: {err}"))?;
    let actual_manifest_sha = sha256_file(&manifest_path);
    if pinned_manifest_sha.trim() != actual_manifest_sha {
        return Err("manifest.json digest mismatch".to_string());
    }

    let manifest: Manifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path).map_err(|err| format!("read manifest: {err}"))?,
    )
    .map_err(|err| format!("parse manifest: {err}"))?;

    let expected_path = dir.join("expected.json");
    let actual_expected_sha = sha256_file(&expected_path);
    if manifest.expected_sha256 != actual_expected_sha {
        return Err("expected.json digest mismatch".to_string());
    }

    for fixture in &manifest.fixtures {
        let actual = sha256_file(&dir.join(&fixture.file));
        if actual != fixture.sha256 {
            return Err(format!("{} digest mismatch", fixture.file));
        }
    }

    let expected: ExpectedFile = serde_json::from_str(
        &fs::read_to_string(&expected_path).map_err(|err| format!("read expected: {err}"))?,
    )
    .map_err(|err| format!("parse expected: {err}"))?;

    for case in expected.cases {
        let body = fs::read_to_string(dir.join(&case.file))
            .map_err(|err| format!("read {}: {err}", case.file))?;
        let got = verify_json_str(&body);
        if got.status != case.status || got.ceiling != case.ceiling || got.reasons != case.reasons {
            return Err(format!("{} replay mismatch", case.file));
        }
    }

    Ok(())
}

fn copy_demo_dir() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    for name in DEMO_FILES {
        fs::copy(Path::new(DEMO_DIR).join(name), temp.path().join(name)).expect("copy demo file");
    }
    temp
}

fn write_json(path: &Path, value: &Value) {
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(value).unwrap()),
    )
    .unwrap();
}

fn sha256_file(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}
