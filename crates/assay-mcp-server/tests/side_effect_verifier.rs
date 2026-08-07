//! Eb guard: the producer and verifier reproduce the Ea fixtures, and the honesty rules bite.
//!
//! Ea (`side_effect_fixtures.rs`) proved the vectors are internally consistent and the binding math
//! is sound. This proves the *code* agrees with them, which is a different claim: Ea would still pass
//! if no verifier existed, and did.

use assay_mcp_server::side_effect::{
    binding_digest, check_audit_record, promote_with_audit_record, promote_with_observed_read,
    subject_from_action, AuditBinding, SideEffect, SideEffectLevel, VerificationSource,
};
use serde_json::{json, Value};
use std::path::PathBuf;

fn fx(name: &str) -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/side_effect")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()
}

fn decision0(v: &Value) -> Value {
    v["observed_tool_decisions"][0].clone()
}

// ---------------------------------------------------------------- the projection

#[test]
fn the_action_projection_reproduces_the_committed_binding() {
    // The load-bearing claim of Eb.2: the subject we derive from an observed call is byte-identical
    // to the one the independently produced audit record carries. If this drifts, every `verified`
    // in the corpus is unreachable and the ladder silently tops out at asserted.
    let action = decision0(&fx("verified.json"))["action"].clone();
    let record = fx("audit_record_github_deploy_key.json");

    let subject = subject_from_action(&action).expect("classified action must project");
    assert_eq!(
        binding_digest(&subject).unwrap(),
        record["binding_digest"].as_str().unwrap(),
        "projected subject must reproduce the audit record's binding"
    );
}

#[test]
fn the_projection_drops_request_side_fields() {
    // `provider` and `read_only` describe our request, not the effect. An audit entry written by a
    // system that never saw our request cannot carry them, so including them would make every
    // binding unmatchable.
    let action = decision0(&fx("verified.json"))["action"].clone();
    let subject = subject_from_action(&action).unwrap();
    let target = subject["target"].as_object().unwrap();

    assert!(target.contains_key("owner") && target.contains_key("repo"));
    assert!(!target.contains_key("provider"), "provider is request-side");
    assert!(
        !target.contains_key("read_only"),
        "read_only is request-side"
    );
}

#[test]
fn an_unclassified_action_does_not_project() {
    // Minting a subject from an unclassified action would produce a binding that matches nothing,
    // which reads as "no audit record found" rather than "this call cannot be bound".
    for action in [
        json!({"resource_type": null, "verb": "create", "target": {"owner": "o"}}),
        json!({"resource_type": "x", "verb": null, "target": {"owner": "o"}}),
        json!({"resource_type": "x", "verb": "create", "target": {}}),
    ] {
        assert!(
            subject_from_action(&action).is_none(),
            "{action} must not project"
        );
    }
}

// ---------------------------------------------------------------- the two spec checks

#[test]
fn a_matching_record_binds_and_promotes_to_verified() {
    let decision = decision0(&fx("verified.json"));
    let record = fx("audit_record_github_deploy_key.json");

    let (se, binding) =
        promote_with_audit_record(SideEffect::asserted(true), &record, &decision["action"]);

    assert!(binding.is_bound());
    assert_eq!(se.level, SideEffectLevel::Verified);
    assert_eq!(
        se.verification_source,
        Some(VerificationSource::ProviderAuditImport)
    );
    assert_eq!(
        se.verification_subject_digest.as_deref(),
        decision["response"]["side_effect"]["verification_subject_digest"].as_str(),
        "the produced digest must equal the one the fixture commits"
    );
    assert!(se.verified_flag(), "verified level sets the compat boolean");
}

#[test]
fn the_mismatch_record_stays_asserted_and_is_reported() {
    // The rule the spec states outright: a record that fails a check leaves the level at asserted
    // and is reported, never silently promoted and never silently dropped.
    let decision = decision0(&fx("verified.json"));
    let record = fx("audit_record_mismatch.json");

    let (se, binding) =
        promote_with_audit_record(SideEffect::asserted(true), &record, &decision["action"]);

    assert_eq!(se.level, SideEffectLevel::Asserted, "must not promote");
    assert!(!se.verified_flag());
    assert!(se.verification_source.is_none());
    match binding {
        AuditBinding::BindsDifferentCall {
            record_digest,
            call_digest,
        } => {
            assert_ne!(record_digest, call_digest);
        }
        other => panic!("expected BindsDifferentCall, got {other:?}"),
    }
}

#[test]
fn an_internally_inconsistent_record_is_rejected_before_the_call_check() {
    // A record whose own binding_digest does not recompute is not evidence of anything, and saying
    // so is more useful than reporting that it failed to match the call.
    let decision = decision0(&fx("verified.json"));
    let mut record = fx("audit_record_github_deploy_key.json");
    record["binding_digest"] = json!("sha256:deadbeef");

    match check_audit_record(&record, &decision["action"]) {
        AuditBinding::RecordInconsistent { declared, .. } => {
            assert_eq!(declared, "sha256:deadbeef")
        }
        other => panic!("expected RecordInconsistent, got {other:?}"),
    }
}

#[test]
fn a_tampered_subject_cannot_keep_its_binding() {
    // The property that makes the binding worth computing: change what the record says happened and
    // the digest stops matching, so the record can no longer promote anything.
    let decision = decision0(&fx("verified.json"));
    let mut record = fx("audit_record_github_deploy_key.json");
    record["subject"]["target"]["repo"] = json!("attacker-repo");

    let (se, binding) =
        promote_with_audit_record(SideEffect::asserted(true), &record, &decision["action"]);
    assert_eq!(se.level, SideEffectLevel::Asserted);
    assert!(matches!(binding, AuditBinding::RecordInconsistent { .. }));
}

// ---------------------------------------------------------------- promotion discipline

#[test]
fn asserted_never_auto_promotes() {
    let se = SideEffect::asserted(true);
    assert_eq!(se.level, SideEffectLevel::Asserted);
    assert!(se.verification_source.is_none());
    assert!(se.verification_subject_digest.is_none());
    assert!(!se.verified_flag());
}

#[test]
fn an_unasserted_side_effect_cannot_be_verified() {
    // Nothing was claimed, so there is nothing to bind. A denied or failed call must not become
    // `verified` because an audit record happens to exist for that shape of action.
    let decision = decision0(&fx("verified.json"));
    let record = fx("audit_record_github_deploy_key.json");

    let (se, binding) =
        promote_with_audit_record(SideEffect::asserted(false), &record, &decision["action"]);
    assert_eq!(se.level, SideEffectLevel::Asserted);
    assert!(!binding.is_bound());
}

#[test]
fn observed_confirmed_is_sequence_evidence_and_not_verified() {
    let write = decision0(&fx("verified.json"))["action"].clone();
    let read = json!({
        "resource_type": "github_deploy_key",
        "verb": "list",
        "target": write["target"].clone(),
    });

    let se = promote_with_observed_read(SideEffect::asserted(true), &write, &read);
    assert_eq!(se.level, SideEffectLevel::ObservedConfirmed);
    assert_eq!(
        se.verification_source,
        Some(VerificationSource::ObservedReadFollowup)
    );
    assert!(
        !se.verified_flag(),
        "observed_confirmed is in-run sequence evidence, never external verification"
    );
    assert_eq!(
        se.verification_subject_digest.as_deref(),
        decision0(&fx("observed_confirmed.json"))["response"]["side_effect"]
            ["verification_subject_digest"]
            .as_str(),
        "must match the digest the observed_confirmed fixture commits"
    );
}

#[test]
fn a_read_of_a_different_target_does_not_confirm() {
    let write = decision0(&fx("verified.json"))["action"].clone();
    let mut read = write.clone();
    read["target"]["repo"] = json!("some-other-repo");

    let se = promote_with_observed_read(SideEffect::asserted(true), &write, &read);
    assert_eq!(
        se.level,
        SideEffectLevel::Asserted,
        "same shape is not the same call"
    );
}

#[test]
fn an_observed_read_cannot_downgrade_or_overwrite_a_verified_level() {
    let decision = decision0(&fx("verified.json"));
    let record = fx("audit_record_github_deploy_key.json");
    let (verified, _) =
        promote_with_audit_record(SideEffect::asserted(true), &record, &decision["action"]);

    let after =
        promote_with_observed_read(verified.clone(), &decision["action"], &decision["action"]);
    assert_eq!(
        after.level,
        SideEffectLevel::Verified,
        "sequence evidence must not overwrite audit"
    );
    assert_eq!(after, verified);
}

// ---------------------------------------------------------------- serialization contract

#[test]
fn the_produced_block_serializes_to_the_committed_shape() {
    let decision = decision0(&fx("asserted.json"));
    let produced = serde_json::to_value(SideEffect::asserted(true)).unwrap();

    // `asserted.json` commits explicit nulls; the producer omits absent fields. Both must read as
    // "no verification", so compare the meaning rather than the bytes.
    let committed = &decision["response"]["side_effect"];
    assert_eq!(produced["level"], committed["level"]);
    assert_eq!(produced["asserted"], committed["asserted"]);
    assert!(produced.get("verification_source").is_none());
    assert!(committed["verification_source"].is_null());
}

#[test]
fn levels_serialize_to_the_pinned_wire_names() {
    for (level, wire) in [
        (SideEffectLevel::Asserted, "asserted"),
        (SideEffectLevel::ObservedConfirmed, "observed_confirmed"),
        (SideEffectLevel::Verified, "verified"),
    ] {
        assert_eq!(serde_json::to_value(level).unwrap(), json!(wire));
    }
}
