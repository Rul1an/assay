//! Parity test: the runner's built-in secret rules must match the shared `secret-rules.v1.json`
//! contract fixture exactly (ADR-034 Phase 2). The same fixture is committed to the Plimsoll repo
//! with its own parity test, so the Rust runner and the Python detector cannot drift apart.

use std::collections::BTreeMap;

use assay_runner_core::rule_specs;

#[test]
fn builtin_rules_match_the_shared_contract_fixture() {
    let raw = include_str!("fixtures/secret-rules.v1.json");
    let doc: serde_json::Value = serde_json::from_str(raw).expect("fixture is valid json");

    assert_eq!(
        doc["schema"], "assay.secret-rules.v1",
        "fixture schema tag changed unexpectedly"
    );

    let fixture: BTreeMap<String, String> = doc["rules"]
        .as_array()
        .expect("rules is an array")
        .iter()
        .map(|r| {
            (
                r["name"].as_str().unwrap().to_string(),
                r["pattern"].as_str().unwrap().to_string(),
            )
        })
        .collect();

    let builtin: BTreeMap<String, String> = rule_specs()
        .iter()
        .map(|(name, pat)| (name.to_string(), pat.to_string()))
        .collect();

    assert_eq!(
        builtin, fixture,
        "runner secret rules drifted from secret-rules.v1.json; update the fixture AND the Plimsoll \
         detector together so the two implementations stay in lockstep"
    );
}
