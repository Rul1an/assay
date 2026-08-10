use assay_core::mcp::policy::McpPolicy;
use std::io::Write;

fn load_error_for(schema_hash: &str, meta_hash: &str) -> String {
    let mut file = tempfile::NamedTempFile::new().expect("temporary policy file");
    writeln!(
        file,
        r#"tool_pins:
  read_file:
    server_id: filesystem-prod
    tool_name: read_file
    schema_hash: "{schema_hash}"
    meta_hash: "{meta_hash}""#
    )
    .expect("write policy");

    McpPolicy::from_file(file.path())
        .expect_err("malformed tool pin must fail at policy load")
        .to_string()
}

#[test]
fn malformed_tool_pin_hashes_are_rejected_at_policy_load() {
    let valid = "a".repeat(64);

    let short_schema = load_error_for("ab", &valid);
    assert!(
        short_schema.contains("tool_pins.read_file.schema_hash"),
        "error must name the malformed field: {short_schema}"
    );

    let uppercase_meta = load_error_for(&valid, &"A".repeat(64));
    assert!(
        uppercase_meta.contains("tool_pins.read_file.meta_hash"),
        "error must name the malformed field: {uppercase_meta}"
    );
}

#[test]
fn valid_tool_pin_hashes_load() {
    let valid = "a".repeat(64);
    let mut file = tempfile::NamedTempFile::new().expect("temporary policy file");
    writeln!(
        file,
        r#"tool_pins:
  read_file:
    server_id: filesystem-prod
    tool_name: read_file
    schema_hash: "{valid}"
    meta_hash: "{valid}""#
    )
    .expect("write policy");

    McpPolicy::from_file(file.path()).expect("valid lowercase SHA-256 pins must load");
}
