//! P2a `mcp-signal-followup` pack: parity, MCP-002/003 behavior, Trust Basis alignment for MCP-001.

mod common;

use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions, LintReportWithPacks};
use assay_evidence::lint::packs::loader::{load_pack, load_pack_from_file};
use assay_evidence::lint::packs::schema::CheckDefinition;
use assay_evidence::lint::packs::LoadedPack;
use assay_evidence::{
    generate_trust_basis, TrustBasis, TrustBasisOptions, TrustClaimId, TrustClaimLevel,
    VerifyLimits,
};
use serde_json::json;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn open_pack_path() -> PathBuf {
    repo_root()
        .join("packs")
        .join("open")
        .join("mcp-signal-followup")
        .join("pack.yaml")
}

fn readme_path() -> PathBuf {
    repo_root()
        .join("packs")
        .join("open")
        .join("mcp-signal-followup")
        .join("README.md")
}

fn builtin_pack_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("packs")
        .join("mcp-signal-followup.yaml")
}

fn load_open_pack() -> LoadedPack {
    load_pack_from_file(&open_pack_path()).expect("open pack should load")
}

fn load_builtin_pack() -> LoadedPack {
    load_pack("mcp-signal-followup").expect("built-in pack should load")
}

fn claim_level(tb: &TrustBasis, id: TrustClaimId) -> TrustClaimLevel {
    tb.claims
        .iter()
        .find(|c| c.id == id)
        .expect("claim present")
        .level
}

fn lint_with_pack(pack: LoadedPack, bundle: &[u8]) -> LintReportWithPacks {
    let options = LintOptions {
        packs: vec![pack],
        max_results: Some(500),
        bundle_path: Some("mcp-pack.tar.gz".to_string()),
    };
    lint_bundle_with_options(Cursor::new(bundle), VerifyLimits::default(), options)
        .expect("lint should succeed")
}

fn has_rule_finding(result: &LintReportWithPacks, pack_name: &str, rule_id: &str) -> bool {
    let prefix = format!("{pack_name}@");
    result
        .report
        .findings
        .iter()
        .any(|finding| finding.rule_id.starts_with(&prefix) && finding.rule_id.ends_with(rule_id))
}

fn mcp002_only_bundle() -> Vec<u8> {
    common::make_bundle(vec![common::make_event(
        "assay.tool.decision",
        "mcp2",
        0,
        json!({
            "tool": "t",
            "decision": "allow",
            "principal": "x",
            "delegated_from": "agent:p",
        }),
    )])
}

fn mcp002_fail_bundle() -> Vec<u8> {
    common::make_bundle(vec![common::make_event(
        "assay.tool.decision",
        "mcp2f",
        0,
        json!({
            "tool": "t",
            "decision": "allow",
            "principal": "x",
        }),
    )])
}

fn mcp003_only_bundle() -> Vec<u8> {
    common::make_bundle(vec![common::make_event(
        "assay.sandbox.degraded",
        "mcp3",
        0,
        json!({"reason_code": "x"}),
    )])
}

#[test]
fn mcp_followup_loads_builtin_and_open_with_three_rules() {
    let builtin = load_builtin_pack();
    let open = load_open_pack();
    assert_eq!(builtin.definition.name, "mcp-signal-followup");
    assert_eq!(open.definition.name, "mcp-signal-followup");
    assert_eq!(builtin.definition.rules.len(), 3);
    assert_eq!(open.definition.rules.len(), 3);
}

#[test]
fn mcp_followup_builtin_and_open_pack_are_exactly_equivalent() {
    let builtin_raw = fs::read_to_string(builtin_pack_path()).expect("read builtin yaml");
    let open_raw = fs::read_to_string(open_pack_path()).expect("read open yaml");
    assert_eq!(
        builtin_raw, open_raw,
        "open pack and built-in mirror must match exactly"
    );

    let builtin = load_builtin_pack();
    let open = load_open_pack();
    assert_eq!(builtin.digest, open.digest);
    assert_eq!(
        serde_json::to_value(&builtin.definition).expect("serialize builtin"),
        serde_json::to_value(&open.definition).expect("serialize open")
    );
}

#[test]
fn mcp001_uses_g3_check_definition() {
    let pack = load_builtin_pack();
    let r = pack
        .definition
        .rules
        .iter()
        .find(|rule| rule.id == "MCP-001")
        .expect("MCP-001");
    assert!(matches!(
        r.check,
        CheckDefinition::G3AuthorizationContextPresent
    ));
    assert_eq!(r.engine_min_version.as_deref(), Some("1.2"));
}

#[test]
fn mcp001_aligns_trust_basis_verified_and_pack_passes() {
    let bundle = common::full_signal_bundle();
    let tb = generate_trust_basis(
        Cursor::new(&bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    assert_eq!(
        claim_level(&tb, TrustClaimId::AuthorizationContextVisible),
        TrustClaimLevel::Verified
    );
    assert_eq!(
        claim_level(&tb, TrustClaimId::DelegationContextVisible),
        TrustClaimLevel::Verified
    );
    assert_eq!(
        claim_level(&tb, TrustClaimId::ContainmentDegradationObserved),
        TrustClaimLevel::Verified
    );

    let result = lint_with_pack(load_builtin_pack(), &bundle);
    assert!(
        !has_rule_finding(&result, "mcp-signal-followup", "MCP-001"),
        "MCP-001 should pass when G3 predicate matches"
    );
    assert!(!has_rule_finding(&result, "mcp-signal-followup", "MCP-002"));
    assert!(!has_rule_finding(&result, "mcp-signal-followup", "MCP-003"));
}

#[test]
fn mcp001_aligns_trust_basis_absent_and_pack_fails() {
    let bundle = common::g3_absent_principal_only_bundle();
    let tb = generate_trust_basis(
        Cursor::new(&bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    assert_eq!(
        claim_level(&tb, TrustClaimId::AuthorizationContextVisible),
        TrustClaimLevel::Absent
    );

    let result = lint_with_pack(load_builtin_pack(), &bundle);
    assert!(
        has_rule_finding(&result, "mcp-signal-followup", "MCP-001"),
        "MCP-001 should fail when Trust Basis G3 is absent"
    );
}

#[test]
fn mcp002_passes_with_delegated_from() {
    let result = lint_with_pack(load_builtin_pack(), &mcp002_only_bundle());
    assert!(!has_rule_finding(&result, "mcp-signal-followup", "MCP-002"));
}

#[test]
fn mcp002_fails_without_delegated_from() {
    let result = lint_with_pack(load_builtin_pack(), &mcp002_fail_bundle());
    assert!(has_rule_finding(&result, "mcp-signal-followup", "MCP-002"));
}

#[test]
fn mcp003_passes_with_degraded_event() {
    let result = lint_with_pack(load_builtin_pack(), &mcp003_only_bundle());
    assert!(!has_rule_finding(&result, "mcp-signal-followup", "MCP-003"));
}

#[test]
fn mcp003_fails_without_degraded_event() {
    let result = lint_with_pack(load_builtin_pack(), &mcp002_fail_bundle());
    assert!(has_rule_finding(&result, "mcp-signal-followup", "MCP-003"));
}

#[test]
fn mcp_readme_lists_non_goals() {
    let readme = fs::read_to_string(readme_path()).expect("readme");
    assert!(readme.contains("## Non-Goals"));
    assert!(readme.contains("authorization validity"));
}

/// Writes two `.tar.gz` bundles under `target/mcp-lint-demo/` for manual CLI demos:
/// `assay evidence lint <path> --pack mcp-signal-followup`
#[test]
#[ignore]
fn write_mcp_lint_demo_bundles() {
    let dir = repo_root().join("target").join("mcp-lint-demo");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(
        dir.join("g3_full_pass.tar.gz"),
        common::full_signal_bundle(),
    )
    .expect("write full");
    fs::write(
        dir.join("g3_principal_only_fail.tar.gz"),
        common::g3_absent_principal_only_bundle(),
    )
    .expect("write fail");
    eprintln!(
        "Wrote:\n  {}\n  {}",
        dir.join("g3_full_pass.tar.gz").display(),
        dir.join("g3_principal_only_fail.tar.gz").display()
    );
}
