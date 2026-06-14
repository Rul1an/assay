//! SARIF 2.1.0 projection of `assay.enforcement_decision.v0` records (C2-4).
//!
//! A consumer view for the GitHub Security tab. Each `deny` becomes a SARIF result (the finding);
//! `allow` records, and any non-enforcement-decision records, are skipped — SARIF surfaces the
//! things to review, not the normal path. The projection is leak-free: it reads only the sanitized
//! fields the record already exposes (tool name, action_class, reason, drift_state, fail_closed),
//! never raw arguments or targets. The level is `warning`: a deny is fail-closed caution surfaced
//! for review, not a maliciousness verdict; the pass/fail of a PR comes from the gate's exit code,
//! not from this projection.

use assay_core::report::sarif::SARIF_SCHEMA;
use serde_json::{json, Value};

/// The carrier this projection consumes.
pub const ENFORCEMENT_DECISION_SCHEMA: &str = "assay.enforcement_decision.v0";

/// Project enforcement-decision records into a SARIF 2.1.0 document. Deterministic; only `deny`
/// records produce results.
pub fn enforcement_decisions_to_sarif(records: &[Value]) -> Value {
    let mut results: Vec<Value> = Vec::new();
    let mut rule_ids: Vec<String> = Vec::new();

    for record in records {
        if record.get("schema").and_then(Value::as_str) != Some(ENFORCEMENT_DECISION_SCHEMA) {
            continue;
        }
        if record.get("decision").and_then(Value::as_str) != Some("deny") {
            continue;
        }
        let reason = record
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let tool = record
            .pointer("/tool/name")
            .and_then(Value::as_str)
            .unwrap_or("");
        let action_class = record.pointer("/tool/action_class").and_then(Value::as_str);
        let drift_state = record.get("drift_state").and_then(Value::as_str);

        if !rule_ids.iter().any(|existing| existing == reason) {
            rule_ids.push(reason.to_string());
        }

        results.push(json!({
            "ruleId": reason,
            "level": "warning",
            "message": {
                "text": format!(
                    "Privileged tool action denied before forward: {tool} ({}) — {reason}",
                    action_class.unwrap_or("unclassified")
                )
            },
            "logicalLocations": [{ "name": tool, "kind": "function" }],
            "properties": {
                "decision": "deny",
                "reason": reason,
                "action_class": action_class,
                "drift_state": drift_state,
                "fail_closed": record.get("fail_closed"),
            }
        }));
    }

    let rules: Vec<Value> = rule_ids
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "shortDescription": { "text": describe_reason(id) },
            })
        })
        .collect();

    json!({
        "$schema": SARIF_SCHEMA,
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "assay",
                "informationUri": "https://github.com/Rul1an/assay",
                "rules": rules,
            }},
            "results": results,
        }]
    })
}

/// Bounded, human-readable text for a deny reason. Unknown reasons fall back to a generic label so a
/// new producer code never breaks the projection.
fn describe_reason(reason: &str) -> &'static str {
    match reason {
        "no_declared_allowance" => "No allowance declares this privileged action for the caller",
        "credential_scope_insufficient" => {
            "The declared credential scope does not cover the action"
        }
        "credential_scope_unknown" => "No credential declared; scope coverage cannot be determined",
        "manifest_drifted_since_approval" => "The observed tool surface changed since approval",
        "manifest_baseline_missing" => "No approved baseline exists for the tool",
        "manifest_current_observation_incomplete" => {
            "No complete current observation of the tool surface"
        }
        "manifest_current_observation_incomplete_tool_absent" => {
            "The tool was absent from the current complete observation"
        }
        "manifest_observation_ambiguous" => "The observed manifest is ambiguous (duplicate names)",
        "allowance_target_mismatch" => "The action target is not in the caller's allowance",
        "unclassified_tool_call" => "The tool call could not be classified",
        "classification_incomplete" => "The tool classification was incomplete",
        _ => "Enforcement deny",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deny(reason: &str, tool: &str, action_class: &str, drift: &str) -> Value {
        json!({
            "schema": ENFORCEMENT_DECISION_SCHEMA,
            "caller": {"id": "ci-agent"},
            "tool": {"name": tool, "action_class": action_class},
            "action": {"verb": "create", "resource_type": action_class,
                       "target": {"owner": "acme", "repo": "prod-app"},
                       "target_digest": "sha256:deadbeef"},
            "decision": "deny",
            "reason": reason,
            "fail_closed": true,
            "drift_state": drift,
            "credential_alias": "gh-deploy",
            "non_claims": []
        })
    }

    fn allow(tool: &str) -> Value {
        json!({
            "schema": ENFORCEMENT_DECISION_SCHEMA,
            "tool": {"name": tool, "action_class": "github_deploy_key"},
            "decision": "allow", "reason": "allow", "fail_closed": false,
            "drift_state": "satisfied"
        })
    }

    #[test]
    fn denies_become_results_allows_are_skipped() {
        let recs = vec![
            deny(
                "no_declared_allowance",
                "github.add_deploy_key",
                "github_deploy_key",
                "not_evaluated",
            ),
            allow("github.add_deploy_key"),
            deny(
                "manifest_drifted_since_approval",
                "github.add_deploy_key",
                "github_deploy_key",
                "drifted",
            ),
        ];
        let sarif = enforcement_decisions_to_sarif(&recs);
        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(sarif["$schema"], json!(SARIF_SCHEMA));
        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 2, "only the two denies become results");
        assert_eq!(results[0]["ruleId"], "no_declared_allowance");
        assert_eq!(results[0]["level"], "warning");
        assert_eq!(results[1]["ruleId"], "manifest_drifted_since_approval");
        let rules = sarif["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap();
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn projection_does_not_leak_raw_target_or_arguments() {
        let recs = vec![deny(
            "no_declared_allowance",
            "github.add_deploy_key",
            "github_deploy_key",
            "not_evaluated",
        )];
        let sarif = enforcement_decisions_to_sarif(&recs);
        let blob = serde_json::to_string(&sarif).unwrap();
        for forbidden in [
            "prod-app",
            "deadbeef",
            "target_digest",
            "arguments",
            "owner",
        ] {
            assert!(
                !blob.contains(forbidden),
                "SARIF must not leak `{forbidden}`"
            );
        }
    }

    #[test]
    fn non_enforcement_records_and_unknown_reasons_are_handled() {
        let recs = vec![
            json!({"schema": "assay.manifest_establish.v0", "decision": "deny"}),
            deny("some_future_reason", "x.y", "z", "not_evaluated"),
        ];
        let sarif = enforcement_decisions_to_sarif(&recs);
        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 1, "the non-enforcement record is skipped");
        assert_eq!(results[0]["ruleId"], "some_future_reason");
        // unknown reason still gets a rule with the generic description (never panics)
        assert_eq!(
            sarif["runs"][0]["tool"]["driver"]["rules"][0]["shortDescription"]["text"],
            "Enforcement deny"
        );
    }

    // ---- Golden contract fixture (C2-4) -------------------------------------------------------

    fn fixture_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/enforcement_decision_sarif.v0.json")
    }

    /// Canonical input: the three deny axes plus an allow (which is skipped), mirroring the
    /// privileged-action-gate example.
    fn canonical_input() -> Vec<Value> {
        vec![
            deny(
                "no_declared_allowance",
                "github.add_deploy_key",
                "github_deploy_key",
                "not_evaluated",
            ),
            deny(
                "credential_scope_insufficient",
                "github.add_deploy_key",
                "github_deploy_key",
                "not_evaluated",
            ),
            deny(
                "manifest_drifted_since_approval",
                "github.add_deploy_key",
                "github_deploy_key",
                "drifted",
            ),
            allow("github.add_deploy_key"),
        ]
    }

    #[test]
    fn enforcement_decision_sarif_contract_fixture() {
        let generated = enforcement_decisions_to_sarif(&canonical_input());
        let path = fixture_path();
        if std::env::var("ASSAY_UPDATE_GOLDEN").is_ok() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                format!("{}\n", serde_json::to_string_pretty(&generated).unwrap()),
            )
            .unwrap();
        }
        let committed: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!(
                    "missing {}; regenerate with ASSAY_UPDATE_GOLDEN=1",
                    path.display()
                )
            }))
            .unwrap();
        assert_eq!(
            committed, generated,
            "enforcement_decision_sarif fixture is stale; regenerate with ASSAY_UPDATE_GOLDEN=1"
        );
        // three denies -> three results; the allow is skipped.
        assert_eq!(generated["runs"][0]["results"].as_array().unwrap().len(), 3);
    }
}
