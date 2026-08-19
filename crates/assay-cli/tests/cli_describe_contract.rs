//! Runtime CLI contract traversal (#2178).
//!
//! The product claim is that contracts are machine-checkable. This test is the
//! first place a caller can ask for the surface and descend it. Identities are
//! asserted from the shipping constants that emit them, not from copies here.

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn assay_src(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative)
}

/// Read a `&str` shipping constant from assay-cli source. The value lives in one
/// place; copying it into this test would be a second definition that can drift.
fn shipping_str_const(relative: &str, name: &str) -> String {
    let source = fs::read_to_string(assay_src(relative)).unwrap_or_else(|error| {
        panic!("failed to read shipping constant source {relative}: {error}")
    });
    let needle = format!("const {name}: &str = \"");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("{relative} must define {name} as a &str constant"));
    let rest = &source[start + needle.len()..];
    let end = rest
        .find('"')
        .unwrap_or_else(|| panic!("{name} in {relative} is not a closed string literal"));
    rest[..end].to_string()
}

fn describe(path: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_")
        {
            command.env_remove(name);
        }
    }
    command.env("NO_COLOR", "1").arg("describe").args(path);
    run_bounded(command, b"", GOLDEN_PATH_LIMITS, "assay describe").expect("describe ran")
}

fn exit_code(output: &Output) -> i32 {
    output
        .status
        .code()
        .expect("describe exited by code rather than by signal")
}

fn sole_report(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "describe stdout is not one JSON document: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn command_names(document: &Value) -> Vec<&str> {
    document["commands"]
        .as_array()
        .expect("describe document must list commands")
        .iter()
        .map(|command| {
            command["name"]
                .as_str()
                .expect("each command entry must have a name")
        })
        .collect()
}

fn identities(document: &Value) -> Vec<&str> {
    document["identities"]
        .as_array()
        .expect("describe document must list identities")
        .iter()
        .map(|identity| identity.as_str().expect("identities are strings"))
        .collect()
}

#[test]
fn describe_lists_top_level_commands_without_dumping_identities() {
    let output = describe(&[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        exit_code(&output),
        0,
        "assay describe must be a runtime entry point; stderr={stderr}"
    );
    assert!(
        output.stderr.is_empty(),
        "machine describe must not also write a human line; stderr={stderr}"
    );

    let document = sole_report(&output);
    let describe_schema = shipping_str_const("cli/commands/describe.rs", "DESCRIBE_REPORT_SCHEMA");
    assert_eq!(document["schema"], describe_schema);
    assert_eq!(document["path"], Value::Array(vec![]));

    let names = command_names(&document);
    assert!(
        names.contains(&"doctor"),
        "top-level listing must include doctor so a caller can descend; names={names:?}"
    );
    assert!(
        names.contains(&"evidence"),
        "top-level listing must include evidence so a caller can descend; names={names:?}"
    );
    assert!(
        names.contains(&"describe"),
        "the introspection command is itself part of the surface; names={names:?}"
    );
    assert!(
        names.iter().all(|name| *name != "runner-spike"),
        "hidden commands stay hidden; names={names:?}"
    );

    let top_identities = identities(&document);
    let doctor_schema = shipping_str_const("diagnostics/report.rs", "DOCTOR_REPORT_SCHEMA");
    assert!(
        !top_identities.contains(&doctor_schema.as_str()),
        "root must not dump leaf identities; identities={top_identities:?}"
    );
}

#[test]
fn describe_descends_to_a_leaf_and_lists_its_shipping_identity() {
    let doctor_schema = shipping_str_const("diagnostics/report.rs", "DOCTOR_REPORT_SCHEMA");
    let output = describe(&["doctor"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        exit_code(&output),
        0,
        "assay describe doctor must descend; stderr={stderr}"
    );

    let document = sole_report(&output);
    assert_eq!(document["path"], Value::Array(vec!["doctor".into()]));
    let listed = identities(&document);
    assert!(
        listed.contains(&doctor_schema.as_str()),
        "doctor listing must publish the shipping doctor report identity; identities={listed:?}"
    );
}

#[test]
fn describe_parent_listing_includes_child_identities() {
    let list_schema = shipping_str_const(
        "cli/commands/evidence/schema/reports.rs",
        "SCHEMA_LIST_REPORT",
    );
    let output = describe(&["evidence", "schema"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        exit_code(&output),
        0,
        "assay describe evidence schema must descend; stderr={stderr}"
    );

    let document = sole_report(&output);
    let names = command_names(&document);
    assert!(
        names.contains(&"list"),
        "parent listing must name the list child; names={names:?}"
    );

    let listed = identities(&document);
    assert!(
        listed.contains(&list_schema.as_str()),
        "parent listing must include a child identity that exists in code; identities={listed:?}"
    );
}
