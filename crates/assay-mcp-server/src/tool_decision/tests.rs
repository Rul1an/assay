use super::*;

fn call<'a>(tool: &'a str, args: &'a Value, effect: Effect, status: &'a str) -> ObservedCall<'a> {
    ObservedCall {
        server_id: "github",
        tool_name: tool,
        args,
        effect,
        status,
        rule_id: Some("r1"),
    }
}

#[test]
fn allowed_success_asserts_but_never_verifies_the_side_effect() {
    let a = json!({});
    let d = build_decision(&call("github.add_deploy_key", &a, Effect::Allow, "success"));
    assert_eq!(d["response"]["side_effect_asserted"], json!(true));
    assert_eq!(d["response"]["side_effect_verified"], json!(false));
}

#[test]
fn denied_call_asserts_no_side_effect() {
    let a = json!({"owner": "org", "repo": "prod-repo"});
    let d = build_decision(&call("github.add_deploy_key", &a, Effect::Deny, "blocked"));
    assert_eq!(d["response"]["side_effect_asserted"], json!(false));
    assert_eq!(d["response"]["side_effect_verified"], json!(false));
}

#[test]
fn unclassified_tool_is_observed_unknown_never_clean() {
    let a = json!({});
    let d = build_decision(&call("misc.do_thing", &a, Effect::Allow, "success"));
    assert_eq!(d["classification"], json!("observed_unknown_tool"));
    assert_eq!(d["reason_code"], json!("unknown_tool_name"));
    assert_eq!(d["action"]["class"], json!("unclassified"));
}

#[test]
fn hostile_strings_are_sanitized() {
    let a = json!({});
    let hostile = "tool\u{1b}[31m\u{0000}";
    let d = build_decision(&call(hostile, &a, Effect::Allow, "success"));
    let name = d["tool"]["name"].as_str().unwrap();
    assert!(
        !name.contains('\u{1b}') && !name.contains('\u{0000}'),
        "control chars sanitized"
    );
    assert!(name.contains('\u{FFFD}'));
}

#[test]
fn surface_carries_schema_and_non_claims() {
    let a = json!({});
    let s = surface(vec![build_decision(&call(
        "t",
        &a,
        Effect::Allow,
        "success",
    ))]);
    assert_eq!(s["schema"], json!(SCHEMA));
    assert!(s["non_claims"]
        .as_array()
        .map(|a| !a.is_empty())
        .unwrap_or(false));
    assert_eq!(s["observed_tool_decisions"].as_array().unwrap().len(), 1);
}

#[test]
fn github_deploy_key_classified_projects_owner_repo_and_hashed_title() {
    let a = json!({"owner": "org", "repo": "prod-repo", "title": "ci-key", "read_only": true,
                   "public_key": "ssh-ed25519 AAAA...", "token": "ghp_secret"});
    let d = build_decision(&call("github.add_deploy_key", &a, Effect::Allow, "success"));
    assert_eq!(d["classification"], json!("classified"));
    assert_eq!(d["reason_code"], json!("classified_github_deploy_key"));
    assert_eq!(d["tool"]["category"], json!("github_deploy_key"));
    let t = &d["action"]["target"];
    assert_eq!(t["owner"], json!("org"));
    assert_eq!(t["repo"], json!("prod-repo"));
    assert_eq!(t["read_only"], json!(true));
    assert_eq!(
        t["key_title_hash"],
        json!(target_hash("github_key_title", "ci-key"))
    );

    let text = serde_json::to_string(&d).unwrap();
    assert!(
        observed_secret_arg(&a),
        "fixture must include a secret-like arg"
    );
    for raw in ["ssh-ed25519", "AAAA", "ghp_secret", "ci-key"] {
        assert!(
            !text.contains(raw),
            "raw value {raw} must not appear in the record"
        );
    }
}

#[test]
fn github_deploy_key_missing_repo_is_incomplete() {
    let a = json!({"owner": "org"});
    let d = build_decision(&call("github.add_deploy_key", &a, Effect::Allow, "success"));
    assert_eq!(d["classification"], json!("classified_incomplete"));
    assert_eq!(d["reason_code"], json!("missing_required_target_field"));
    assert_eq!(d["detail"], json!("missing_github_owner_or_repo"));
    assert_eq!(d["action"]["target"]["owner"], json!("org"));
    assert!(d["action"]["target"].get("repo").is_none());
}

#[test]
fn slack_add_member_classified_hashes_all_ids() {
    let a = json!({"workspace_id": "T0123", "channel_id": "C0123", "user": "alice@example.com"});
    let d = build_decision(&call("slack.add_member", &a, Effect::Allow, "success"));
    assert_eq!(d["classification"], json!("classified"));
    assert_eq!(d["reason_code"], json!("classified_slack_add_member"));
    assert_eq!(d["action"]["resource_type"], json!("workspace_member"));
    let t = &d["action"]["target"];
    assert_eq!(
        t["workspace_id_hash"],
        json!(target_hash("slack_workspace", "T0123"))
    );
    assert_eq!(
        t["channel_id_hash"],
        json!(target_hash("slack_channel", "C0123"))
    );
    assert_eq!(
        t["principal_hash"],
        json!(target_hash("slack_principal", "alice@example.com"))
    );
    let text = serde_json::to_string(&d).unwrap();
    assert!(
        !text.contains("alice@example.com"),
        "raw email must not appear"
    );
}

#[test]
fn slack_workspace_level_membership_has_null_channel() {
    let a = json!({"workspace_id": "T0123", "user_id": "U999"});
    let d = build_decision(&call("slack.add_member", &a, Effect::Allow, "success"));
    assert_eq!(d["classification"], json!("classified"));
    assert_eq!(d["action"]["target"]["channel_id_hash"], json!(null));
}

#[test]
fn slack_missing_principal_is_incomplete() {
    let a = json!({"workspace_id": "T0123"});
    let d = build_decision(&call("slack.add_member", &a, Effect::Allow, "success"));
    assert_eq!(d["classification"], json!("classified_incomplete"));
    assert_eq!(d["detail"], json!("missing_slack_principal"));
}

#[test]
fn workspace_admin_classified_hashes_workspace_and_principal() {
    let a = json!({"workspace_id": "acme", "principal": "bob@example.com", "role": "admin"});
    let d = build_decision(&call("workspace.grant_admin", &a, Effect::Allow, "success"));
    assert_eq!(d["classification"], json!("classified"));
    assert_eq!(d["reason_code"], json!("classified_workspace_admin"));
    assert_eq!(d["action"]["resource_type"], json!("workspace_role"));
    let t = &d["action"]["target"];
    assert_eq!(
        t["workspace_id_hash"],
        json!(target_hash("workspace", "acme"))
    );
    assert_eq!(
        t["principal_hash"],
        json!(target_hash("workspace_principal", "bob@example.com"))
    );
    assert_eq!(t["role"], json!("admin"));
}

#[test]
fn required_scope_is_derived_from_category_not_args() {
    let gh = build_decision(&call(
        "github.add_deploy_key",
        &json!({"owner": "org", "repo": "r"}),
        Effect::Allow,
        "success",
    ));
    assert_eq!(
        gh["action"]["required_scope"],
        json!("repo:deploy_key:write")
    );

    let sl = build_decision(&call(
        "slack.add_member",
        &json!({"workspace_id": "T", "user_id": "U"}),
        Effect::Allow,
        "success",
    ));
    assert_eq!(
        sl["action"]["required_scope"],
        json!("conversations:members:write")
    );

    let wa = build_decision(&call(
        "workspace.grant_admin",
        &json!({"workspace_id": "a", "principal": "p"}),
        Effect::Allow,
        "success",
    ));
    assert_eq!(wa["action"]["required_scope"], json!("workspace:admin"));

    let unk = build_decision(&call("misc.do_thing", &json!({}), Effect::Allow, "success"));
    assert_eq!(unk["action"]["required_scope"], Value::Null);
}

#[test]
fn workspace_admin_unknown_verb_is_observed_unknown() {
    let a = json!({"workspace_id": "acme", "principal": "x"});
    let d = build_decision(&call(
        "workspace.do_random_thing",
        &a,
        Effect::Allow,
        "success",
    ));
    assert_eq!(d["classification"], json!("observed_unknown_tool"));
    assert_eq!(d["reason_code"], json!("unknown_tool_name"));
}

#[test]
fn hashes_are_domain_separated() {
    assert_ne!(
        target_hash("slack_principal", "alice@example.com"),
        target_hash("workspace_principal", "alice@example.com")
    );
}

#[test]
fn extra_and_secret_args_are_ignored_not_copied() {
    let a = json!({"owner": "org", "repo": "r", "junk": "ignore-me", "api_key": "sk-xyz"});
    let d = build_decision(&call("github.add_deploy_key", &a, Effect::Allow, "success"));
    let t = &d["action"]["target"];
    let keys: Vec<&str> = t.as_object().unwrap().keys().map(|s| s.as_str()).collect();
    for k in &keys {
        assert!(
            ["provider", "owner", "repo", "key_title_hash", "read_only"].contains(k),
            "unexpected target field {k}"
        );
    }
    let text = serde_json::to_string(&d).unwrap();
    assert!(!text.contains("ignore-me") && !text.contains("sk-xyz"));
}
