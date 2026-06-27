use super::*;

#[test]
fn declared_read_only_true_mismatches_observed_mutating() {
    let rec = conformance_for(
        &DeclaredToolAnnotations {
            read_only: Some(true),
            destructive: None,
            idempotent: None,
            open_world: None,
        },
        "github.add_deploy_key",
        &github_args(),
    );

    assert_eq!(rec["schema"], json!(TOOL_ANNOTATION_CONFORMANCE_SCHEMA));
    assert_eq!(rec["conformance"], json!("mismatched"));
    assert_eq!(
        rec["mismatch_kind"],
        json!("declared_read_only_observed_mutating")
    );
    assert_eq!(rec["assessed_axes"], json!(["read_only"]));
}

#[test]
fn declared_non_destructive_mismatches_observed_destructive() {
    let rec = conformance_for(
        &DeclaredToolAnnotations {
            read_only: Some(false),
            destructive: Some(false),
            idempotent: None,
            open_world: None,
        },
        "workspace.modify_org_policy",
        &workspace_args(),
    );

    assert_eq!(rec["conformance"], json!("mismatched"));
    assert_eq!(
        rec["mismatch_kind"],
        json!("declared_non_destructive_observed_destructive")
    );
    assert_eq!(rec["observed"]["behavior_class"], json!("destructive"));
}

#[test]
fn compatible_assessed_hints_are_consistent_not_certified() {
    let rec = conformance_for(
        &DeclaredToolAnnotations {
            read_only: Some(false),
            destructive: Some(false),
            idempotent: None,
            open_world: None,
        },
        "github.add_deploy_key",
        &github_args(),
    );

    assert_eq!(rec["conformance"], json!("consistent"));
    assert_eq!(rec["mismatch_kind"], Value::Null);
    assert_eq!(rec["observed"]["behavior_class"], json!("mutating"));
    assert!(rec["non_claims"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| { v.as_str().unwrap().contains("consistent does not certify") }));
}

#[test]
fn missing_assessed_hints_are_undeclared_not_consistent() {
    let rec = conformance_for(
        &DeclaredToolAnnotations {
            read_only: None,
            destructive: None,
            idempotent: Some(true),
            open_world: Some(false),
        },
        "github.add_deploy_key",
        &github_args(),
    );

    assert_eq!(rec["conformance"], json!("undeclared"));
    assert_eq!(rec["assessed_axes"], json!([]));
    assert_eq!(rec["declared"]["idempotent"], json!(true));
    assert_eq!(rec["declared"]["open_world"], json!(false));
    assert_eq!(rec["unassessed_axes"], json!(["idempotent", "open_world"]));
}

#[test]
fn unclassified_or_incomplete_calls_are_inconclusive() {
    for (tool, args) in [
        ("unknown.tool", json!({})),
        ("github.add_deploy_key", json!({"owner": "acme"})),
    ] {
        let rec = conformance_for(
            &DeclaredToolAnnotations {
                read_only: Some(true),
                destructive: Some(false),
                idempotent: None,
                open_world: None,
            },
            tool,
            &args,
        );

        assert_eq!(rec["conformance"], json!("inconclusive"));
        assert_eq!(rec["mismatch_kind"], Value::Null);
        assert_eq!(rec["assessed_axes"], json!([]));
    }
}

#[test]
fn record_has_no_verdict_delivery_or_sensitive_identity_fields() {
    let rec = conformance_for(
        &DeclaredToolAnnotations {
            read_only: Some(false),
            destructive: Some(false),
            idempotent: None,
            open_world: None,
        },
        "github.add_deploy_key",
        &github_args(),
    );

    let text = serde_json::to_string(&rec).unwrap();
    for forbidden in [
        "decision",
        "reason",
        "forwarded",
        "delivered",
        "credential_alias",
        "scopes",
        "target_digest",
        "caller_id",
    ] {
        assert!(
            rec.get(forbidden).is_none(),
            "annotation conformance record must not carry field {forbidden}"
        );
    }
    assert!(
        !text.contains("ci-key"),
        "raw sensitive argument values must not be copied into the record"
    );
}

#[test]
fn incomplete_basis_forces_inconclusive_with_null_declared_and_digest() {
    // Declared hints and digest are nulled (annotations were not observed), but the classifier's
    // observed behavior is still recorded; conformance is forced inconclusive, never undeclared.
    let rec = build_tool_annotation_conformance_record(
        ObservationBasis::Incomplete,
        &DeclaredToolAnnotations {
            read_only: Some(true),
            destructive: Some(false),
            idempotent: None,
            open_world: None,
        },
        "github.add_deploy_key",
        Some("sha256:ignored-when-incomplete"),
        &json!({"owner": "acme", "repo": "prod-app"}),
    );
    assert_eq!(rec["observation_basis"], json!("incomplete"));
    assert_eq!(rec["conformance"], json!("inconclusive"));
    assert_eq!(rec["mismatch_kind"], Value::Null);
    assert_eq!(rec["assessed_axes"], json!([]));
    assert_eq!(rec["tool"]["tool_digest"], Value::Null);
    assert_eq!(rec["declared"]["read_only"], Value::Null);
    assert_eq!(rec["declared"]["destructive"], Value::Null);
    assert_eq!(rec["observed"]["behavior_class"], json!("mutating"));
}

#[test]
fn complete_basis_records_observation_basis_and_digest() {
    let rec = build_tool_annotation_conformance_record(
        ObservationBasis::Complete,
        &DeclaredToolAnnotations {
            read_only: Some(true),
            destructive: None,
            idempotent: None,
            open_world: None,
        },
        "github.add_deploy_key",
        Some("sha256:abc"),
        &json!({"owner": "acme", "repo": "prod-app"}),
    );
    assert_eq!(rec["observation_basis"], json!("complete"));
    assert_eq!(rec["tool"]["tool_digest"], json!("sha256:abc"));
    assert_eq!(rec["conformance"], json!("mismatched"));
}

#[test]
fn every_classifier_verb_maps_to_an_observed_behavior() {
    // observed_behavior() must stay in sync with the verbs tool_decision::classify can emit.
    // classify currently emits only privileged mutating/destructive verbs, so each must map; a
    // new privileged verb added there without a mapping would silently downgrade a contradiction
    // to inconclusive. A non-mutating verb, if ever added, maps to None by design.
    let calls = [
        (
            "github.add_deploy_key",
            json!({"owner": "acme", "repo": "prod-app"}),
        ),
        (
            "slack.add_member",
            json!({"workspace_id": "acme", "user_id": "u1"}),
        ),
        (
            "workspace.grant_admin",
            json!({"workspace_id": "acme", "principal": "p"}),
        ),
        (
            "workspace.change_role",
            json!({"workspace_id": "acme", "principal": "p"}),
        ),
        (
            "workspace.invite_external",
            json!({"workspace_id": "acme", "principal": "p"}),
        ),
        (
            "workspace.modify_org_policy",
            json!({"workspace_id": "acme", "principal": "p"}),
        ),
        (
            "workspace.create_workspace_token",
            json!({"workspace_id": "acme", "principal": "p"}),
        ),
    ];
    for (tool, args) in calls {
        let classified = classify(tool, &args);
        assert_eq!(classified.state, "classified", "{tool} should classify");
        assert!(
            observed_behavior(classified.verb).is_some(),
            "verb {:?} from {tool} has no observed_behavior mapping",
            classified.verb
        );
    }
}
