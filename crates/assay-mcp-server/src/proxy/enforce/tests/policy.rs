use super::fixtures::*;
use super::*;

#[test]
fn loads_a_valid_policy() {
    let p = policy_from(VALID).unwrap();
    assert_eq!(p.caller_id(), "ci-agent");
    assert!(p.declares_target("github_deploy_key", "acme", "prod-app"));
    assert!(p.credential().is_some());
}

#[test]
fn missing_caller_id_fails_load() {
    assert!(policy_from("allowances: []\n").is_err());
    assert!(policy_from("caller:\n  id: \"\"\n").is_err());
}

#[test]
fn malformed_yaml_fails_load() {
    assert!(policy_from("caller: : :\n").is_err());
}

// --- declared-manifest baseline loader (strict, startup-validated) ------------------------------

fn manifest_from(json: &str) -> Result<DeclaredManifest> {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(json.as_bytes()).unwrap();
    load_declared_manifest(f.path())
}

#[test]
fn valid_baseline_loads() {
    let m = manifest_from(&super::fixtures::declared_json(
        "github.add_deploy_key",
        super::fixtures::APPROVED,
    ))
    .unwrap();
    assert_eq!(
        m.tool_digest_for("github.add_deploy_key"),
        Some(super::fixtures::APPROVED)
    );
    assert_eq!(m.tool_digest_for("nope"), None);
}

#[test]
fn wrong_schema_baseline_fails() {
    assert!(manifest_from(
        r#"{"schema":"assay.mcp_manifest_observed.v0","tools":[{"name":"t","tool_digest":"sha256:abc"}]}"#
    )
    .is_err());
}

#[test]
fn empty_tools_baseline_fails() {
    assert!(manifest_from(r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[]}"#).is_err());
}

#[test]
fn non_sha256_digest_fails() {
    assert!(manifest_from(
        r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[{"name":"t","tool_digest":"deadbeef"}]}"#
    )
    .is_err());
}

#[test]
fn tool_without_digest_fails() {
    // tool_digest is required (not Option) -> a tool missing it fails to parse.
    assert!(
        manifest_from(r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[{"name":"t"}]}"#)
            .is_err()
    );
}

#[test]
fn duplicate_baseline_tool_names_fail_load() {
    // An approval baseline must be unambiguous: duplicate names fail startup (no first-match-wins).
    assert!(manifest_from(
        r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[{"name":"t","tool_digest":"sha256:a"},{"name":"t","tool_digest":"sha256:b"}]}"#
    )
    .is_err());
}

// --- every key of the enforce policy is acted on, or the policy is refused ----------------------

/// The load error for `yaml`, as one string.
fn refusal(yaml: &str) -> String {
    format!(
        "{:#}",
        policy_from(yaml).expect_err("the policy must be refused")
    )
}

/// serde's defaults dropped a key the proxy does not read. An extra key on a target narrowed
/// nothing: `{owner, repo, read_only: true}` allowed every deploy key on that repository.
#[test]
fn a_key_the_proxy_does_not_act_on_refuses_the_policy_named_by_its_path() {
    let target = r#"{ owner: "acme", repo: "prod-app" }"#;
    assert_eq!(VALID.matches(target).count(), 1);
    for (yaml, path) in [
        (
            VALID.replace(
                target,
                r#"{ owner: "acme", repo: "prod-app", read_only: true }"#,
            ),
            "allowances.0.targets.0.read_only",
        ),
        (VALID.replace("allowances:", "allowance:"), "allowance"),
        (
            VALID.replace("  scopes:", "  scope:"),
            "upstream_credential.scope",
        ),
        (
            VALID.replace("  id: \"ci-agent\"", "  id: \"ci-agent\"\n  role: admin"),
            "caller.role",
        ),
        (
            VALID.replace("    targets:", "    conditions: [weekday]\n    targets:"),
            "allowances.0.conditions",
        ),
        (format!("\"?\": 1\n{VALID}"), "?"),
    ] {
        let error = refusal(&yaml);
        assert!(
            error.contains("the enforcing proxy does not act on")
                && error.contains(&format!("{path:?}")),
            "{path} must be named: {error}"
        );
    }
}

#[test]
fn every_refused_key_is_named_in_file_order() {
    let yaml = format!("zeta: 1\n{VALID}alpha: 2\n");
    let error = refusal(&yaml);
    let zeta = error.find("\"zeta\"").expect("zeta named");
    let alpha = error.find("\"alpha\"").expect("alpha named");
    assert!(zeta < alpha, "file order: {error}");
    assert!(error.contains("2 key(s)"), "{error}");
}

/// Positive control: every enforce policy the repository ships loads.
#[test]
fn every_enforce_policy_in_the_repository_loads() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut loaded = 0;
    for dir in [
        "examples/privileged-action-gate/policies",
        "packaging/agent-plugin/skills/assay-golden-path/assets/privileged-action-gate/policies",
        "packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/policies",
    ] {
        for entry in std::fs::read_dir(root.join(dir)).expect(dir) {
            let path = entry.expect("entry").path();
            if path.extension().is_some_and(|e| e == "yaml") {
                load(&path).unwrap_or_else(|e| panic!("{} must load: {e:#}", path.display()));
                loaded += 1;
            }
        }
    }
    assert!(loaded >= 5, "found only {loaded} policies");
}

#[test]
fn enforce_policy_limits_source_bytes_before_decoding() {
    const LIMIT: usize = 16 * 1024 * 1024;
    let mut input = String::from("caller: {id: ci}\n#");
    input.extend(std::iter::repeat_n('x', LIMIT - input.len()));
    assert_eq!(policy_from(&input).unwrap().caller_id(), "ci");
    input.push('x');
    assert!(refusal(&input).contains("16777216"));
    let mut bytes = input.into_bytes();
    bytes[LIMIT] = 0xff;
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), bytes).unwrap();
    let error = load(f.path()).unwrap_err();
    let io = error.downcast_ref::<std::io::Error>().unwrap();
    let exceeded = assay_common::limits::LimitExceeded::from_io(io).unwrap();
    assert_eq!(exceeded.limit, LIMIT as u64);
}

#[test]
fn enforce_policy_keeps_scalar_spelling_and_typed_error_precedence() {
    let policy = policy_from("caller: {id: 0x10}").unwrap();
    assert_eq!(policy.caller_id(), "0x10");
    let error = policy_from("unknown: true\ncaller: {id: []}").unwrap_err();
    let yaml = error.downcast_ref::<serde_yaml::Error>().unwrap();
    assert_eq!(yaml.location().unwrap().line(), 2);
    let detail = refusal("0x10: 1\ncaller: {id: ci}\n0x10: 2");
    assert!(detail.contains("2 key(s)"), "{detail}");
    assert!(detail.contains("\"0x10\", \"0x10\""), "{detail}");
}
