use super::*;

#[test]
fn test_producer_meta_compact() {
    let meta = ProducerMeta::new("assay-cli", "2.6.0").with_git("abc1234");
    assert_eq!(meta.to_string_compact(), "assay-cli/2.6.0 (abc1234)");

    let meta_no_git = ProducerMeta::new("assay-cli", "2.6.0");
    assert_eq!(meta_no_git.to_string_compact(), "assay-cli/2.6.0");
}

#[test]
fn version_constants_keep_cloudevents_and_assay_axes_separate() {
    assert_eq!(CE_SPECVERSION, "1.0");
    assert_eq!(ASSAY_EVIDENCE_SPEC_VERSION, "1.0");
    assert_eq!(SPEC_VERSION, CE_SPECVERSION);

    let event = EvidenceEvent::new(
        "assay.test.event",
        "urn:assay:test",
        "run_version_constants",
        0,
        serde_json::json!({}),
    );
    assert_eq!(event.specversion, CE_SPECVERSION);
}

#[test]
fn with_semantic_digest_sets_soft_pair_set_order_invariant() {
    // The soft digest is the assay-canonical semantic digest over the payload.
    let paths = vec![vec!["passed_keys".to_string()]];
    let profile = "assay.semantic-digest.jcs-rfc8785.v1";
    let e1 = EvidenceEvent::new(
        "assay.test",
        "urn:assay:test",
        "r",
        0,
        serde_json::json!({"passed_keys": ["B", "A"]}),
    )
    .with_semantic_digest(&paths, profile)
    .unwrap();
    let e2 = EvidenceEvent::new(
        "assay.test",
        "urn:assay:test",
        "r",
        1,
        serde_json::json!({"passed_keys": ["A", "B"]}),
    )
    .with_semantic_digest(&paths, profile)
    .unwrap();
    assert_eq!(e1.semantic_digest, e2.semantic_digest);
    assert_eq!(e1.digest_profile.as_deref(), Some(profile));
    assert!(e1.semantic_digest.as_ref().unwrap().starts_with("sha256:"));
}

#[test]
fn soft_pair_absent_is_backwards_compatible() {
    let event = EvidenceEvent::new(
        "assay.test",
        "urn:assay:test",
        "r",
        0,
        serde_json::json!({}),
    );
    let json = serde_json::to_string(&event).unwrap();
    assert!(!json.contains("assaysemanticdigest"));
    assert!(!json.contains("assaydigestprofile"));
    let back: EvidenceEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back.semantic_digest, None);
    assert_eq!(back.digest_profile, None);
}

#[test]
fn soft_pair_round_trips_when_present() {
    let mut event = EvidenceEvent::new(
        "assay.test",
        "urn:assay:test",
        "r",
        0,
        serde_json::json!({}),
    );
    event.semantic_digest = Some("sha256:abc".to_string());
    event.digest_profile = Some("assay.semantic-digest.jcs-rfc8785.v1".to_string());
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("assaysemanticdigest"));
    let back: EvidenceEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back.semantic_digest.as_deref(), Some("sha256:abc"));
    assert_eq!(
        back.digest_profile.as_deref(),
        Some("assay.semantic-digest.jcs-rfc8785.v1")
    );
}

#[test]
fn tool_decision_payload_delegation_fields_are_additive() {
    let without = serde_json::json!({
        "tool": "deploy_service",
        "decision": "allow",
        "reason_code": "P_POLICY_ALLOW",
        "args_schema_hash": null
    });
    let without_payload: PayloadToolDecision =
        serde_json::from_value(without).expect("legacy payload should deserialize");
    assert_eq!(without_payload.delegated_from, None);
    assert_eq!(without_payload.delegation_depth, None);

    let with = serde_json::json!({
        "tool": "deploy_service",
        "decision": "allow",
        "reason_code": "P_POLICY_ALLOW",
        "args_schema_hash": null,
        "delegated_from": "agent:planner",
        "delegation_depth": 1
    });
    let with_payload: PayloadToolDecision =
        serde_json::from_value(with).expect("delegation payload should deserialize");
    assert_eq!(
        with_payload.delegated_from.as_deref(),
        Some("agent:planner")
    );
    assert_eq!(with_payload.delegation_depth, Some(1));
}

#[test]
fn tool_decision_payload_policy_snapshot_fields_are_additive() {
    let without = serde_json::json!({
        "tool": "deploy_service",
        "decision": "allow",
        "reason_code": "P_POLICY_ALLOW",
        "args_schema_hash": null
    });
    let without_payload: PayloadToolDecision =
        serde_json::from_value(without).expect("legacy payload should deserialize");
    assert_eq!(without_payload.policy_digest, None);
    assert_eq!(without_payload.policy_snapshot_digest, None);
    assert_eq!(without_payload.policy_snapshot_digest_alg, None);
    assert_eq!(without_payload.policy_snapshot_canonicalization, None);
    assert_eq!(without_payload.policy_snapshot_schema, None);
    assert_eq!(without_payload.tool_definition_digest, None);
    assert_eq!(without_payload.tool_definition_digest_alg, None);
    assert_eq!(without_payload.tool_definition_canonicalization, None);
    assert_eq!(without_payload.tool_definition_schema, None);
    assert_eq!(without_payload.tool_definition_source, None);

    let with = serde_json::json!({
        "tool": "deploy_service",
        "decision": "allow",
        "reason_code": "P_POLICY_ALLOW",
        "args_schema_hash": null,
        "policy_digest": "sha256:abc123",
        "policy_snapshot_digest": "sha256:abc123",
        "policy_snapshot_digest_alg": "sha256",
        "policy_snapshot_canonicalization": "jcs:mcp_policy",
        "policy_snapshot_schema": "assay.mcp.policy.snapshot.v1",
        "tool_definition_digest": "sha256:def456",
        "tool_definition_digest_alg": "sha256",
        "tool_definition_canonicalization": "jcs:mcp_tool_definition.v1",
        "tool_definition_schema": "assay.mcp.tool-definition.snapshot.v1",
        "tool_definition_source": "mcp.tools/list"
    });
    let with_payload: PayloadToolDecision =
        serde_json::from_value(with).expect("policy snapshot payload should deserialize");
    assert_eq!(with_payload.policy_digest.as_deref(), Some("sha256:abc123"));
    assert_eq!(
        with_payload.policy_snapshot_digest.as_deref(),
        Some("sha256:abc123")
    );
    assert_eq!(
        with_payload.policy_snapshot_digest_alg.as_deref(),
        Some("sha256")
    );
    assert_eq!(
        with_payload.policy_snapshot_canonicalization.as_deref(),
        Some("jcs:mcp_policy")
    );
    assert_eq!(
        with_payload.policy_snapshot_schema.as_deref(),
        Some("assay.mcp.policy.snapshot.v1")
    );
    assert_eq!(
        with_payload.tool_definition_digest.as_deref(),
        Some("sha256:def456")
    );
    assert_eq!(
        with_payload.tool_definition_digest_alg.as_deref(),
        Some("sha256")
    );
    assert_eq!(
        with_payload.tool_definition_canonicalization.as_deref(),
        Some("jcs:mcp_tool_definition.v1")
    );
    assert_eq!(
        with_payload.tool_definition_schema.as_deref(),
        Some("assay.mcp.tool-definition.snapshot.v1")
    );
    assert_eq!(
        with_payload.tool_definition_source.as_deref(),
        Some("mcp.tools/list")
    );
}

#[test]
fn test_event_id_format() {
    let event = EvidenceEvent::new(
        "assay.test",
        "urn:assay:test",
        "run_123",
        42,
        serde_json::json!({}),
    );
    assert_eq!(event.id, "run_123:42");
    assert_eq!(event.run_id, "run_123");
    assert_eq!(event.seq, 42);
}

#[test]
fn sandbox_degraded_payload_serde_shape_is_stable() {
    let payload = PayloadSandboxDegraded {
        reason_code: SandboxDegradationReasonCode::BackendUnavailable,
        degradation_mode: SandboxDegradationMode::AuditFallback,
        component: SandboxDegradationComponent::Landlock,
        detail: None,
    };

    let value = serde_json::to_value(&payload).expect("payload should serialize");
    assert_eq!(value["reason_code"], "backend_unavailable");
    assert_eq!(value["degradation_mode"], "audit_fallback");
    assert_eq!(value["component"], "landlock");
    assert!(value.get("detail").is_none(), "detail should stay optional");

    let roundtrip: PayloadSandboxDegraded =
        serde_json::from_value(value).expect("payload should deserialize");
    assert_eq!(roundtrip, payload);
}

// -- ADR-047: session-scope findings --

/// The record parses as a typed struct, and `Unknown` is not the fallback it looks like.
///
/// `PayloadSessionFinding` ships without a `Payload` variant: adding one to that exhaustive public
/// enum is a semver major (`cargo semver-checks`: `enum_variant_added`), which ADR-047 records as
/// deferred to the next major together with `#[non_exhaustive]`, so the break is paid once for
/// every kind that follows rather than once per kind.
///
/// The control is what makes this worth pinning: **there is no fallback member at all**, so an
/// unregistered kind is a deserialisation error and not an untyped landing. `Unknown(Value)` used
/// to sit here reading like a catch-all while matching only the literal tag `"Unknown"`, which no
/// producer emits; #2123 removed it rather than renaming it, because the thing the name described
/// should not exist. Forward compatibility is `EvidenceEvent::payload` staying a raw `Value`.
#[test]
fn a_session_finding_parses_as_a_typed_record_and_there_is_no_fallback_member() {
    let payload = serde_json::json!({
        "rule_id": "after:read_credentials->http_post",
        "kind": "after",
        "outcome": "violated",
        "spanned": [1, 2],
        "extent": "complete",
        "reason": "credential read at 1 followed by egress at 2"
    });
    let f: PayloadSessionFinding =
        serde_json::from_value(payload.clone()).expect("typed record parses");
    assert_eq!(f.rule_id, "after:read_credentials->http_post");
    assert_eq!(f.spanned, vec![1, 2]);
    assert_eq!(f.extent, "complete");
    assert_eq!(serde_json::to_value(&f).unwrap(), payload, "round trip");

    let stranger = serde_json::json!({
        "type": "assay.not.a.registered.kind",
        "payload": {"whatever": true}
    });
    let err = serde_json::from_value::<Payload>(stranger)
        .expect_err("an unregistered kind is a hard error, not an Unknown");
    assert!(
        err.to_string().contains("unknown variant"),
        "expected a variant error, got: {err}"
    );

    // The old shape's own tag is now just another unregistered kind. This is the assertion that
    // fails if a fallback member is ever reintroduced under any name: nothing may absorb a tag the
    // enum does not list.
    let literal = serde_json::json!({"type": "Unknown", "payload": {"anything": 1}});
    let err = serde_json::from_value::<Payload>(literal)
        .expect_err("no member absorbs an unlisted tag, including the one this used to have");
    assert!(
        err.to_string().contains("unknown variant"),
        "expected a variant error, got: {err}"
    );

    // And the layer that does carry forward compatibility still does: the raw wire keeps the bytes
    // of a kind this build has never heard of, which is why removing the member costs nothing.
    let raw: serde_json::Value =
        serde_json::from_str(r#"{"type":"assay.from.the.future","payload":{"kept":[1,2]}}"#)
            .expect("the wire is untyped and parses regardless");
    assert_eq!(raw["payload"]["kept"], serde_json::json!([1, 2]));
}
