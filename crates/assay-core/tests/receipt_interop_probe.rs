use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
enum VerificationResult {
    Valid,
    Invalid,
    Malformed,
    Unchecked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProbeEvidenceView {
    receipt_present: bool,
    verification_result: VerificationResult,
    issuer_id: Option<String>,
    claimed_issuer_tier: Option<String>,
    policy_digest: Option<String>,
    spec: Option<String>,
    tool_name: Option<String>,
    decision: Option<String>,
    binding_key: Option<String>,
}

impl ProbeEvidenceView {
    fn elevatable_fields(&self) -> Vec<&'static str> {
        vec!["verification_result"]
    }
}

#[test]
fn contract_probe_fixture_inventory_is_complete() {
    let fixture_dir = fixture_dir();
    let expected = [
        "README.md",
        "issuer_key.json",
        "malformed.json",
        "tampered.json",
        "valid_allow.json",
        "valid_deny.json",
    ];

    let mut actual: Vec<String> = fs::read_dir(&fixture_dir)
        .expect("read fixture dir")
        .map(|entry| {
            entry
                .expect("valid fixture entry")
                .file_name()
                .into_string()
                .expect("utf-8 filename")
        })
        .collect();
    actual.sort();

    assert_eq!(actual, expected);
}

#[test]
fn contract_probe_issuer_key_fixture_is_loadable() {
    let value = read_fixture_json("issuer_key.json");

    assert_eq!(value["kid"], "sb:issuer:1feed234c727");
    assert_eq!(value["alg"], "EdDSA");
    assert_eq!(
        value["public_key_hex"],
        "1feed234c727d30cf5b5f7f27df5d6168620bd8fcfd0a565efea65c08e93ed89"
    );
}

#[test]
fn contract_probe_valid_allow_maps_to_observed_view() {
    let view = derive_probe_evidence("valid_allow.json").expect("valid allow view");

    assert_eq!(
        view,
        ProbeEvidenceView {
            receipt_present: true,
            verification_result: VerificationResult::Unchecked,
            issuer_id: Some("sb:issuer:1feed234c727".to_string()),
            claimed_issuer_tier: Some("self-signed".to_string()),
            policy_digest: Some("sha256:9d0fd4c9e72c1d5d8a3b7f2e1c4d6a8b".to_string()),
            spec: Some("draft-farley-acta-signed-receipts-01".to_string()),
            tool_name: Some("read_file".to_string()),
            decision: Some("allow".to_string()),
            binding_key: Some(
                "sb:issuer:1feed234c727|sess_a1b2c3d4e5f6|1|sha256:ff7e2796c004bfab32b6b81f06e4a70e21ca0b5231398b15035b0d4582331c76"
                    .to_string(),
            ),
        }
    );
    assert_eq!(view.elevatable_fields(), vec!["verification_result"]);
}

#[test]
fn contract_probe_valid_deny_maps_to_observed_view() {
    let view = derive_probe_evidence("valid_deny.json").expect("valid deny view");

    assert_eq!(
        view,
        ProbeEvidenceView {
            receipt_present: true,
            verification_result: VerificationResult::Unchecked,
            issuer_id: Some("sb:issuer:1feed234c727".to_string()),
            claimed_issuer_tier: Some("self-signed".to_string()),
            policy_digest: Some("sha256:9d0fd4c9e72c1d5d8a3b7f2e1c4d6a8b".to_string()),
            spec: Some("draft-farley-acta-signed-receipts-01".to_string()),
            tool_name: Some("execute_command".to_string()),
            decision: Some("deny".to_string()),
            binding_key: Some(
                "sb:issuer:1feed234c727|sess_a1b2c3d4e5f6|2|sha256:5c7923bd67b06c93279d49c466301c57023822eec29c49e269063e47aecd973c"
                    .to_string(),
            ),
        }
    );
    assert_eq!(view.elevatable_fields(), vec!["verification_result"]);
}

#[test]
fn contract_probe_tampered_receipt_stays_unchecked_and_unpromoted() {
    let view = derive_probe_evidence("tampered.json").expect("tampered view");

    assert_eq!(view.receipt_present, true);
    assert_eq!(view.verification_result, VerificationResult::Unchecked);
    assert_eq!(view.tool_name.as_deref(), Some("delete_database"));
    assert_eq!(view.decision.as_deref(), Some("deny"));
    assert_eq!(
        view.binding_key.as_deref(),
        Some(
            "sb:issuer:1feed234c727|sess_a1b2c3d4e5f6|1|sha256:ff7e2796c004bfab32b6b81f06e4a70e21ca0b5231398b15035b0d4582331c76"
        )
    );
    assert_eq!(view.elevatable_fields(), vec!["verification_result"]);
}

#[test]
fn contract_probe_malformed_receipt_is_rejected_as_malformed() {
    let err = derive_probe_evidence("malformed.json").expect_err("malformed receipt must fail");

    assert!(
        err.contains("missing or invalid string field \"decision\""),
        "unexpected malformed error: {err}"
    );

    let view = derive_malformed_probe_evidence("malformed.json");
    assert_eq!(view.receipt_present, false);
    assert_eq!(view.verification_result, VerificationResult::Malformed);
    assert_eq!(view.elevatable_fields(), vec!["verification_result"]);
}

#[test]
fn contract_probe_binding_key_is_deterministic() {
    let first = derive_probe_evidence("valid_allow.json").expect("first load");
    let second = derive_probe_evidence("valid_allow.json").expect("second load");

    assert_eq!(first.binding_key, second.binding_key);
}

#[test]
fn contract_probe_valid_and_invalid_variants_remain_reserved_for_future_boundary_work() {
    let reserved = [
        VerificationResult::Valid,
        VerificationResult::Invalid,
        VerificationResult::Malformed,
        VerificationResult::Unchecked,
    ];

    assert_eq!(reserved.len(), 4);
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/interop/protect_mcp_receipts")
}

fn read_fixture_json(name: &str) -> Value {
    let path = fixture_dir().join(name);
    let text = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!("failed to read fixture {}: {err}", path.display());
    });
    serde_json::from_str(&text).unwrap_or_else(|err| {
        panic!("failed to parse fixture {}: {err}", path.display());
    })
}

fn derive_probe_evidence(name: &str) -> Result<ProbeEvidenceView, String> {
    let value = read_fixture_json(name);
    validate_minimal_shape(&value)?;

    let issuer_id = get_string(&value, "issuer_id")?;
    let session_id = get_string(&value, "session_id")?;
    let sequence = get_u64(&value, "sequence")?;
    let tool_input_hash = get_string(&value, "tool_input_hash")?;

    Ok(ProbeEvidenceView {
        receipt_present: true,
        verification_result: VerificationResult::Unchecked,
        issuer_id: Some(issuer_id.clone()),
        claimed_issuer_tier: Some(get_string(&value, "claimed_issuer_tier")?),
        policy_digest: Some(get_string(&value, "policy_digest")?),
        spec: Some(get_string(&value, "spec")?),
        tool_name: Some(get_string(&value, "tool_name")?),
        decision: Some(get_string(&value, "decision")?),
        binding_key: Some(format!(
            "{issuer_id}|{session_id}|{sequence}|{tool_input_hash}"
        )),
    })
}

fn derive_malformed_probe_evidence(_name: &str) -> ProbeEvidenceView {
    ProbeEvidenceView {
        receipt_present: false,
        verification_result: VerificationResult::Malformed,
        issuer_id: None,
        claimed_issuer_tier: None,
        policy_digest: None,
        spec: None,
        tool_name: None,
        decision: None,
        binding_key: None,
    }
}

fn validate_minimal_shape(value: &Value) -> Result<(), String> {
    let type_name = get_string(value, "type")?;
    if type_name != "protectmcp:decision" {
        return Err(format!(
            "unexpected type {:?}, expected \"protectmcp:decision\"",
            type_name
        ));
    }

    let decision = get_string(value, "decision")?;
    if decision != "allow" && decision != "deny" {
        return Err(format!(
            "unexpected decision {:?}, expected \"allow\" or \"deny\"",
            decision
        ));
    }

    get_string(value, "tool_name")?;
    get_string(value, "tool_input_hash")?;
    get_string(value, "policy_digest")?;
    get_string(value, "issued_at")?;
    get_string(value, "issuer_id")?;
    get_string(value, "spec")?;
    get_string(value, "claimed_issuer_tier")?;
    get_string(value, "session_id")?;
    get_u64(value, "sequence")?;

    let signature = value
        .get("signature")
        .and_then(Value::as_object)
        .ok_or_else(|| "missing or invalid object field \"signature\"".to_string())?;
    get_string_from_object(signature, "alg")?;
    get_string_from_object(signature, "kid")?;
    get_string_from_object(signature, "sig")?;

    Ok(())
}

fn get_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing or invalid string field \"{field}\""))
}

fn get_u64(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing or invalid integer field \"{field}\""))
}

fn get_string_from_object(
    value: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing or invalid string field \"signature.{field}\""))
}
