//! Focused #2512 contract: a delegated-exec observation can live in a bundle
//! outside privileged-mcp-action/v0 without becoming a whole-action cell.
//!
//! Named assertions are the must-bite: drop the content-address binding, or
//! move the schema into a v0 namespace, and this file fails on that name.

use assay_evidence::crypto::id::compute_content_hash;
use assay_evidence::{
    delegated_exec_observation_event, BundleWriter, EvidenceEvent,
    DELEGATED_EXEC_OBSERVATION_SCHEMA,
};
use assert_cmd::Command;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::Path;

const V0_NAMESPACES: &[&str] = &[
    "assay.enforcement_decision.",
    "assay.denied_call_observation.",
    "assay.manifest_establish.",
];

const DECISION_NON_CLAIMS: &[&str] = &[
    "policy decision only; does not assert or verify the upstream side effect (stays asserted, E9 ladder)",
    "an allow is the decision to forward; it does not assert the call reached or was performed by the upstream (a transport failure surfaces as proxy_failed, not here)",
    "credential referenced by alias only, never the token or declared scopes",
    "deny is fail-closed caution and allow is a policy decision \u{2014} neither is a maliciousness verdict",
    "not the observation artifact (assay.mcp_manifest_observed.v0) and not the mechanism artifact (assay.enforcement_health.v0)",
];

const TOOL: &str = "github.add_deploy_key";
const DIGEST: &str = "sha256:c3ff823d7fb2ee33b9f1a3f7be6eaf849acb980b6ec960731506436b56384dfc";
const SOURCE: &str = "urn:assay:test";
const RUN_ID: &str = "run-delegated-exec";
const CHILD_ARGV: &[u8] = b"child-tool --repo acme/prod-app";

fn decision_event(seq: u64) -> EvidenceEvent {
    let payload = json!({
        "schema": "assay.enforcement_decision.v0",
        "caller": {"id": "ci-agent"},
        "tool": {"name": TOOL, "action_class": "github_deploy_key"},
        "action": {
            "verb": "create",
            "resource_type": "github_deploy_key",
            "target": {"provider": "github", "owner": "acme", "repo": "prod-app"},
            "target_digest": DIGEST,
        },
        "decision": "deny",
        "reason": "no_declared_allowance",
        "fail_closed": true,
        "drift_state": "not_evaluated",
        "credential_alias": "gh-deploy",
        "non_claims": DECISION_NON_CLAIMS,
    });
    EvidenceEvent::new(
        "assay.enforcement_decision.v0",
        SOURCE,
        RUN_ID,
        seq,
        payload,
    )
}

fn write_bundle(path: &Path, events: Vec<EvidenceEvent>) {
    let file = File::create(path).expect("create bundle");
    let mut writer = BundleWriter::new(file);
    writer.add_events(events);
    writer.finish().expect("finish bundle");
}

fn verify_v0(bundle: &Path) -> Value {
    let output = Command::cargo_bin("assay")
        .expect("assay binary")
        .args(["evidence", "verify-privileged-mcp-action"])
        .arg(bundle)
        .args(["--format", "json"])
        .output()
        .expect("run v0 verifier");
    assert_eq!(
        output.status.code(),
        Some(0),
        "v0 verifier must accept the bundle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("verify report is JSON")
}

#[test]
fn delegated_exec_observation_is_present_content_addressed_and_ignored_by_v0() {
    assert!(
        !V0_NAMESPACES
            .iter()
            .any(|prefix| DELEGATED_EXEC_OBSERVATION_SCHEMA.starts_with(prefix)),
        "delegated_exec_schema_must_stay_outside_privileged_mcp_action_v0_namespaces"
    );

    let observation = delegated_exec_observation_event(SOURCE, RUN_ID, 1, "child-tool", CHILD_ARGV)
        .expect("producer emits observation");
    assert_eq!(observation.type_, DELEGATED_EXEC_OBSERVATION_SCHEMA);
    assert_eq!(
        observation.payload["schema"],
        DELEGATED_EXEC_OBSERVATION_SCHEMA
    );
    assert!(
        observation.payload.get("decision").is_none()
            && observation.payload.get("verdict").is_none()
            && observation.payload.get("outcome").is_none()
            && observation.payload.get("claims").is_none(),
        "delegated_exec_producer_emits_observation_not_decision_or_outcome_pairing"
    );
    let expected_argv = format!("sha256:{}", hex::encode(Sha256::digest(CHILD_ARGV)));
    assert_eq!(
        observation.payload["invocation"]["argv_digest"], expected_argv,
        "delegated_exec_observation_must_be_content_addressed"
    );
    assert_eq!(
        observation.content_hash.as_deref(),
        Some(
            compute_content_hash(&observation)
                .expect("recompute content hash")
                .as_str()
        ),
        "delegated_exec_observation_must_be_content_addressed"
    );

    let tmp = tempfile::tempdir().expect("tempdir");
    let without_path = tmp.path().join("without.tar.gz");
    let with_path = tmp.path().join("with.tar.gz");
    write_bundle(&without_path, vec![decision_event(0)]);
    write_bundle(&with_path, vec![decision_event(0), observation]);

    let without = verify_v0(&without_path);
    let with = verify_v0(&with_path);
    assert_eq!(without["bundle_integrity"], "pass");
    assert_eq!(with["bundle_integrity"], "pass");
    assert_eq!(without["verdict"], "valid");
    assert_eq!(with["verdict"], "valid");
    assert!(
        with["claims"].get("whole_action").is_none(),
        "v0 must not grow a whole-action cell"
    );
    let without_cells = serde_json::to_vec(&without["claims"]).expect("serialize without claims");
    let with_cells = serde_json::to_vec(&with["claims"]).expect("serialize with claims");
    assert_eq!(
        without_cells, with_cells,
        "v0_claim_cells_byte_identical_with_delegated_exec_observation"
    );
}
