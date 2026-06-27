use super::*;

pub(super) fn policy_from(yaml: &str) -> Result<EnforcePolicy> {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(yaml.as_bytes()).unwrap();
    load(f.path())
}

pub(super) const VALID: &str = r#"
caller:
  id: "ci-agent"
upstream_credential:
  alias: "gh-deploy"
  scopes: ["repo:deploy_key:write"]
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

/// An allow-acme policy with a custom upstream_credential block (pass "" for no credential).
pub(super) fn allow_acme_with_cred(cred_block: &str) -> EnforcePolicy {
    let yaml = format!(
        "caller:\n  id: \"ci-agent\"\n{cred_block}allowances:\n  - action_class: \"github_deploy_key\"\n    targets:\n      - {{ owner: \"acme\", repo: \"prod-app\" }}\n"
    );
    policy_from(&yaml).unwrap()
}

/// The one tools/call that matches the acme allowance (so the credential-scope gate is reached).
pub(super) fn acme_call() -> Value {
    json!({"owner": "acme", "repo": "prod-app"})
}

pub(super) const TOOL: &str = "github.add_deploy_key";
pub(super) const APPROVED: &str = "sha256:approved-digest";

/// A single-tool declared baseline (loaded + strictly validated like at startup).
pub(super) fn baseline_with(name: &str, digest: &str) -> DeclaredManifest {
    let j = format!(
        r#"{{"schema":"assay.declared_mcp_manifest.v0","tools":[{{"name":"{name}","tool_digest":"{digest}"}}]}}"#
    );
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(j.as_bytes()).unwrap();
    load_declared_manifest(f.path()).unwrap()
}

/// `decide` with a baseline + observed digest that MATCH for `github.add_deploy_key`, so only the
/// gates BEFORE the drift gate can produce a deny (the classification/allowance/credential tests).
/// A call that also clears those earlier gates therefore reaches an ALLOW here.
pub(super) fn decide_match(p: &EnforcePolicy, tool: &str, args: &Value) -> Decision {
    decide(
        p,
        &baseline_with(TOOL, APPROVED),
        &ObservedToolDigest::Present(APPROVED.to_string()),
        tool,
        args,
    )
}

pub(super) fn cred_policy(cred_block: &str) -> EnforcePolicy {
    allow_acme_with_cred(cred_block)
}

/// One golden row: an owned scenario plus its expected verdict and record fields.
pub(super) struct GoldenCase {
    pub(super) name: &'static str,
    pub(super) policy: EnforcePolicy,
    pub(super) baseline: DeclaredManifest,
    pub(super) observed: ObservedToolDigest,
    pub(super) tool: &'static str,
    pub(super) args: Value,
    pub(super) reason: &'static str,
    pub(super) allow: bool,
    pub(super) drift_state: &'static str,
    pub(super) action_class: Option<&'static str>,
}

pub(super) fn matching_baseline() -> DeclaredManifest {
    baseline_with(TOOL, APPROVED)
}
pub(super) fn matching_observed() -> ObservedToolDigest {
    ObservedToolDigest::Present(APPROVED.to_string())
}

pub(super) fn golden_corpus() -> Vec<GoldenCase> {
    let ro_cred = "upstream_credential:\n  alias: \"gh-ro\"\n  scopes: [\"repo:read\"]\n";
    vec![
        // 1. classification gate
        GoldenCase {
            name: "unclassified_tool_call",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: matching_observed(),
            tool: "misc.do_thing",
            args: json!({}),
            reason: "unclassified_tool_call",
            allow: false,
            drift_state: "not_evaluated",
            action_class: None,
        },
        GoldenCase {
            name: "classification_incomplete",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: matching_observed(),
            tool: TOOL,
            args: json!({"owner": "acme"}), // missing repo
            reason: "classification_incomplete",
            allow: false,
            drift_state: "not_evaluated",
            action_class: Some("github_deploy_key"),
        },
        // 2. caller-allowance gate (two scenarios, same precedence-pinned reason)
        GoldenCase {
            name: "no_declared_allowance",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: matching_observed(),
            tool: TOOL,
            args: json!({"owner": "other", "repo": "x"}),
            reason: "no_declared_allowance",
            allow: false,
            drift_state: "not_evaluated",
            action_class: Some("github_deploy_key"),
        },
        GoldenCase {
            name: "allowance_target_mismatch",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: matching_observed(),
            tool: TOOL,
            args: json!({"owner": "acme", "repo": "other-repo"}),
            reason: "no_declared_allowance",
            allow: false,
            drift_state: "not_evaluated",
            action_class: Some("github_deploy_key"),
        },
        // 3. credential-scope gate
        GoldenCase {
            name: "credential_scope_unknown",
            policy: cred_policy(""), // no declared credential
            baseline: matching_baseline(),
            observed: matching_observed(),
            tool: TOOL,
            args: acme_call(),
            reason: "credential_scope_unknown",
            allow: false,
            drift_state: "not_evaluated",
            action_class: Some("github_deploy_key"),
        },
        GoldenCase {
            name: "credential_scope_insufficient",
            policy: cred_policy(ro_cred),
            baseline: matching_baseline(),
            observed: matching_observed(),
            tool: TOOL,
            args: acme_call(),
            reason: "credential_scope_insufficient",
            allow: false,
            drift_state: "not_evaluated",
            action_class: Some("github_deploy_key"),
        },
        // 4. drift gate
        GoldenCase {
            name: "manifest_baseline_missing",
            policy: policy_from(VALID).unwrap(),
            baseline: baseline_with("github.other_tool", APPROVED), // baseline lacks TOOL
            observed: matching_observed(),
            tool: TOOL,
            args: acme_call(),
            reason: "manifest_baseline_missing",
            allow: false,
            drift_state: "baseline_missing",
            action_class: Some("github_deploy_key"),
        },
        GoldenCase {
            name: "manifest_current_observation_incomplete",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: ObservedToolDigest::NoCompleteManifest,
            tool: TOOL,
            args: acme_call(),
            reason: "manifest_current_observation_incomplete",
            allow: false,
            drift_state: "current_observation_incomplete",
            action_class: Some("github_deploy_key"),
        },
        GoldenCase {
            name: "manifest_current_observation_incomplete_tool_absent",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: ObservedToolDigest::CompleteButToolAbsent,
            tool: TOOL,
            args: acme_call(),
            reason: "manifest_current_observation_incomplete",
            allow: false,
            drift_state: "current_observation_incomplete",
            action_class: Some("github_deploy_key"),
        },
        GoldenCase {
            name: "manifest_observation_ambiguous",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: ObservedToolDigest::Ambiguous,
            tool: TOOL,
            args: acme_call(),
            reason: "manifest_observation_ambiguous",
            allow: false,
            drift_state: "observation_ambiguous",
            action_class: Some("github_deploy_key"),
        },
        GoldenCase {
            name: "manifest_drifted_since_approval",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: ObservedToolDigest::Present("sha256:something-else".to_string()),
            tool: TOOL,
            args: acme_call(),
            reason: "manifest_drifted_since_approval",
            allow: false,
            drift_state: "drifted",
            action_class: Some("github_deploy_key"),
        },
        // 5. all gates pass -> allow (decision-only; no forward in the corpus)
        GoldenCase {
            name: "all_gates_pass_allow",
            policy: policy_from(VALID).unwrap(),
            baseline: matching_baseline(),
            observed: matching_observed(),
            tool: TOOL,
            args: acme_call(),
            reason: "allow",
            allow: true,
            drift_state: "satisfied",
            action_class: Some("github_deploy_key"),
        },
    ]
}
