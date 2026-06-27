use super::fixtures::*;
use super::*;
use assay_core::mcp::jcs;
use assay_mcp_server::cache::sha256_hex;

#[test]
fn unclassified_tool_is_denied_unclassified() {
    let p = policy_from(VALID).unwrap();
    let d = decide_match(&p, "misc.do_thing", &json!({}));
    assert_eq!(d.reason, "unclassified_tool_call");
}

#[test]
fn incomplete_classification_is_denied_before_allowance() {
    // Missing repo -> classified_incomplete; must read as classification_incomplete, NOT
    // no_declared_allowance/target-mismatch.
    let p = policy_from(VALID).unwrap();
    let d = decide_match(&p, "github.add_deploy_key", &json!({"owner": "acme"}));
    assert_eq!(d.reason, "classification_incomplete");
}

#[test]
fn classified_without_allowance_is_denied_no_declared_allowance() {
    let p = policy_from(VALID).unwrap();
    let d = decide_match(
        &p,
        "github.add_deploy_key",
        &json!({"owner": "other", "repo": "x"}),
    );
    assert_eq!(d.reason, "no_declared_allowance");
    assert_eq!(d.action_class.as_deref(), Some("github_deploy_key"));
}

#[test]
fn all_gates_pass_with_matching_digest_allows() {
    // VALID clears classification + allowance + credential-scope; with a baseline + observed digest
    // that match (decide_match), the drift gate passes too -> the one and only allow path fires.
    let p = policy_from(VALID).unwrap();
    let d = decide_match(&p, "github.add_deploy_key", &acme_call());
    assert!(d.allow, "every gate passed -> forward");
    assert_eq!(d.reason, "allow");
}

// --- c2 credential-scope gate (runs only after the allowance matches) -----------------------

#[test]
fn no_declared_credential_is_credential_scope_unknown() {
    // Allowance matches, but no credential is declared -> coverage cannot be determined.
    let p = allow_acme_with_cred("");
    let d = decide_match(&p, "github.add_deploy_key", &acme_call());
    assert_eq!(d.reason, "credential_scope_unknown");
}

#[test]
fn non_covering_scope_is_credential_scope_insufficient() {
    let p =
        allow_acme_with_cred("upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:read\"]\n");
    let d = decide_match(&p, "github.add_deploy_key", &acme_call());
    assert_eq!(d.reason, "credential_scope_insufficient");
}

#[test]
fn coarse_repo_scope_is_unrecognized_and_unknown() {
    // "repo" is NOT in the documented contract (only repo:deploy_key:write / repo:admin cover,
    // repo:read/metadata/contents:read are recognized-non-covering); so "repo" is unrecognized
    // -> unknown, never silently covered and never "insufficient".
    let p = allow_acme_with_cred("upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo\"]\n");
    let d = decide_match(&p, "github.add_deploy_key", &acme_call());
    assert_eq!(d.reason, "credential_scope_unknown");
}

#[test]
fn unrecognized_scope_is_credential_scope_unknown() {
    let p = allow_acme_with_cred("upstream_credential:\n  alias: \"gh\"\n  scopes: [\"banana\"]\n");
    let d = decide_match(&p, "github.add_deploy_key", &acme_call());
    assert_eq!(d.reason, "credential_scope_unknown");
}

#[test]
fn repo_write_does_not_cover_deploy_key_per_p59_contract() {
    // repo:write is NOT a covering scope in credential-scope.md — it is unrecognized -> unknown,
    // NOT a silent pass. (The c2 gate must never be broader than the documented contract.)
    let p =
        allow_acme_with_cred("upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:write\"]\n");
    let d = decide_match(&p, "github.add_deploy_key", &acme_call());
    assert_eq!(d.reason, "credential_scope_unknown");
}

#[test]
fn repo_admin_covers_then_matching_digest_allows() {
    // repo:admin covers the credential gate (admin breadth); with a matching baseline + observed
    // digest the drift gate passes too -> allow.
    let p =
        allow_acme_with_cred("upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:admin\"]\n");
    let d = decide_match(&p, "github.add_deploy_key", &acme_call());
    assert!(d.allow);
    assert_eq!(d.reason, "allow");
}

#[test]
fn unknown_required_scope_fails_closed() {
    // Defensive: a class with no required_scope must deny (unknown), never pass — guards against
    // the `?` fail-open trap in credential_scope_gate.
    let p = policy_from(VALID).unwrap();
    assert_eq!(
        credential_scope_gate(&p, "not_a_real_class"),
        Some("credential_scope_unknown")
    );
}

#[test]
fn credential_gate_runs_after_allowance_not_before() {
    // A non-matching target denies at the allowance gate even with a fully insufficient credential
    // (precedence: allowance before credential-scope).
    let p =
        allow_acme_with_cred("upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:read\"]\n");
    let d = decide_match(
        &p,
        "github.add_deploy_key",
        &json!({"owner": "other", "repo": "x"}),
    );
    assert_eq!(d.reason, "no_declared_allowance");
}

#[test]
fn scope_covers_matches_the_p59_contract() {
    // The enforcement lattice equals credential-scope.md exactly: covered by
    // {repo:deploy_key:write, repo:admin}; NOT covered by {repo:read, repo:metadata,
    // repo:contents:read}; everything else (incl. repo:write, repo) is unrecognized -> unknown.
    let req = "repo:deploy_key:write";
    let cov = |s: &[&str]| {
        matches!(
            scope_covers(
                "github_deploy_key",
                req,
                &s.iter().map(|x| x.to_string()).collect::<Vec<_>>()
            ),
            ScopeCoverage::Covered
        )
    };
    assert!(cov(&["repo:deploy_key:write"]), "exact covers");
    assert!(cov(&["repo:admin"]), "admin breadth covers");
    // repo:write is NOT in the documented contract -> unrecognized -> unknown (not covered).
    assert!(matches!(
        scope_covers("github_deploy_key", req, &["repo:write".to_string()]),
        ScopeCoverage::Unknown
    ));
    for ns in ["repo:read", "repo:metadata", "repo:contents:read"] {
        assert!(
            matches!(
                scope_covers("github_deploy_key", req, &[ns.to_string()]),
                ScopeCoverage::Insufficient
            ),
            "{ns} is recognized-non-covering -> insufficient"
        );
    }
    assert!(matches!(
        scope_covers("github_deploy_key", req, &["repo".to_string()]),
        ScopeCoverage::Unknown
    ));
    // An unrecognized scope alongside a recognized non-covering one -> unknown takes precedence.
    assert!(
        matches!(
            scope_covers(
                "github_deploy_key",
                req,
                &["banana".to_string(), "repo:read".to_string()]
            ),
            ScopeCoverage::Unknown
        ),
        "unknown takes precedence over insufficient"
    );
    // No documented lattice for another class -> Unknown (fail-closed).
    assert!(matches!(
        scope_covers("slack_add_member", req, &["repo:admin".to_string()]),
        ScopeCoverage::Unknown
    ));
}

#[test]
fn target_digest_is_domain_separated_and_stable() {
    let a = target_digest(&json!({"owner": "acme", "repo": "prod-app"}));
    let b = target_digest(&json!({"repo": "prod-app", "owner": "acme"}));
    assert_eq!(a, b, "canonical: key order independent");
    assert!(a.starts_with("sha256:"));
    // Domain separation: the same bytes under a different domain would differ (sanity).
    let raw = format!(
        "sha256:{}",
        sha256_hex(&jcs::to_vec(&json!({"owner": "acme", "repo": "prod-app"})).unwrap())
    );
    assert_ne!(
        a, raw,
        "domain prefix must change the digest vs a bare hash"
    );
}

#[test]
fn pdp_golden_corpus_truth_table() {
    let corpus = golden_corpus();
    let cases_total = corpus.len();
    let mut expected_reason_match = 0usize;
    let mut unexpected_allows = 0usize;
    let mut unexpected_forwards = 0usize;

    for c in &corpus {
        let d = decide(&c.policy, &c.baseline, &c.observed, c.tool, &c.args);

        // verdict
        assert_eq!(d.allow, c.allow, "{}: allow", c.name);
        assert_eq!(d.reason, c.reason, "{}: reason", c.name);
        if d.reason == c.reason {
            expected_reason_match += 1;
        }
        if d.allow && !c.allow {
            unexpected_allows += 1;
        }

        // emitted record shape (the producer+consumer contract)
        let rec = decision_record(&c.policy, &d, c.tool, &c.args);
        assert_eq!(rec["schema"], "assay.enforcement_decision.v0", "{}", c.name);
        assert_eq!(
            rec["decision"],
            if c.allow { "allow" } else { "deny" },
            "{}: decision",
            c.name
        );
        assert_eq!(rec["reason"], c.reason, "{}: record reason", c.name);
        assert_eq!(rec["fail_closed"], !c.allow, "{}: fail_closed", c.name);
        assert_eq!(rec["drift_state"], c.drift_state, "{}: drift_state", c.name);
        match c.action_class {
            None => assert!(
                rec["tool"]["action_class"].is_null(),
                "{}: action_class must be null",
                c.name
            ),
            Some(ac) => assert_eq!(rec["tool"]["action_class"], ac, "{}: action_class", c.name),
        }
        // an allow is a decision-to-forward, never a delivery claim: no `forwarded` field, ever.
        if rec.get("forwarded").is_some() {
            unexpected_forwards += 1;
        }

        // No credential material leaks: the record references the credential by ALIAS only, never
        // the declared scopes and never a token. (The contract a consumer reads must be safe to
        // store and project.)
        let serialized = rec.to_string();
        if let Some(cred) = c.policy.upstream_credential.as_ref() {
            for scope in &cred.scopes {
                assert!(
                    !serialized.contains(scope.as_str()),
                    "{}: declared scope {:?} leaked into the decision record",
                    c.name,
                    scope
                );
            }
            // credential_alias, when present, is exactly the alias string — not the scopes.
            assert_eq!(
                rec["credential_alias"], cred.alias,
                "{}: credential_alias must be the alias only",
                c.name
            );
        } else {
            // no declared credential -> the alias field is null, never fabricated.
            assert!(
                rec["credential_alias"].is_null(),
                "{}: credential_alias must be null when no credential is declared",
                c.name
            );
        }
    }

    // The corpus measurement (assay.experiment.pdp_golden.v0): every reason matches, nothing is
    // allowed that should deny, and no record claims a forward.
    assert_eq!(expected_reason_match, cases_total, "expected_reason_match");
    assert_eq!(unexpected_allows, 0, "unexpected_allows must be 0");
    assert_eq!(unexpected_forwards, 0, "unexpected_forwards must be 0");
    // exactly one allow row in the whole corpus
    assert_eq!(
        corpus.iter().filter(|c| c.allow).count(),
        1,
        "the corpus has exactly one all-gates-pass allow row"
    );
}

#[test]
fn pdp_golden_reason_precedence() {
    // First failing gate wins. Each row would fail a LATER gate too; the earlier reason must win.
    let valid = policy_from(VALID).unwrap();
    let no_cred = cred_policy("");

    // classification > allowance: an unclassified tool whose (irrelevant) target would also miss
    // the allowance still reads unclassified.
    let d = decide(
        &valid,
        &matching_baseline(),
        &matching_observed(),
        "misc.do_thing",
        &json!({"owner": "other"}),
    );
    assert_eq!(
        d.reason, "unclassified_tool_call",
        "classification > allowance"
    );

    // classification_incomplete > allowance: acme + missing repo cannot match the allowance, yet
    // it reads as classification_incomplete, not no_declared_allowance.
    let d = decide(
        &valid,
        &matching_baseline(),
        &matching_observed(),
        TOOL,
        &json!({"owner": "acme"}),
    );
    assert_eq!(
        d.reason, "classification_incomplete",
        "classification_incomplete > allowance"
    );

    // allowance > credential: a non-matching target with a broken (absent) credential reads as
    // no_declared_allowance, never credential_scope_unknown.
    let d = decide(
        &no_cred,
        &matching_baseline(),
        &matching_observed(),
        TOOL,
        &json!({"owner": "other", "repo": "x"}),
    );
    assert_eq!(d.reason, "no_declared_allowance", "allowance > credential");

    // credential > drift: an absent credential AND a drifted observation both fail; the credential
    // reason wins because c2 runs before c3.
    let d = decide(
        &no_cred,
        &matching_baseline(),
        &ObservedToolDigest::Present("sha256:something-else".to_string()),
        TOOL,
        &acme_call(),
    );
    assert_eq!(d.reason, "credential_scope_unknown", "credential > drift");
}
