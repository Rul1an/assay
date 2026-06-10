//! P57a guard: the committed `assay.tool_decision_surface.v0` reference fixtures must hold the
//! load-bearing invariants of the spec (docs/reference/tool-decision-surface.md). There is no
//! producer yet (that is P57b/P57c); this only keeps the reference vectors honest so a later
//! producer has a fixed target and cannot quietly relax the contract.

use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tool_decisions")
}

const KNOWN_CLASSIFICATIONS: &[&str] = &[
    "classified",
    "classified_incomplete",
    "observed_unknown_tool",
    "redaction_failed",
    "not_observed",
];

const KNOWN_REASON_CODES: &[&str] = &[
    "classified_github_deploy_key",
    "classified_slack_add_member",
    "classified_workspace_admin",
    "missing_required_target_field",
    "unknown_tool_name",
    "redacted_secret_argument",
    "unsupported_argument_shape",
];

#[test]
fn tool_decision_fixtures_hold_the_spec_invariants() {
    let dir = fixtures_dir();
    let mut checked = 0;
    for entry in fs::read_dir(&dir).expect("fixtures dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| {
                panic!("fixture {} is not valid JSON: {e}", path.display());
            });
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        assert_eq!(
            v["schema"].as_str(),
            Some("assay.tool_decision_surface.v0"),
            "{name}: wrong schema id"
        );
        assert!(
            v["non_claims"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false),
            "{name}: non_claims must be present and non-empty"
        );

        let decisions = v["observed_tool_decisions"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: observed_tool_decisions must be an array"));
        assert!(
            !decisions.is_empty(),
            "{name}: at least one decision expected"
        );

        for d in decisions {
            let classification = d["classification"].as_str().unwrap_or("");
            assert!(
                KNOWN_CLASSIFICATIONS.contains(&classification),
                "{name}: unknown classification {classification:?} (an unknown tool must be \
                 observed_unknown_tool, never absent/clean)"
            );

            // Every decision carries a machine-readable reason code from the pinned set.
            let reason = d["reason_code"].as_str().unwrap_or("");
            assert!(
                KNOWN_REASON_CODES.contains(&reason),
                "{name}: unknown reason_code {reason:?}"
            );

            // No raw secret material ever rides in the record.
            assert_eq!(
                d["redaction"]["secret_material_stored"].as_bool(),
                Some(false),
                "{name}: secret_material_stored must be false"
            );

            // Asserted is not verified: a fixture must never claim a verified side effect, since
            // none of these carry independently-checked audit evidence.
            assert_eq!(
                d["response"]["side_effect_verified"].as_bool(),
                Some(false),
                "{name}: side_effect_verified must be false without verified audit evidence"
            );

            // A denied decision must not assert a side effect.
            if d["decision"]["effect"].as_str() == Some("deny") {
                assert_eq!(
                    d["response"]["side_effect_asserted"].as_bool(),
                    Some(false),
                    "{name}: a denied decision must not assert a side effect"
                );
            }
        }
        checked += 1;
    }
    assert!(
        checked >= 7,
        "expected the full reference-vector set, found {checked}"
    );
}
