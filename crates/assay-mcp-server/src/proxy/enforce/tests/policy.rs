use super::fixtures::*;
use super::*;

#[test]
fn loads_a_valid_policy() {
    let p = policy_from(VALID).unwrap();
    assert_eq!(p.caller.id, "ci-agent");
    assert_eq!(p.allowances.len(), 1);
    assert!(p.upstream_credential.is_some());
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
    let m = manifest_from(
        r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[{"name":"github.add_deploy_key","tool_digest":"sha256:abc"}]}"#,
    )
    .unwrap();
    assert_eq!(
        m.tool_digest_for("github.add_deploy_key"),
        Some("sha256:abc")
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
