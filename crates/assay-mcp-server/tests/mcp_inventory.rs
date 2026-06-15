use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

fn fixture() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/mcp_inventory/inventory_cases.v0.json"
    );
    let bytes = std::fs::read(path).expect("fixture readable");
    serde_json::from_slice(&bytes).expect("fixture is valid json")
}

fn string_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{key} must be a string in {value:?}"))
}

fn coverage_incomplete(inventory: &Value) -> bool {
    let coverage = &inventory["scanner_coverage"];
    let config_incomplete = coverage["config_sources"]
        .as_object()
        .map(|sources| {
            sources.values().any(|state| {
                matches!(
                    state.as_str(),
                    Some("not_scanned") | Some("unavailable") | Some("partial")
                )
            })
        })
        .unwrap_or(true);

    let process_incomplete = !matches!(
        coverage["process_scan"].as_str(),
        Some("complete") | Some("not_applicable")
    );
    let network_incomplete = !matches!(
        coverage["network_scan"].as_str(),
        Some("complete") | Some("not_applicable")
    );

    config_incomplete || process_incomplete || network_incomplete
}

fn classify(case: &Value) -> BTreeSet<String> {
    let declared = case["declared_servers"]
        .as_array()
        .expect("declared_servers array");
    let inventory = &case["inventory"];
    assert_eq!(inventory["schema"], "assay.mcp_server_inventory.v0");
    assert!(inventory["non_claims"]
        .as_array()
        .expect("non_claims array")
        .iter()
        .any(|claim| claim
            .as_str()
            .is_some_and(|s| s.contains("absence from inventory is not absence"))));

    let mut declared_by_id = BTreeMap::new();
    for server in declared {
        declared_by_id.insert(string_field(server, "server_id"), server);
    }

    let servers = inventory["servers"].as_array().expect("servers array");
    let mut findings = BTreeSet::new();
    let mut digests_by_server = BTreeMap::<&str, BTreeSet<&str>>::new();

    for server in servers {
        let server_id = string_field(server, "server_id");
        let command_digest = string_field(server, "command_digest");
        digests_by_server
            .entry(server_id)
            .or_default()
            .insert(command_digest);

        match declared_by_id.get(server_id) {
            None => {
                findings.insert("shadow_mcp_server_observed".to_string());
            }
            Some(expected) => {
                if string_field(expected, "command_digest") != command_digest {
                    findings.insert("mcp_server_command_drift".to_string());
                }
                if string_field(expected, "args_digest") != string_field(server, "args_digest") {
                    findings.insert("mcp_server_args_drift".to_string());
                }
            }
        }
    }

    if digests_by_server
        .values()
        .any(|command_digests| command_digests.len() > 1)
    {
        findings.insert("duplicate_mcp_server_identity".to_string());
    }

    if coverage_incomplete(inventory) {
        findings.insert("mcp_inventory_coverage_incomplete".to_string());
    }

    findings
}

#[test]
fn mcp_inventory_fixture_reproduces_expected_findings() {
    let fixture = fixture();
    assert_eq!(fixture["schema_contract"], "assay.mcp_server_inventory.v0");

    let cases = fixture["cases"].as_array().expect("cases array");
    assert_eq!(
        cases.len(),
        9,
        "M1 corpus should pin nine shadow-server cases"
    );

    for case in cases {
        let name = string_field(case, "case");
        let expected: BTreeSet<_> = case["expected_findings"]
            .as_array()
            .unwrap_or_else(|| panic!("{name} expected_findings array"))
            .iter()
            .map(|finding| {
                finding
                    .as_str()
                    .unwrap_or_else(|| panic!("{name} finding must be string"))
                    .to_string()
            })
            .collect();
        assert_eq!(classify(case), expected, "{name}");
    }
}

#[test]
fn mcp_inventory_incomplete_coverage_never_reads_clean() {
    let fixture = fixture();
    let cases = fixture["cases"].as_array().expect("cases array");

    for case in cases {
        if coverage_incomplete(&case["inventory"]) {
            let findings = classify(case);
            assert!(
                findings.contains("mcp_inventory_coverage_incomplete"),
                "{} must report incomplete coverage",
                string_field(case, "case")
            );
        }
    }
}
