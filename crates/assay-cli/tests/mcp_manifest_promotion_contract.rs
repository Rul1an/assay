use assay_mcp_server::manifest_io::MAX_MANIFEST_BYTES;
use assay_mcp_server::manifest_observed::{
    build_observed, manifest_digest, Completeness, CANONICALIZATION, SCHEMA,
};
use assert_cmd::Command;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

type JsonMutation = Box<dyn Fn(&mut Value)>;

fn digest(byte: u8) -> String {
    format!("sha256:{}", hex::encode([byte; 32]))
}

fn promotable_observed() -> Value {
    let tool_digest = digest(1);
    let manifest_digest = manifest_digest(&[("echo".into(), tool_digest.clone())]);
    json!({
        "schema": SCHEMA,
        "status": "observed",
        "server": { "id": "local" },
        "observed": {
            "manifest_digest": manifest_digest,
            "canonicalization": CANONICALIZATION,
            "tool_count": 1,
            "privileged_tool_count": 0,
            "tools_list_observed": true,
            "tools_list_complete": "complete",
            "tool_digests": [{
                "name": "echo",
                "tool_digest": tool_digest,
                "privileged": false,
                "privilege_classification": "unclassified",
                "action_class": null,
                "field_digests": {
                    "description": digest(2),
                    "input_schema": digest(3),
                    "output_schema": digest(4),
                    "annotations": digest(5)
                }
            }]
        },
        "non_claims": [
            "does not judge whether a manifest change is malicious",
            "does not infer tools outside the observed tools/list",
            "does not detect behavior drift under identical metadata",
            "privileged is classifier-derived, not the server's own annotations"
        ]
    })
}

fn write_json(path: &Path, value: &Value) -> Vec<u8> {
    let bytes = serde_json::to_vec_pretty(value).unwrap();
    fs::write(path, &bytes).unwrap();
    bytes
}

fn padded_json(mut bytes: Vec<u8>, len: usize) -> Vec<u8> {
    assert!(bytes.len() <= len);
    bytes.resize(len, b' ');
    bytes
}

fn candidate_command(observed: &Path, candidate: &Path) -> Command {
    let mut command = Command::cargo_bin("assay").unwrap();
    command.args([
        "mcp",
        "manifest",
        "candidate",
        "--from-observed",
        observed.to_str().unwrap(),
        "--out",
        candidate.to_str().unwrap(),
    ]);
    command
}

fn promote_command(candidate: &Path, source: &Path, declared: &Path) -> Command {
    let mut command = Command::cargo_bin("assay").unwrap();
    command.args([
        "mcp",
        "manifest",
        "promote",
        "--candidate",
        candidate.to_str().unwrap(),
        "--source",
        source.to_str().unwrap(),
        "--out",
        declared.to_str().unwrap(),
    ]);
    command
}

fn create_candidate(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let observed = dir.join("observed.json");
    let candidate = dir.join("candidate.json");
    write_json(&observed, &promotable_observed());
    candidate_command(&observed, &candidate).assert().success();
    (observed, candidate)
}

fn assert_promotion_failure(candidate: &Path, source: &Path, declared: &Path) {
    let output = promote_command(candidate, source, declared)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "invalid promotion unexpectedly minted a declared manifest"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("fatal:"),
        "invalid promotion did not return a normal CLI error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !declared.exists(),
        "invalid promotion left a declared output behind"
    );
}

fn assert_candidate_failure(label: &str, source: &[u8], expected_error: &str) {
    let dir = tempfile::tempdir().unwrap();
    let observed = dir.path().join("observed.json");
    let candidate = dir.path().join("candidate.json");
    fs::write(&observed, source).unwrap();

    let output = candidate_command(&observed, &candidate).output().unwrap();
    assert!(
        !output.status.success(),
        "{label} unexpectedly produced a candidate"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected_error),
        "{label} returned the wrong error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !candidate.exists(),
        "{label} left a candidate output behind"
    );
}

#[test]
fn candidate_then_promotion_mints_a_new_declared_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let observed = dir.path().join("observed.json");
    let candidate = dir.path().join("candidate.json");
    let declared = dir.path().join("declared.json");
    let source = write_json(&observed, &promotable_observed());

    candidate_command(&observed, &candidate).assert().success();

    let candidate_bytes = fs::read(&candidate).unwrap();
    let candidate_json: Value = serde_json::from_slice(&candidate_bytes).unwrap();
    assert_eq!(candidate_json["schema"], "assay.mcp_manifest_candidate.v0");
    assert_eq!(candidate_json["status"], "candidate");
    assert_eq!(candidate_json["approval"], "not_approved");
    assert_eq!(
        candidate_json["source_sha256"],
        format!("sha256:{}", hex::encode(Sha256::digest(&source)))
    );
    assert!(candidate_bytes.ends_with(b"\n"));

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "mcp",
            "manifest",
            "promote",
            "--candidate",
            candidate.to_str().unwrap(),
            "--source",
            observed.to_str().unwrap(),
            "--out",
            declared.to_str().unwrap(),
        ])
        .assert()
        .success();

    let declared_bytes = fs::read(&declared).unwrap();
    let declared_text = std::str::from_utf8(&declared_bytes).unwrap();
    let parsed = assay_mcp_server::declared_manifest::parse_declared_manifest(declared_text)
        .expect("promotion must mint a valid declared-v0 artifact");
    assert_eq!(parsed.tools.len(), 1);
    assert_eq!(parsed.tools[0].name, "echo");
    assert!(declared_bytes.ends_with(b"\n"));
}

#[test]
fn candidate_refuses_non_promotable_observations_without_output() {
    let cases: [(&str, &str, JsonMutation); 4] = [
        (
            "ambiguous",
            "source must be an observed",
            Box::new(|value| value["status"] = json!("ambiguous")),
        ),
        (
            "partial",
            "complete, unambiguous tools/list",
            Box::new(|value| value["observed"]["tools_list_complete"] = json!("partial")),
        ),
        (
            "null-digest",
            "complete, unambiguous tools/list",
            Box::new(|value| value["observed"]["manifest_digest"] = Value::Null),
        ),
        (
            "empty-tools",
            "non-empty, count-consistent tool list",
            Box::new(|value| {
                value["observed"]["tool_count"] = json!(0);
                value["observed"]["tool_digests"] = json!([]);
                value["observed"]["manifest_digest"] = json!(manifest_digest(&[]));
            }),
        ),
    ];

    for (name, expected_error, mutate) in cases {
        let mut value = promotable_observed();
        mutate(&mut value);
        let source = serde_json::to_vec_pretty(&value).unwrap();
        assert_candidate_failure(name, &source, expected_error);
    }
}

#[test]
fn candidate_refuses_sources_that_do_not_strictly_describe_their_own_bytes() {
    let mut unknown_member = promotable_observed();
    unknown_member["unexpected"] = json!(true);
    assert_candidate_failure(
        "unknown-member",
        &serde_json::to_vec_pretty(&unknown_member).unwrap(),
        "unknown field",
    );

    let mut unknown_field_digest = promotable_observed();
    unknown_field_digest["observed"]["tool_digests"][0]["field_digests"]["invented"] =
        json!(digest(9));
    assert_candidate_failure(
        "unknown-field-digest",
        &serde_json::to_vec_pretty(&unknown_field_digest).unwrap(),
        "unknown field_digest key",
    );

    let mut stale_manifest_digest = promotable_observed();
    stale_manifest_digest["observed"]["manifest_digest"] = json!(digest(9));
    assert_candidate_failure(
        "stale-manifest-digest",
        &serde_json::to_vec_pretty(&stale_manifest_digest).unwrap(),
        "cannot be reproduced",
    );

    let sanitized_identity_loss = build_observed(
        "local",
        &[json!({"name": "escape\u{1}tool"})],
        Completeness::Complete,
    );
    assert_candidate_failure(
        "sanitized-identity-loss",
        &serde_json::to_vec_pretty(&sanitized_identity_loss).unwrap(),
        "cannot be reproduced",
    );

    let duplicated_status =
        String::from_utf8(serde_json::to_vec_pretty(&promotable_observed()).unwrap())
            .unwrap()
            .replacen(
                "\"status\": \"observed\",",
                "\"status\": \"observed\",\n  \"status\": \"observed\",",
                1,
            );
    assert_candidate_failure(
        "duplicate-member",
        duplicated_status.as_bytes(),
        "duplicate member",
    );
}

#[test]
fn candidate_refuses_observed_contract_claim_drift() {
    let mut source = promotable_observed();
    source["non_claims"][0] = json!("all observed tools are safe");
    assert_candidate_failure(
        "observed-non-claim-drift",
        &serde_json::to_vec_pretty(&source).unwrap(),
        "non_claims",
    );
}

#[test]
fn promotion_binds_exact_source_bytes_and_every_candidate_field() {
    let dir = tempfile::tempdir().unwrap();
    let (observed, candidate) = create_candidate(dir.path());

    let identical_source = dir.path().join("same-bytes-at-another-path.json");
    fs::copy(&observed, &identical_source).unwrap();
    let declared_from_copy = dir.path().join("declared-from-copy.json");
    promote_command(&candidate, &identical_source, &declared_from_copy)
        .assert()
        .success();

    let drifted_source = dir.path().join("drifted-source.json");
    let mut drifted_bytes = fs::read(&observed).unwrap();
    drifted_bytes.push(b'\n');
    fs::write(&drifted_source, drifted_bytes).unwrap();
    assert_promotion_failure(
        &candidate,
        &drifted_source,
        &dir.path().join("declared-from-drift.json"),
    );

    let original_candidate: Value = serde_json::from_slice(&fs::read(&candidate).unwrap()).unwrap();
    let mutations: [(&str, JsonMutation); 9] = [
        (
            "schema",
            Box::new(|value| value["schema"] = json!("assay.mcp_manifest_candidate.v1")),
        ),
        (
            "status",
            Box::new(|value| value["status"] = json!("approved")),
        ),
        (
            "approval",
            Box::new(|value| value["approval"] = json!("approved")),
        ),
        (
            "source-sha",
            Box::new(|value| value["source_sha256"] = json!(digest(8))),
        ),
        (
            "canonicalization",
            Box::new(|value| value["canonicalization"] = json!("other")),
        ),
        (
            "manifest-digest",
            Box::new(|value| value["manifest_digest"] = json!(digest(8))),
        ),
        (
            "server",
            Box::new(|value| value["server"]["id"] = json!("other-server")),
        ),
        (
            "tools",
            Box::new(|value| value["tools"][0]["name"] = json!("other-tool")),
        ),
        (
            "non-claims",
            Box::new(|value| value["non_claims"][0] = json!("candidate is approved")),
        ),
    ];

    for (name, mutate) in mutations {
        let mut value = original_candidate.clone();
        mutate(&mut value);
        let changed_candidate = dir.path().join(format!("candidate-{name}.json"));
        write_json(&changed_candidate, &value);
        assert_promotion_failure(
            &changed_candidate,
            &observed,
            &dir.path().join(format!("declared-{name}.json")),
        );
    }
}

#[test]
fn candidate_and_promotion_never_overwrite_existing_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let observed = dir.path().join("observed.json");
    write_json(&observed, &promotable_observed());

    let candidate = dir.path().join("candidate.json");
    let candidate_sentinel = b"operator candidate notes\n";
    fs::write(&candidate, candidate_sentinel).unwrap();
    let candidate_output = candidate_command(&observed, &candidate).output().unwrap();
    assert!(!candidate_output.status.success());
    assert_eq!(fs::read(&candidate).unwrap(), candidate_sentinel);

    let generated_candidate = dir.path().join("generated-candidate.json");
    candidate_command(&observed, &generated_candidate)
        .assert()
        .success();
    let declared = dir.path().join("declared.json");
    let declared_sentinel = b"operator-approved baseline\n";
    fs::write(&declared, declared_sentinel).unwrap();
    let declared_output = promote_command(&generated_candidate, &observed, &declared)
        .output()
        .unwrap();
    assert!(!declared_output.status.success());
    assert_eq!(fs::read(&declared).unwrap(), declared_sentinel);

    let names = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        names,
        [
            "candidate.json",
            "declared.json",
            "generated-candidate.json",
            "observed.json",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        "failed create-new writes left temporary residue"
    );
}

#[test]
fn generated_candidate_is_not_a_declared_approval_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let (_observed, candidate) = create_candidate(dir.path());

    let error = assay_mcp_server::declared_manifest::load_declared_manifest(&candidate)
        .expect_err("candidate schema must not be accepted as declared approval");
    assert!(
        error.to_string().contains("candidate") || error.to_string().contains("schema"),
        "candidate refusal should identify the schema boundary: {error:#}"
    );
}

#[test]
fn all_manifest_reads_share_the_same_exact_byte_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let base_observed = serde_json::to_vec(&promotable_observed()).unwrap();

    let observed_exact = dir.path().join("observed-exact.json");
    fs::write(
        &observed_exact,
        padded_json(base_observed.clone(), MAX_MANIFEST_BYTES),
    )
    .unwrap();
    let candidate_from_exact = dir.path().join("candidate-from-exact.json");
    candidate_command(&observed_exact, &candidate_from_exact)
        .assert()
        .success();

    let observed_over = dir.path().join("observed-over.json");
    fs::write(
        &observed_over,
        padded_json(base_observed.clone(), MAX_MANIFEST_BYTES + 1),
    )
    .unwrap();
    let output = candidate_command(&observed_over, &dir.path().join("candidate-from-over.json"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("manifest limit"));

    let observed = dir.path().join("observed.json");
    fs::write(&observed, &base_observed).unwrap();
    let candidate = dir.path().join("candidate.json");
    candidate_command(&observed, &candidate).assert().success();
    let candidate_bytes = fs::read(&candidate).unwrap();

    let candidate_exact = dir.path().join("candidate-exact.json");
    fs::write(
        &candidate_exact,
        padded_json(candidate_bytes.clone(), MAX_MANIFEST_BYTES),
    )
    .unwrap();
    let declared_from_exact_candidate = dir.path().join("declared-from-exact-candidate.json");
    promote_command(&candidate_exact, &observed, &declared_from_exact_candidate)
        .assert()
        .success();

    let candidate_over = dir.path().join("candidate-over.json");
    fs::write(
        &candidate_over,
        padded_json(candidate_bytes, MAX_MANIFEST_BYTES + 1),
    )
    .unwrap();
    let candidate_over_output = promote_command(
        &candidate_over,
        &observed,
        &dir.path().join("declared-from-over-candidate.json"),
    )
    .output()
    .unwrap();
    assert!(!candidate_over_output.status.success());
    assert!(String::from_utf8_lossy(&candidate_over_output.stderr).contains("manifest limit"));

    let promotion_source_over = dir.path().join("promotion-source-over.json");
    fs::write(
        &promotion_source_over,
        padded_json(base_observed, MAX_MANIFEST_BYTES + 1),
    )
    .unwrap();
    let source_over_output = promote_command(
        &candidate,
        &promotion_source_over,
        &dir.path().join("declared-from-over-source.json"),
    )
    .output()
    .unwrap();
    assert!(!source_over_output.status.success());
    assert!(String::from_utf8_lossy(&source_over_output.stderr).contains("manifest limit"));

    let declared_bytes = fs::read(&declared_from_exact_candidate).unwrap();
    let declared_exact = dir.path().join("declared-exact.json");
    fs::write(
        &declared_exact,
        padded_json(declared_bytes.clone(), MAX_MANIFEST_BYTES),
    )
    .unwrap();
    assay_mcp_server::declared_manifest::load_declared_manifest(&declared_exact)
        .expect("exact-limit declared manifest must load");

    let declared_over = dir.path().join("declared-over.json");
    fs::write(
        &declared_over,
        padded_json(declared_bytes, MAX_MANIFEST_BYTES + 1),
    )
    .unwrap();
    let error = assay_mcp_server::declared_manifest::load_declared_manifest(&declared_over)
        .expect_err("declared manifest above the shared limit must fail");
    assert!(error.to_string().contains("manifest limit"));
}

#[test]
fn candidate_and_declared_outputs_are_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let observed = dir.path().join("observed.json");
    write_json(&observed, &promotable_observed());

    let candidate_a = dir.path().join("candidate-a.json");
    let candidate_b = dir.path().join("candidate-b.json");
    candidate_command(&observed, &candidate_a)
        .assert()
        .success();
    candidate_command(&observed, &candidate_b)
        .assert()
        .success();
    assert_eq!(
        fs::read(&candidate_a).unwrap(),
        fs::read(&candidate_b).unwrap()
    );

    let declared_a = dir.path().join("declared-a.json");
    let declared_b = dir.path().join("declared-b.json");
    promote_command(&candidate_a, &observed, &declared_a)
        .assert()
        .success();
    promote_command(&candidate_b, &observed, &declared_b)
        .assert()
        .success();
    assert_eq!(
        fs::read(&declared_a).unwrap(),
        fs::read(&declared_b).unwrap()
    );
}

#[test]
fn promotion_refuses_unknown_and_duplicate_candidate_members() {
    let dir = tempfile::tempdir().unwrap();
    let (observed, candidate) = create_candidate(dir.path());
    let candidate_bytes = fs::read(&candidate).unwrap();

    let mut unknown: Value = serde_json::from_slice(&candidate_bytes).unwrap();
    unknown["unexpected"] = json!(true);
    let unknown_path = dir.path().join("candidate-unknown.json");
    write_json(&unknown_path, &unknown);
    assert_promotion_failure(
        &unknown_path,
        &observed,
        &dir.path().join("declared-from-unknown.json"),
    );

    let mut unknown_field_digest: Value = serde_json::from_slice(&candidate_bytes).unwrap();
    unknown_field_digest["tools"][0]["field_digests"]["invented"] = json!(digest(9));
    let unknown_field_path = dir.path().join("candidate-unknown-field.json");
    write_json(&unknown_field_path, &unknown_field_digest);
    assert_promotion_failure(
        &unknown_field_path,
        &observed,
        &dir.path().join("declared-from-unknown-field.json"),
    );

    let duplicated_status = String::from_utf8(candidate_bytes).unwrap().replacen(
        "\"status\": \"candidate\",",
        "\"status\": \"candidate\",\n  \"status\": \"candidate\",",
        1,
    );
    let duplicate_path = dir.path().join("candidate-duplicate.json");
    fs::write(&duplicate_path, duplicated_status).unwrap();
    assert_promotion_failure(
        &duplicate_path,
        &observed,
        &dir.path().join("declared-from-duplicate.json"),
    );
}
