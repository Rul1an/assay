use crate::support::{
    deny_reason_for, ALLOW_ACME, ALLOW_ACME_INSUFFICIENT_CRED, ALLOW_ACME_NO_CRED,
};

// --- pre-drift gates (classification / allowance / credential), no observation needed --------------

#[test]
fn unclassified_tool_call_denied_unclassified() {
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "echo", "arguments": {}}),
    );
    assert_eq!(reason, "unclassified_tool_call");
}

#[test]
fn classification_incomplete_denied_before_allowance() {
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "github.add_deploy_key", "arguments": {"owner": "acme"}}),
    );
    assert_eq!(reason, "classification_incomplete");
}

#[test]
fn classified_privileged_without_matching_allowance_denied() {
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "evil", "repo": "x"}}),
    );
    assert_eq!(reason, "no_declared_allowance");
}

#[test]
fn allowance_target_mismatch_denied_no_declared_allowance() {
    let (reason, _) = deny_reason_for(
        ALLOW_ACME,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "staging-app"}}),
    );
    assert_eq!(reason, "no_declared_allowance");
}

#[test]
fn insufficient_credential_scope_denied() {
    let (reason, _) = deny_reason_for(
        ALLOW_ACME_INSUFFICIENT_CRED,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "prod-app"}}),
    );
    assert_eq!(reason, "credential_scope_insufficient");
}

#[test]
fn no_declared_credential_is_scope_unknown() {
    let (reason, _) = deny_reason_for(
        ALLOW_ACME_NO_CRED,
        serde_json::json!({"name": "github.add_deploy_key",
                           "arguments": {"owner": "acme", "repo": "prod-app"}}),
    );
    assert_eq!(reason, "credential_scope_unknown");
}
