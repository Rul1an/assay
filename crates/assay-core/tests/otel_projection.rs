//! The OTel/OpenInference projection is a one-directional, lossy view of assay evidence. These tests
//! pin the mapping and the honesty invariants: a tool span is observed (never carries enforcement),
//! enforcement lives in its own attribute set, absence of enforcement makes no claim, and the
//! standards fields coexist with the assay.* qualifiers.

use assay_core::otel::projection::{project, PROJECTION_SCHEMA, SEMCONV_VERSION};
use serde_json::json;

fn surface() -> serde_json::Value {
    json!({
        "schema": "assay.runner.capability_surface.v0",
        "filesystem_paths": ["/workspace/app"],
        "network_endpoints": ["203.0.113.10:443"],
        "process_execs": [],
        "mcp_tools": ["search"],
        "policy_decisions": ["allow:read_file", "deny:write_file"]
    })
}

fn observation() -> serde_json::Value {
    json!({
        "schema": "assay.runner.observation_health.v0",
        "kernel_layer": "complete",
        "network_protocol_coverage": "connect_only",
        "policy_layer": "present"
    })
}

fn enforcement() -> serde_json::Value {
    json!({
        "schema": "assay.enforcement_health.v0",
        "network_enforcement": "active",
        "attach_confirmed": true,
        "blocked_count": 1,
        "allowed_count": 2,
        "scope": "ipv4_tcp_connect"
    })
}

#[test]
fn schema_and_version_are_pinned() {
    let p = project(&surface(), None, None);
    assert_eq!(p.schema, PROJECTION_SCHEMA);
    assert_eq!(p.semconv_version, SEMCONV_VERSION);
    assert!(!p.non_claims.is_empty());
}

#[test]
fn tool_span_carries_genai_and_openinference_and_is_observed() {
    let p = project(&surface(), Some(&observation()), Some(&enforcement()));
    let tool = p
        .spans
        .iter()
        .find(|s| {
            s.attributes
                .get("gen_ai.tool.name")
                .and_then(|v| v.as_str())
                == Some("search")
        })
        .expect("tool span present");
    assert_eq!(tool.kind, "INTERNAL");
    assert_eq!(
        tool.attributes
            .get("gen_ai.operation.name")
            .and_then(|v| v.as_str()),
        Some("execute_tool")
    );
    // OpenInference span.kind coexists with gen_ai.*
    assert_eq!(
        tool.attributes
            .get("openinference.span.kind")
            .and_then(|v| v.as_str()),
        Some("TOOL")
    );
    // Honesty: a capability-surface tool is observed.
    assert_eq!(
        tool.attributes
            .get("assay.claim_class")
            .and_then(|v| v.as_str()),
        Some("observed")
    );
}

#[test]
fn enforcement_is_never_folded_into_a_tool_span() {
    // The load-bearing invariant: enforcement truth must not ride on an observed tool span.
    let p = project(&surface(), Some(&observation()), Some(&enforcement()));
    for span in &p.spans {
        for key in span.attributes.keys() {
            assert!(
                !key.starts_with("assay.enforcement."),
                "span {} leaked an enforcement attribute: {key}",
                span.name
            );
        }
    }
    // Enforcement lives only in the run-level resource attributes.
    assert_eq!(
        p.resource_attributes
            .get("assay.enforcement.network")
            .and_then(|v| v.as_str()),
        Some("active")
    );
    assert_eq!(
        p.resource_attributes
            .get("assay.enforcement.attach_confirmed")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        p.resource_attributes
            .get("assay.enforcement.scope")
            .and_then(|v| v.as_str()),
        Some("ipv4_tcp_connect")
    );
}

#[test]
fn absent_enforcement_makes_no_enforcement_claim() {
    let p = project(&surface(), Some(&observation()), None);
    let leaked: Vec<_> = p
        .resource_attributes
        .keys()
        .filter(|k| k.starts_with("assay.enforcement."))
        .collect();
    assert!(
        leaked.is_empty(),
        "absent enforcement_health must emit no enforcement attrs: {leaked:?}"
    );
}

#[test]
fn guardrail_decision_projects_with_verdict() {
    let p = project(&surface(), None, None);
    let deny = p
        .spans
        .iter()
        .find(|s| {
            s.attributes
                .get("openinference.span.kind")
                .and_then(|v| v.as_str())
                == Some("GUARDRAIL")
                && s.attributes.get("assay.tool").and_then(|v| v.as_str()) == Some("write_file")
        })
        .expect("deny guardrail span present");
    assert_eq!(
        deny.attributes
            .get("assay.decision")
            .and_then(|v| v.as_str()),
        Some("deny")
    );
}

#[test]
fn observed_sets_with_no_standard_field_stay_in_assay_namespace() {
    // The lossy part, stated: egress endpoints have no gen_ai/openinference home, so they stay assay.*
    let p = project(&surface(), Some(&observation()), None);
    let endpoints = p
        .resource_attributes
        .get("assay.capability.network_endpoints")
        .and_then(|v| v.as_array())
        .expect("network endpoints projected under assay.*");
    assert_eq!(endpoints.len(), 1);
    assert_eq!(
        p.resource_attributes
            .get("assay.observation.network_protocol_coverage")
            .and_then(|v| v.as_str()),
        Some("connect_only")
    );
}

#[test]
fn network_not_observed_is_carried_without_asserting_enforcement() {
    let oh = json!({
        "schema": "assay.runner.observation_health.v0",
        "kernel_layer": "complete",
        "network_protocol_coverage": "absent"
    });
    let p = project(&surface(), Some(&oh), None);
    assert_eq!(
        p.resource_attributes
            .get("assay.observation.network_protocol_coverage")
            .and_then(|v| v.as_str()),
        Some("absent")
    );
    // Observation says network was not seen; the projection must not invent an enforcement claim.
    assert!(p
        .resource_attributes
        .keys()
        .all(|k| !k.starts_with("assay.enforcement.")));
}
