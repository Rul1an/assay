use assay_evidence::bundle::BundleReader;
use assert_cmd::Command;
use jsonschema::{Draft, Validator};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn schema_path(relative: &str) -> PathBuf {
    repo_root()
        .join("docs/reference/receipt-schemas")
        .join(relative)
}

fn packaged_schema_path(relative: &str) -> PathBuf {
    repo_root()
        .join("crates/assay-cli/receipt-schemas")
        .join(relative)
}

fn receipt_family_matrix() -> Value {
    let path = repo_root().join("docs/reference/receipt-family-matrix.json");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn compile_schema(relative: &str) -> Validator {
    let path = schema_path(relative);
    let schema: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema)
        .unwrap_or_else(|err| panic!("failed to compile {}: {err}", path.display()))
}

fn assert_valid(schema_relative: &str, instance: &Value) {
    let validator = compile_schema(schema_relative);
    if validator.is_valid(instance) {
        return;
    }
    let errors = validator
        .iter_errors(instance)
        .map(|err| format!("{err} at {}", err.instance_path()))
        .collect::<Vec<_>>()
        .join("\n");
    panic!("{schema_relative} validation failed:\n{errors}\ninstance: {instance}");
}

fn assay_schema_command() -> Command {
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence").arg("schema");
    cmd
}

fn jsonl_values(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn bundle_payloads(path: &Path) -> Vec<Value> {
    let reader = BundleReader::open(File::open(path).unwrap()).unwrap();
    reader
        .events()
        .map(|event| event.unwrap().payload)
        .collect()
}

#[path = "receipt_schema_registry_test/schema_family_paths.rs"]
mod schema_family_paths;
#[path = "receipt_schema_registry_test/schema_inventory.rs"]
mod schema_inventory;
#[path = "receipt_schema_registry_test/schema_validation_cli_errors.rs"]
mod schema_validation_cli_errors;
