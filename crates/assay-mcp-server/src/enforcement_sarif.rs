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

/// Synthetic, repo-relative location for each finding. An enforcement decision is not tied to a
/// source line, but GitHub code scanning requires every result to carry a `locations` entry, so each
/// result points at this stable synthetic path and names the tool as a logical location.
const SYNTHETIC_LOCATION_URI: &str = ".assay/enforcement-decisions.ndjson";

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
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": { "uri": SYNTHETIC_LOCATION_URI },
                    "region": { "startLine": 1, "startColumn": 1 }
                },
                "logicalLocations": [{ "name": tool, "kind": "function" }]
            }],
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

/// Bounded, human-readable text for every deny reason the producer can emit.
///
/// One table, one meaning: [`describe_reason`] reads this table, and the
/// claims backstop below iterates it, so a new reason cannot be added without
/// being scanned for unearned status (#2232).
const REASON_DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "no_declared_allowance",
        "No allowance declares this privileged action for the caller",
    ),
    (
        "credential_scope_insufficient",
        "The declared credential scope does not cover the action",
    ),
    (
        "credential_scope_unknown",
        "No credential declared; scope coverage cannot be determined",
    ),
    (
        "manifest_drifted_since_approval",
        "The observed tool surface changed since approval",
    ),
    (
        "manifest_baseline_missing",
        "No declared baseline exists for the tool",
    ),
    (
        "manifest_current_observation_incomplete",
        "No complete current observation of the tool surface",
    ),
    (
        "manifest_current_observation_incomplete_tool_absent",
        "The tool was absent from the current complete observation",
    ),
    (
        "manifest_observation_ambiguous",
        "The observed manifest is ambiguous (duplicate names)",
    ),
    (
        "allowance_target_mismatch",
        "The action target is not in the caller's allowance",
    ),
    (
        "unclassified_tool_call",
        "The tool call could not be classified",
    ),
    (
        "classification_incomplete",
        "The tool classification was incomplete",
    ),
];

/// Look up a deny reason in [`REASON_DESCRIPTIONS`]. Unknown reasons fall back
/// to a generic label so a new producer code never breaks the projection.
fn describe_reason(reason: &str) -> &'static str {
    REASON_DESCRIPTIONS
        .iter()
        .find(|(code, _)| *code == reason)
        .map(|(_, description)| *description)
        .unwrap_or("Enforcement deny")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::claims_boundary_tests::assert_no_unearned_status;

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

    /// Tool name used by the scanned inputs below; the same value is handed to
    /// the scan as reflected, so input and excision cannot drift apart.
    const SCAN_TOOL: &str = "github.add_deploy_key";

    /// A deny whose record carries a null action_class. Production-reachable:
    /// proxy/enforce/tests/pdp.rs asserts a real record can have
    /// `tool.action_class` null, and the projection renders it "unclassified".
    fn deny_null_action_class(reason: &str, tool: &str, drift: &str) -> Value {
        let mut record = deny(reason, tool, "github_deploy_key", drift);
        record["tool"]["action_class"] = Value::Null;
        record
    }

    /// A deny record without a `reason` leaf at all. The projection falls back
    /// to the "unknown" reason and the generic description.
    fn deny_without_reason(tool: &str) -> Value {
        let mut record = deny(
            "no_declared_allowance",
            tool,
            "github_deploy_key",
            "not_evaluated",
        );
        record
            .as_object_mut()
            .expect("record is an object")
            .remove("reason");
        record
    }

    /// Every Assay-authored leaf the projection can emit: one deny per
    /// [`REASON_DESCRIPTIONS`] row (iterated from the production table, so a
    /// new reason is scanned from the moment it is added), plus the
    /// null-action_class, missing-reason, and unknown-reason fallbacks.
    fn full_coverage_input() -> Vec<Value> {
        let mut records: Vec<Value> = REASON_DESCRIPTIONS
            .iter()
            .map(|(code, _)| deny(code, SCAN_TOOL, "github_deploy_key", "not_evaluated"))
            .collect();
        records.push(deny_null_action_class(
            "no_declared_allowance",
            SCAN_TOOL,
            "not_evaluated",
        ));
        records.push(deny_without_reason(SCAN_TOOL));
        records.push(deny(
            "some_future_reason",
            SCAN_TOOL,
            "github_deploy_key",
            "not_evaluated",
        ));
        records
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
    fn result_location_shape_is_github_valid() {
        // GitHub's SARIF uploader rejects `logicalLocations` placed directly on a result; it must
        // nest inside `locations[].location` next to a `physicalLocation`. This guards the exact
        // shape GitHub once rejected (a deny that failed to upload to the Security tab).
        let recs = vec![deny(
            "no_declared_allowance",
            "github.add_deploy_key",
            "github_deploy_key",
            "not_evaluated",
        )];
        let sarif = enforcement_decisions_to_sarif(&recs);
        let result = &sarif["runs"][0]["results"][0];
        assert!(
            result.get("logicalLocations").is_none(),
            "logicalLocations must not sit directly on a result"
        );
        let loc = &result["locations"][0];
        assert_eq!(
            loc["physicalLocation"]["artifactLocation"]["uri"],
            SYNTHETIC_LOCATION_URI
        );
        assert_eq!(loc["physicalLocation"]["region"]["startLine"], 1);
        assert_eq!(loc["logicalLocations"][0]["name"], "github.add_deploy_key");
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

    // ---- ADR-043 §2 wire status claims (#2232) -------------------------------------------
    //
    // The SARIF projection is Assay's own words reaching a consumer (the GitHub
    // Security tab): the rule descriptions from `REASON_DESCRIPTIONS`, the deny
    // message template, and the `decision`/`reason`/`action_class`/`drift_state`
    // leaves. The generated document therefore gets the same closed-set
    // backstop as the handshake, through the single implementation in
    // `crate::server::claims_boundary_tests` — one list, one meaning.
    //
    // Value domain: exactly those Assay-authored leaves. The reflected upstream
    // tool names are the caller's data, not Assay's assertion (the same reason
    // the `tool_identity` guard pins reflection instead of scanning words,
    // #3082), so they are excised per field before the scan — the whole
    // `logicalLocations[].name` value and the known message slot only, never a
    // document-wide replace that could mask Assay's own words — and pinned
    // byte-exact by the acceptance case below instead.

    /// Scan the Assay-authored leaves of a generated SARIF document, excising
    /// the reflected upstream tool names by field: each `logicalLocations[].name`
    /// value is blanked whole (the field IS the reflection), and the tool name
    /// is removed from the known `message.text` slot only, so an identical word
    /// in any Assay-authored leaf still trips the backstop.
    fn assert_sarif_has_no_unearned_status(
        label: &str,
        sarif: &serde_json::Value,
        reflected_tool_names: &[&str],
    ) {
        let mut scrubbed = sarif.clone();
        let results = scrubbed
            .get_mut("runs")
            .and_then(|runs| runs.get_mut(0))
            .and_then(|run| run.get_mut("results"))
            .and_then(Value::as_array_mut)
            .expect("sarif run results");
        for result in results.iter_mut() {
            if let Some(locations) = result.get_mut("locations").and_then(Value::as_array_mut) {
                for location in locations.iter_mut() {
                    if let Some(names) = location
                        .get_mut("logicalLocations")
                        .and_then(Value::as_array_mut)
                    {
                        for name in names.iter_mut() {
                            if name.get("name").is_some() {
                                name["name"] = Value::String(String::new());
                            }
                        }
                    }
                }
            }
            if let Some(text) = result
                .get("message")
                .and_then(|message| message.get("text"))
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                let mut slot = text;
                for reflected in reflected_tool_names {
                    slot = slot.replace(reflected, "");
                }
                result["message"]["text"] = Value::String(slot);
            }
        }
        assert_no_unearned_status(label, &scrubbed);
    }

    #[test]
    fn sarif_projection_asserts_no_unearned_status() {
        let sarif = enforcement_decisions_to_sarif(&full_coverage_input());
        assert_sarif_has_no_unearned_status("sarif projection", &sarif, &[SCAN_TOOL]);
    }

    /// Status-like words in a tool name are upstream content, not an Assay
    /// claim. The projection must reflect them verbatim, and the guard must
    /// still pass on that document — otherwise it has become content
    /// censorship rather than a check on what Assay asserts.
    #[test]
    fn sarif_projection_reflects_status_like_tool_names_unchanged() {
        let recs = vec![deny(
            "no_declared_allowance",
            "certified_partner_export",
            "github_deploy_key",
            "not_evaluated",
        )];
        let sarif = enforcement_decisions_to_sarif(&recs);
        let result = &sarif["runs"][0]["results"][0];
        assert_eq!(
            result["message"]["text"],
            serde_json::json!(
                "Privileged tool action denied before forward: certified_partner_export (github_deploy_key) — no_declared_allowance"
            ),
            "deny message reflects the tool name byte-exactly"
        );
        assert_eq!(
            result["locations"][0]["logicalLocations"][0]["name"],
            serde_json::json!("certified_partner_export"),
            "logical location names the tool unchanged"
        );
        assert_sarif_has_no_unearned_status(
            "sarif projection with status-like tool name",
            &sarif,
            &["certified_partner_export"],
        );
    }

    /// Controls for the guard itself: a backstop never shown to reject
    /// anything proves nothing. Each injects an unearned word into a leaf the
    /// real producer copies into the document, so they pin the assertion; a
    /// mutation of the producer's own template strings is demonstrated against
    /// a /tmp copy at slice time, not in the tree.
    #[test]
    #[should_panic(expected = "asserts `certified`")]
    fn sarif_guard_rejects_an_unearned_word_in_the_projected_reason() {
        let recs = vec![deny(
            "certified_partner_runtime",
            "github.add_deploy_key",
            "github_deploy_key",
            "not_evaluated",
        )];
        let sarif = enforcement_decisions_to_sarif(&recs);
        assert_sarif_has_no_unearned_status("control", &sarif, &["github.add_deploy_key"]);
    }

    #[test]
    #[should_panic(expected = "asserts `certified`")]
    fn sarif_guard_rejects_an_unearned_word_in_the_projected_action_class() {
        let recs = vec![deny(
            "no_declared_allowance",
            "github.add_deploy_key",
            "certified_action",
            "not_evaluated",
        )];
        let sarif = enforcement_decisions_to_sarif(&recs);
        assert_sarif_has_no_unearned_status("control", &sarif, &["github.add_deploy_key"]);
    }
}
