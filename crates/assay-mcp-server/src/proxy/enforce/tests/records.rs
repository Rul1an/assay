use super::fixtures::*;
use super::*;
use crate::proxy::establish::{
    self, build_manifest_establish_record, EstablishPath, RUN_OUTCOME_NOT_PERFORMED,
};

// --- P61e-d: enforcement_decision.v0 record ----------------------------------------------------

#[test]
fn decision_record_for_a_deny_is_shaped_and_leak_free() {
    let p = policy_from(VALID).unwrap();
    let d = decide_match(&p, "misc.do_thing", &json!({})); // unclassified -> deny
    let rec = decision_record(&p, &d, "misc.do_thing", &json!({}));
    assert_eq!(rec["schema"], "assay.enforcement_decision.v0");
    assert_eq!(rec["decision"], "deny");
    assert_eq!(rec["reason"], "unclassified_tool_call");
    assert_eq!(rec["fail_closed"], true);
    assert_eq!(rec["drift_state"], "not_evaluated");
    assert_eq!(rec["caller"]["id"], "ci-agent");
    assert_eq!(rec["credential_alias"], "gh-deploy");
    assert!(rec["non_claims"].is_array());
    // The record carries no transport-outcome field — it must not claim delivery.
    assert!(
        rec.get("forwarded").is_none(),
        "no transport claim in the decision record"
    );
    // The declared scopes are never serialized into the record (alias only).
    let s = serde_json::to_string(&rec).unwrap();
    assert!(
        !s.contains("repo:deploy_key:write"),
        "declared credential scopes must not leak into the decision record"
    );
}

#[test]
fn decision_record_for_an_allow_is_policy_decision_not_a_delivery_claim() {
    let p = policy_from(VALID).unwrap();
    let d = decide_match(&p, "github.add_deploy_key", &acme_call()); // matching -> allow
    assert!(d.allow);
    let rec = decision_record(&p, &d, "github.add_deploy_key", &acme_call());
    assert_eq!(rec["decision"], "allow");
    assert_eq!(rec["reason"], "allow");
    assert_eq!(rec["fail_closed"], false);
    assert_eq!(rec["drift_state"], "satisfied");
    assert_eq!(rec["tool"]["action_class"], "github_deploy_key");
    assert_eq!(rec["action"]["target"]["owner"], "acme");
    // The decision (allow) is the durable fact; the record never asserts the call was delivered.
    assert!(
        rec.get("forwarded").is_none(),
        "an allow decision must not be a transport/delivery claim"
    );
}

#[test]
fn decision_record_drift_state_reflects_the_drift_gate() {
    let p = policy_from(VALID).unwrap();
    let baseline = baseline_with(TOOL, APPROVED);
    let d = decide(
        &p,
        &baseline,
        &ObservedToolDigest::Present("sha256:something-else".to_string()),
        "github.add_deploy_key",
        &acme_call(),
    );
    assert_eq!(d.reason, "manifest_drifted_since_approval");
    let rec = decision_record(&p, &d, "github.add_deploy_key", &acme_call());
    assert_eq!(rec["decision"], "deny");
    assert_eq!(rec["drift_state"], "drifted");
}

// ---- Shared producer/consumer contract fixture (Rul1an/plimsoll#45) ------------------------
//
// The canonical `assay.enforcement_decision.v0` contract is the REAL output of `decision_record`,
// not a hand-authored mirror. This test regenerates one record per distinct producer outcome (the
// allow row plus every deny reason, with real `target_digest` and the full `non_claims`) and
// asserts it equals the committed fixture as a serde_json::Value (order-independent). Plimsoll
// vendors the SAME file and asserts its consumer accepts every record, so neither side can drift.
// Regenerate after an intentional producer change: ASSAY_UPDATE_GOLDEN=1 cargo test -p
// assay-mcp-server --bins pdp_golden_contract_fixture.

fn contract_fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/enforcement_decision_contract.v0.json")
}

/// One real producer record per distinct decision outcome (deduped by reason; the two
/// current_observation_incomplete cases collapse to one identical record).
fn contract_records() -> Vec<Value> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for c in golden_corpus() {
        let d = decide(&c.policy, &c.baseline, &c.observed, c.tool, &c.args);
        if !seen.insert(d.reason) {
            continue; // one canonical record per reason
        }
        let rec = decision_record(&c.policy, &d, c.tool, &c.args);
        out.push(json!({ "case": c.name, "record": rec }));
    }
    out
}

fn contract_document() -> Value {
    json!({
        "schema_contract": "assay.enforcement_decision.v0",
        "generated_by": "assay crates/assay-mcp-server enforce::decision_record (pdp_golden_contract_fixture)",
        "note": "Canonical producer output, regenerated from decision_record. Consumers (e.g. Rul1an/plimsoll#45) vendor this file verbatim. Regenerate with ASSAY_UPDATE_GOLDEN=1.",
        "records": contract_records(),
    })
}

#[test]
fn pdp_golden_contract_fixture() {
    let generated = contract_document();
    let path = contract_fixture_path();

    if std::env::var("ASSAY_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let pretty = serde_json::to_string_pretty(&generated).unwrap();
        std::fs::write(&path, format!("{pretty}\n")).unwrap();
    }

    let committed_text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing {}; regenerate with ASSAY_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    let committed: Value = serde_json::from_str(&committed_text).unwrap();
    assert_eq!(
        committed, generated,
        "the committed contract fixture is stale; regenerate with ASSAY_UPDATE_GOLDEN=1"
    );

    // Sanity: every record is the v0 carrier, carries no `forwarded` field, and references the
    // credential by alias only (no scopes key) — the discipline the consumer relies on.
    let records = generated["records"].as_array().unwrap();
    assert!(!records.is_empty());
    for entry in records {
        let rec = &entry["record"];
        assert_eq!(rec["schema"], "assay.enforcement_decision.v0");
        assert!(
            rec.get("forwarded").is_none(),
            "no record may carry a forwarded field"
        );
        let alias = &rec["credential_alias"];
        assert!(
            alias.is_null() || alias.is_string(),
            "credential_alias is alias-or-null"
        );
        assert!(
            rec.get("scopes").is_none(),
            "a record must not carry a scopes key"
        );
    }
}

// ---- Combined carrier acceptance fixture (Increment 4) -------------------------------------
//
// This fixture pairs the REAL producer output of `assay.enforcement_decision.v0` and
// `assay.manifest_establish.v0` for canonical establish journeys. It is deliberately a consumer
// acceptance fixture, not a new carrier: consumers should read the verdict from
// `enforcement_decision` and the journey from `manifest_establish`, never infer one from the
// other. Regenerate after an intentional producer change: ASSAY_UPDATE_GOLDEN=1 cargo test -p
// assay-mcp-server --bins combined_carrier_acceptance_fixture.

fn combined_fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/combined_carrier_acceptance.v0.json")
}

struct CombinedCase {
    name: &'static str,
    policy: EnforcePolicy,
    baseline: DeclaredManifest,
    observed: ObservedToolDigest,
    tool: &'static str,
    args: Value,
    establish_path: EstablishPath,
    run_outcome: &'static str,
    note: &'static str,
}

fn combined_cases() -> Vec<CombinedCase> {
    vec![
        CombinedCase {
            name: "no_establish_needed_allow",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: matching_observed(),
            tool: TOOL,
            args: acme_call(),
            establish_path: EstablishPath::NoEstablishNeeded,
            run_outcome: RUN_OUTCOME_NOT_PERFORMED,
            note: "current complete observation already existed; verdict carrier allows independently",
        },
        CombinedCase {
            name: "established_then_allowed",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: matching_observed(),
            tool: TOOL,
            args: acme_call(),
            establish_path: EstablishPath::EstablishedThenAllowed,
            run_outcome: "complete",
            note: "establish produced a complete current observation and the re-decided call allowed",
        },
        CombinedCase {
            name: "established_then_denied_tool_absent",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: ObservedToolDigest::CompleteButToolAbsent,
            tool: TOOL,
            args: acme_call(),
            establish_path: EstablishPath::EstablishedThenDenied,
            run_outcome: "complete",
            note: "establish completed but the tool remained absent; verdict carrier denies",
        },
        CombinedCase {
            name: "establish_timed_out_immediate_deny",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: ObservedToolDigest::NoCompleteManifest,
            tool: TOOL,
            args: acme_call(),
            establish_path: EstablishPath::ImmediateDeny,
            run_outcome: "timed_out",
            note: "establish was attempted but failed to complete; original fail-closed verdict stands",
        },
        CombinedCase {
            name: "ambiguous_immediate_deny_no_establish",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: ObservedToolDigest::Ambiguous,
            tool: TOOL,
            args: acme_call(),
            establish_path: EstablishPath::ImmediateDeny,
            run_outcome: RUN_OUTCOME_NOT_PERFORMED,
            note: "ambiguous observation is denied without establish; establish cannot resolve ambiguity",
        },
        CombinedCase {
            name: "baseline_missing_immediate_deny_no_establish",
            policy: policy_from(VALID).unwrap(),
            baseline: baseline_with("github.other_tool", APPROVED),
            observed: matching_observed(),
            tool: TOOL,
            args: acme_call(),
            establish_path: EstablishPath::ImmediateDeny,
            run_outcome: RUN_OUTCOME_NOT_PERFORMED,
            note: "establish only supplies current observation; it never supplies a missing baseline",
        },
        CombinedCase {
            name: "drifted_immediate_deny_no_establish",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: ObservedToolDigest::Present("sha256:something-else".to_string()),
            tool: TOOL,
            args: acme_call(),
            establish_path: EstablishPath::ImmediateDeny,
            run_outcome: RUN_OUTCOME_NOT_PERFORMED,
            note: "establish cannot clear real digest drift; the drift verdict stands",
        },
    ]
}

fn combined_record(c: &CombinedCase) -> Value {
    let decision = decide(&c.policy, &c.baseline, &c.observed, c.tool, &c.args);
    json!({
        "case": c.name,
        "note": c.note,
        "enforcement_decision": decision_record(&c.policy, &decision, c.tool, &c.args),
        "manifest_establish": build_manifest_establish_record(
            c.establish_path,
            decision.action_class.as_deref(),
            c.run_outcome,
        ),
    })
}

fn combined_document() -> Value {
    let records: Vec<Value> = combined_cases().iter().map(combined_record).collect();
    let mismatch_decision_case = CombinedCase {
        name: "consumer_negative_control_established_then_allowed_with_deny_verdict",
        policy: policy_from(VALID).unwrap(),
        baseline: matching_baseline(),
        observed: ObservedToolDigest::NoCompleteManifest,
        tool: TOOL,
        args: acme_call(),
        establish_path: EstablishPath::EstablishedThenAllowed,
        run_outcome: "complete",
        note: "consumer-only negative control: records are individually valid, but the journey must not be promoted into a verdict",
    };
    json!({
        "schema_contract": "assay.combined_carrier_acceptance.v0",
        "generated_by": "assay crates/assay-mcp-server enforce::decision_record + proxy::establish::build_manifest_establish_record (combined_carrier_acceptance_fixture)",
        "note": "Combined acceptance fixture for consumers. Verdict lives only in enforcement_decision; manifest_establish is journey/diagnostic only. Consumer negative controls are intentionally not live producer scenarios.",
        "records": records,
        "consumer_negative_controls": [
            combined_record(&mismatch_decision_case),
        ],
    })
}

#[test]
fn combined_carrier_acceptance_fixture() {
    let generated = combined_document();
    let path = combined_fixture_path();

    if std::env::var("ASSAY_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let pretty = serde_json::to_string_pretty(&generated).unwrap();
        std::fs::write(&path, format!("{pretty}\n")).unwrap();
    }

    let committed_text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing {}; regenerate with ASSAY_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    let committed: Value = serde_json::from_str(&committed_text).unwrap();
    assert_eq!(
        committed, generated,
        "the committed combined-carrier fixture is stale; regenerate with ASSAY_UPDATE_GOLDEN=1"
    );

    let records = generated["records"].as_array().unwrap();
    assert_eq!(records.len(), 7);
    for entry in records
        .iter()
        .chain(generated["consumer_negative_controls"].as_array().unwrap())
    {
        let decision = &entry["enforcement_decision"];
        let establish = &entry["manifest_establish"];
        assert_eq!(decision["schema"], "assay.enforcement_decision.v0");
        assert_eq!(establish["schema"], establish::MANIFEST_ESTABLISH_SCHEMA);
        assert!(
            decision.get("forwarded").is_none(),
            "{}: decision record must not claim delivery",
            entry["case"]
        );
        assert!(
            establish.get("decision").is_none() && establish.get("reason").is_none(),
            "{}: establish carrier must not carry verdict fields",
            entry["case"]
        );
        let decision_text = serde_json::to_string(decision).unwrap();
        assert!(
            !decision_text.contains("repo:deploy_key:write"),
            "{}: declared credential scopes must not leak",
            entry["case"]
        );
        let establish_text = serde_json::to_string(establish).unwrap();
        for forbidden in [
            "target_digest",
            "scope",
            "token",
            "credential",
            "caller",
            "owner",
            "repo",
        ] {
            assert!(
                !establish_text.contains(forbidden),
                "{}: manifest_establish must stay journey-only and omit `{forbidden}`",
                entry["case"]
            );
        }
    }
}
