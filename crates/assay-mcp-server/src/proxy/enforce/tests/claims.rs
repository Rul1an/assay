use super::fixtures::*;
use super::*;
use crate::claims_backstop::assert_no_unearned_status;
use assay_mcp_server::tool_decision::{
    classifier_complete_args, classifier_incomplete_args, classify, sanitize, CLASSIFIER_TOOLS,
    REFLECTED_TARGET_FIELDS,
};
use std::collections::BTreeSet;

// ---- ADR-043 §2 wire status claims on the POLICY DECISION CONTRACT (#2232) ------------------
//
// `decision_record` is Assay's own words reaching evidence and consumers: the `reason`,
// `decision`, `drift_state`, `action_class`/`verb`/`resource_type` leaves and the static
// `non_claims`. The generated record therefore gets the same closed-set backstop as the
// handshake, through the single implementation in `crate::claims_backstop` — the same file
// compiled into the library's test build — one list, one meaning.
//
// Value domain: exactly those Assay-authored leaves. The reflected operator/caller content —
// the caller id, the tool name, the credential alias, and the caller-supplied target sub-leaves
// (owner/repo/role) — is the caller's data, not Assay's assertion (the same reason the
// `tool_identity` guard pins reflection instead of scanning words, #3082, and the SARIF guard
// excises tool names, #3199), so it is excised per field before the scan and pinned byte-exact
// below instead. The REST of the target — provider labels, hashes, booleans, key names — is
// Assay-authored and stays scanned.

/// The Assay-authored leaves of a generated decision record: the reflected caller fields are
/// blanked whole (each field IS the reflection), and within `action.target` only the reflected
/// caller-supplied sub-leaves are blanked — the classifier's own [`REFLECTED_TARGET_FIELDS`]
/// list, so a new reflected field joins the excision automatically. Everything else (provider,
/// hashes, key names) is Assay's assertion and stays in the scanned document, never
/// document-wide blanked, so an identical word in any Assay-authored leaf still trips the
/// backstop.
fn assay_authored_leaves(record: &Value) -> Value {
    let mut scrubbed = record.clone();
    scrubbed["caller"]["id"] = Value::String(String::new());
    scrubbed["tool"]["name"] = Value::String(String::new());
    scrubbed["credential_alias"] = Value::String(String::new());
    if let Some(target) = scrubbed
        .get_mut("action")
        .and_then(|a| a.get_mut("target"))
        .and_then(|t| t.as_object_mut())
    {
        for field in REFLECTED_TARGET_FIELDS {
            if let Some(slot) = target.get_mut(*field) {
                *slot = Value::String(String::new());
            }
        }
    }
    scrubbed
}

/// The excised fields must still reflect their inputs byte-exactly: an Assay-added suffix is
/// detected by equality, not by a word list. The target must carry the classifier projection
/// unchanged (stated as consistency between the two emission points, not as hash correctness).
fn assert_reflection_byte_exact(
    case: &str,
    policy: &EnforcePolicy,
    tool: &str,
    args: &Value,
    record: &Value,
) {
    assert_eq!(
        record["tool"]["name"],
        sanitize(tool),
        "{case}: tool name must be reflected byte-exactly"
    );
    assert_eq!(
        record["caller"]["id"],
        sanitize(&policy.caller.id),
        "{case}: caller id must be reflected byte-exactly"
    );
    let expected_alias = policy
        .upstream_credential
        .as_ref()
        .map(|c| sanitize(&c.alias));
    assert_eq!(
        record["credential_alias"],
        json!(expected_alias),
        "{case}: credential alias must be reflected byte-exactly (or null)"
    );
    assert_eq!(
        record["action"]["target"],
        classify(tool, args).target,
        "{case}: target must carry the classifier projection unchanged"
    );
}

/// Every GENERATED decision record value, driven by the classifier's own [`CLASSIFIER_TOOLS`]
/// table crossed with the PDP gate outcomes — never by a hand list of tools or reasons.
///
/// For each table row the scan runs BOTH classification states (complete args must reach
/// `classified`, incomplete args must reach `classified_incomplete`) through the real `decide`
/// and the real `decision_record`, and asserts the record carries the row's own
/// category/verb/resource_type. A claims-only gate walk on one github leaf then steers the
/// remaining deny reasons plus allow (these inputs live here, not in the vendored golden
/// fixture), and one unclassified tool covers the `None` leaves. A new table row is scanned the
/// moment it is added — no second inventory to update.
///
/// This is what pins the record VALUES rather than the decide reasons: a mutant that smuggles
/// a word into any emitted value (`approved_slack_add_member`, `certified_workspace_role`,
/// `approved_add`, `partner_approval_required`, `github-certified`) flows through the real
/// emission points into a scanned document and fires the backstop.
#[test]
fn decision_records_assert_no_unearned_status() {
    let mut reasons: BTreeSet<&str> = BTreeSet::new();
    let mut drifts: BTreeSet<String> = BTreeSet::new();
    let mut cells = 0;
    let mut scan = |case: &str,
                    policy: &EnforcePolicy,
                    baseline: &DeclaredManifest,
                    observed: &ObservedToolDigest,
                    tool: &str,
                    args: &Value,
                    expected_reason: &str,
                    expected_class: Option<&str>,
                    expected_verb: Option<&str>,
                    expected_resource: Option<&str>| {
        let d = decide(policy, baseline, observed, tool, args);
        assert_eq!(
            d.reason, expected_reason,
            "{case}: matrix input must really exercise its cell"
        );
        let rec = decision_record(policy, &d, tool, args);
        assert_eq!(
            rec["tool"]["action_class"],
            json!(expected_class),
            "{case}: record must carry the matrix category"
        );
        assert_eq!(
            rec["action"]["verb"],
            json!(expected_verb),
            "{case}: record must carry the matrix verb"
        );
        assert_eq!(
            rec["action"]["resource_type"],
            json!(expected_resource),
            "{case}: record must carry the matrix resource_type"
        );
        assert_reflection_byte_exact(case, policy, tool, args, &rec);
        assert_no_unearned_status(
            &format!("decision record {case}"),
            &assay_authored_leaves(&rec),
        );
        reasons.insert(d.reason);
        drifts.insert(rec["drift_state"].as_str().unwrap().to_string());
        cells += 1;
    };

    // 1. Category x state matrix, iterated from the classifier's own table. Non-github
    // categories cannot clear the c1 allowance gate (its matcher only admits github_deploy_key),
    // so their classified cells read as no_declared_allowance — still the record carrying their
    // values, which is what the scan pins.
    let allow_policy = policy_from(VALID).unwrap();
    for entry in CLASSIFIER_TOOLS {
        let tool = format!("claims.{}", entry.leaf);
        let complete = classifier_complete_args(entry.category);
        let incomplete = classifier_incomplete_args(entry.category);
        // Parity at the source: the matrix input must really reach the state the cell claims.
        let c = classify(&tool, &complete);
        assert_eq!(c.state, "classified", "{}", entry.leaf);
        let c = classify(&tool, &incomplete);
        assert_eq!(c.state, "classified_incomplete", "{}", entry.leaf);
        if entry.category == "github_deploy_key" {
            scan(
                &format!("matrix {} classified", entry.leaf),
                &allow_policy,
                &baseline_with(&tool, APPROVED),
                &matching_observed(),
                &tool,
                &complete,
                "allow",
                Some(entry.category),
                Some(entry.verb),
                Some(entry.resource_type),
            );
        } else {
            scan(
                &format!("matrix {} classified", entry.leaf),
                &allow_policy,
                &matching_baseline(),
                &matching_observed(),
                &tool,
                &complete,
                "no_declared_allowance",
                Some(entry.category),
                Some(entry.verb),
                Some(entry.resource_type),
            );
        }
        scan(
            &format!("matrix {} incomplete", entry.leaf),
            &allow_policy,
            &matching_baseline(),
            &matching_observed(),
            &tool,
            &incomplete,
            "classification_incomplete",
            Some(entry.category),
            Some(entry.verb),
            Some(entry.resource_type),
        );
    }

    // 2. Claims-only gate walk: every remaining PDP outcome on one github leaf. (Allow is covered
    // by the matrix cells above; the two incomplete-observation variants share one reason.)
    let walk_tool = "claims.add_deploy_key";
    let walk_args = classifier_complete_args("github_deploy_key");
    let ro_cred = "upstream_credential:\n  alias: \"gh-ro\"\n  scopes: [\"repo:read\"]\n";
    let walk_rows: Vec<(
        &str,
        EnforcePolicy,
        DeclaredManifest,
        ObservedToolDigest,
        Value,
        &str,
    )> = vec![
        (
            "walk no_declared_allowance",
            allow_policy,
            baseline_with(walk_tool, APPROVED),
            matching_observed(),
            json!({"owner": "claims-other", "repo": "claims-app"}),
            "no_declared_allowance",
        ),
        (
            "walk credential_scope_unknown",
            allow_acme_with_cred(""),
            baseline_with(walk_tool, APPROVED),
            matching_observed(),
            walk_args.clone(),
            "credential_scope_unknown",
        ),
        (
            "walk credential_scope_insufficient",
            allow_acme_with_cred(ro_cred),
            baseline_with(walk_tool, APPROVED),
            matching_observed(),
            walk_args.clone(),
            "credential_scope_insufficient",
        ),
        (
            "walk manifest_baseline_missing",
            policy_from(VALID).unwrap(),
            baseline_with("claims.other_tool", APPROVED),
            matching_observed(),
            walk_args.clone(),
            "manifest_baseline_missing",
        ),
        (
            "walk manifest_current_observation_incomplete",
            policy_from(VALID).unwrap(),
            baseline_with(walk_tool, APPROVED),
            ObservedToolDigest::NoCompleteManifest,
            walk_args.clone(),
            "manifest_current_observation_incomplete",
        ),
        (
            "walk manifest_current_observation_incomplete_tool_absent",
            policy_from(VALID).unwrap(),
            baseline_with(walk_tool, APPROVED),
            ObservedToolDigest::CompleteButToolAbsent,
            walk_args.clone(),
            "manifest_current_observation_incomplete",
        ),
        (
            "walk manifest_observation_ambiguous",
            policy_from(VALID).unwrap(),
            baseline_with(walk_tool, APPROVED),
            ObservedToolDigest::Ambiguous,
            walk_args.clone(),
            "manifest_observation_ambiguous",
        ),
        (
            "walk manifest_drifted_since_approval",
            policy_from(VALID).unwrap(),
            baseline_with(walk_tool, APPROVED),
            ObservedToolDigest::Present("sha256:something-else".to_string()),
            walk_args.clone(),
            "manifest_drifted_since_approval",
        ),
    ];
    for (case, policy, baseline, observed, args, reason) in &walk_rows {
        scan(
            case,
            policy,
            baseline,
            observed,
            walk_tool,
            args,
            reason,
            Some("github_deploy_key"),
            Some("create"),
            Some("github_deploy_key"),
        );
    }

    // 3. Unclassified tool: the None leaves.
    let unclassified_policy = policy_from(VALID).unwrap();
    scan(
        "matrix unclassified",
        &unclassified_policy,
        &matching_baseline(),
        &matching_observed(),
        "claims.do_thing",
        &json!({}),
        "unclassified_tool_call",
        None,
        None,
        None,
    );

    // Inventory: every PDP outcome and every drift state must have been scanned. A new gate
    // outcome must name itself here to join the scan — but the VALUES it can smuggle are already
    // scanned the moment its category row exists in CLASSIFIER_TOOLS.
    assert_eq!(
        cells,
        CLASSIFIER_TOOLS.len() * 2 + walk_rows.len() + 1,
        "every matrix cell must be scanned"
    );
    let expected_reasons: BTreeSet<&str> = [
        "unclassified_tool_call",
        "classification_incomplete",
        "no_declared_allowance",
        "credential_scope_unknown",
        "credential_scope_insufficient",
        "manifest_baseline_missing",
        "manifest_current_observation_incomplete",
        "manifest_observation_ambiguous",
        "manifest_drifted_since_approval",
        "allow",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        reasons, expected_reasons,
        "a new decision branch must join the scan explicitly"
    );
    let expected_drifts: BTreeSet<String> = [
        "not_evaluated",
        "baseline_missing",
        "current_observation_incomplete",
        "observation_ambiguous",
        "drifted",
        "satisfied",
    ]
    .map(String::from)
    .into_iter()
    .collect();
    assert_eq!(drifts, expected_drifts, "every drift state must be scanned");
}

/// Policy whose reflected caller fields carry status-like words: operator data the record must
/// reflect verbatim, never Assay claims.
const HOSTILE_CALLER: &str = r#"
caller:
  id: "approved-operator"
upstream_credential:
  alias: "certified-upstream"
  scopes: ["repo:deploy_key:write"]
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

/// Status-like words in reflected content are upstream/operator data, not an Assay claim. The
/// record must reflect them verbatim — caller id, tool name, credential alias, and every
/// reflected target sub-leaf — and the guard must still pass on those documents, otherwise it
/// has become content censorship rather than a check on what Assay asserts.
#[test]
fn decision_record_reflects_hostile_caller_content_unchanged() {
    let policy = policy_from(HOSTILE_CALLER).unwrap();
    // Status-like tool name on an unclassified tool: reflected, never classified.
    let tool = "certified_partner_export";
    let d = decide_match(&policy, tool, &json!({}));
    assert_eq!(d.reason, "unclassified_tool_call");
    let rec = decision_record(&policy, &d, tool, &json!({}));
    assert_reflection_byte_exact("status-like tool name", &policy, tool, &json!({}), &rec);
    assert_no_unearned_status(
        "decision record with status-like tool name",
        &assay_authored_leaves(&rec),
    );
    // Status-like github target sub-leaves: the allowance gate denies, and the projected target
    // still reflects the supplied owner/repo byte-exactly.
    let args = json!({"owner": "approved-acme", "repo": "compliant-repo"});
    let d = decide(
        &policy,
        &matching_baseline(),
        &matching_observed(),
        TOOL,
        &args,
    );
    assert_eq!(d.reason, "no_declared_allowance");
    let rec = decision_record(&policy, &d, TOOL, &args);
    assert_eq!(rec["action"]["target"]["owner"], "approved-acme");
    assert_eq!(rec["action"]["target"]["repo"], "compliant-repo");
    assert_reflection_byte_exact("status-like target owner/repo", &policy, TOOL, &args, &rec);
    assert_no_unearned_status(
        "decision record with status-like target owner/repo",
        &assay_authored_leaves(&rec),
    );
    // Status-like workspace role: reflected as the plain role label, byte-exact.
    let ws_tool = "claims.grant_admin";
    let ws_args = json!({"workspace_id": "acme", "principal": "p", "role": "endorsed-admin"});
    let d = decide(
        &policy,
        &matching_baseline(),
        &matching_observed(),
        ws_tool,
        &ws_args,
    );
    assert_eq!(d.reason, "no_declared_allowance");
    let rec = decision_record(&policy, &d, ws_tool, &ws_args);
    assert_eq!(rec["action"]["target"]["role"], "endorsed-admin");
    assert_reflection_byte_exact("status-like target role", &policy, ws_tool, &ws_args, &rec);
    assert_no_unearned_status(
        "decision record with status-like target role",
        &assay_authored_leaves(&rec),
    );
}

/// One control per excised field: restoring exactly that field into the excised document must
/// fire the guard. Deleting any excision — mutant K deleted the caller.id AND credential_alias
/// excisions and stayed green — turns this test red while the acceptance above keeps passing.
#[test]
fn each_reflection_excision_is_load_bearing() {
    let policy = policy_from(HOSTILE_CALLER).unwrap();
    let github_args = json!({"owner": "approved-acme", "repo": "compliant-repo"});
    let github_decision = decide(
        &policy,
        &matching_baseline(),
        &matching_observed(),
        TOOL,
        &github_args,
    );
    let github_rec = decision_record(&policy, &github_decision, TOOL, &github_args);
    let ws_tool = "claims.grant_admin";
    let ws_args = json!({"workspace_id": "acme", "principal": "p", "role": "endorsed-admin"});
    let ws_decision = decide(
        &policy,
        &matching_baseline(),
        &matching_observed(),
        ws_tool,
        &ws_args,
    );
    let ws_rec = decision_record(&policy, &ws_decision, ws_tool, &ws_args);
    let un_tool = "certified_partner_export";
    let un_decision = decide_match(&policy, un_tool, &json!({}));
    let un_rec = decision_record(&policy, &un_decision, un_tool, &json!({}));
    let controls = [
        (
            "github target owner",
            &github_rec,
            "/action/target/owner",
            "approved",
        ),
        (
            "github target repo",
            &github_rec,
            "/action/target/repo",
            "compliant",
        ),
        (
            "workspace target role",
            &ws_rec,
            "/action/target/role",
            "endorsed",
        ),
        ("caller id", &github_rec, "/caller/id", "approved"),
        (
            "credential alias",
            &github_rec,
            "/credential_alias",
            "certified",
        ),
        ("tool name", &un_rec, "/tool/name", "certified"),
    ];
    for (label, rec, pointer, word) in controls {
        let mut scrubbed = assay_authored_leaves(rec);
        let restored = rec.pointer(pointer).unwrap().clone();
        assert!(
            !restored.as_str().unwrap().is_empty(),
            "{label}: hostile fixture must carry a word at {pointer}"
        );
        *scrubbed.pointer_mut(pointer).unwrap() = restored;
        let result = std::panic::catch_unwind(|| {
            assert_no_unearned_status(&format!("control {label}"), &scrubbed);
        });
        assert!(
            result.is_err(),
            "{label}: removing the {pointer} excision must turn the guard red"
        );
        let payload = result.unwrap_err();
        let msg = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(
            msg.contains(word),
            "{label}: guard must fire on `{word}`, got: {msg}"
        );
    }
}

/// Control for the excision as a whole: the same status-like record scanned WITHOUT excising
/// the reflection must fire — otherwise the excision would be dead code and the acceptance
/// cases would prove nothing.
#[test]
#[should_panic(expected = "asserts `certified`")]
fn decision_record_guard_fires_on_unexcised_reflection() {
    let policy = policy_from(HOSTILE_CALLER).unwrap();
    let tool = "certified_partner_export";
    let d = decide_match(&policy, tool, &json!({}));
    let rec = decision_record(&policy, &d, tool, &json!({}));
    assert_no_unearned_status("control", &rec);
}
