//! The OTel/OpenInference projection is a one-directional, lossy view of assay evidence. These tests
//! pin the mapping and the honesty invariants: a tool span is observed (never carries enforcement),
//! enforcement is its OWN span (never folded onto a tool span), absence of enforcement makes no
//! claim, and the output declares itself lossy with the assay artifacts as the source of truth.

use assay_core::otel::projection::{
    project, OPENINFERENCE_SEMCONV, OTEL_GENAI_SEMCONV, PROJECTION_SCHEMA, SOURCE_OF_TRUTH,
};
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
fn header_pins_schema_versions_and_lossy_contract() {
    let p = project(&surface(), None, None);
    assert_eq!(p.schema, PROJECTION_SCHEMA);
    assert_eq!(p.semconv.otel_genai, OTEL_GENAI_SEMCONV);
    assert_eq!(p.semconv.openinference, OPENINFERENCE_SEMCONV);
    // The whole point: the view declares itself lossy and names the record.
    assert!(p.lossy);
    assert_eq!(p.source_of_truth, SOURCE_OF_TRUTH);
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
    assert_eq!(
        tool.attributes
            .get("openinference.span.kind")
            .and_then(|v| v.as_str()),
        Some("TOOL")
    );
    assert_eq!(
        tool.attributes
            .get("assay.claim_class")
            .and_then(|v| v.as_str()),
        Some("observed")
    );
}

#[test]
fn enforcement_is_its_own_span_never_on_a_tool_span() {
    // The load-bearing invariant: enforcement truth is a separate span, not attributes hung next to
    // an observed tool, so a downstream reader cannot read "tool ran" and miss "enforcement active".
    let p = project(&surface(), Some(&observation()), Some(&enforcement()));
    for span in &p.spans {
        let is_enforcement_span = span
            .attributes
            .get("assay.claim_class")
            .and_then(|v| v.as_str())
            == Some("enforcement");
        for key in span.attributes.keys() {
            if key.starts_with("assay.enforcement.") {
                assert!(
                    is_enforcement_span,
                    "enforcement attribute {key} leaked onto non-enforcement span {}",
                    span.name
                );
            }
        }
    }
    let enf = p
        .spans
        .iter()
        .find(|s| {
            s.attributes
                .get("assay.claim_class")
                .and_then(|v| v.as_str())
                == Some("enforcement")
        })
        .expect("a separate enforcement span exists");
    assert_eq!(
        enf.attributes
            .get("openinference.span.kind")
            .and_then(|v| v.as_str()),
        Some("GUARDRAIL")
    );
    assert_eq!(
        enf.attributes
            .get("assay.enforcement.network")
            .and_then(|v| v.as_str()),
        Some("active")
    );
    assert_eq!(
        enf.attributes
            .get("assay.enforcement.scope")
            .and_then(|v| v.as_str()),
        Some("ipv4_tcp_connect")
    );
    // And no tool span carries an enforcement attribute.
    let tool = p
        .spans
        .iter()
        .find(|s| {
            s.attributes
                .get("openinference.span.kind")
                .and_then(|v| v.as_str())
                == Some("TOOL")
        })
        .unwrap();
    assert!(tool
        .attributes
        .keys()
        .all(|k| !k.starts_with("assay.enforcement.")));
}

#[test]
fn absent_enforcement_makes_no_enforcement_claim() {
    let p = project(&surface(), Some(&observation()), None);
    let enforcement_spans = p
        .spans
        .iter()
        .filter(|s| {
            s.attributes
                .get("assay.claim_class")
                .and_then(|v| v.as_str())
                == Some("enforcement")
        })
        .count();
    assert_eq!(
        enforcement_spans, 0,
        "absent enforcement_health must emit no enforcement span"
    );
    assert!(p.spans.iter().all(|s| s
        .attributes
        .keys()
        .all(|k| !k.starts_with("assay.enforcement."))));
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
    assert!(p.spans.iter().all(|s| s
        .attributes
        .get("assay.claim_class")
        .and_then(|v| v.as_str())
        != Some("enforcement")));
}

#[test]
fn golden_fixture_roundtrip() {
    // A committed input + expected projection, so an external reader sees the contract concretely
    // (and a drift in the mapping is caught here, not in prose).
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/otel_projection"
    );
    let input: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/input.json")).unwrap())
            .unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/expected.json")).unwrap())
            .unwrap();
    let got = serde_json::to_value(project(
        &input["capability_surface"],
        input.get("observation_health"),
        input.get("enforcement_health"),
    ))
    .unwrap();
    assert_eq!(
        got, expected,
        "projection drifted from the committed golden fixture"
    );
}

#[test]
fn bless_golden_fixture() {
    if std::env::var("BLESS").is_err() {
        return;
    }
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/otel_projection"
    );
    let input: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/input.json")).unwrap())
            .unwrap();
    let got = serde_json::to_value(project(
        &input["capability_surface"],
        input.get("observation_health"),
        input.get("enforcement_health"),
    ))
    .unwrap();
    std::fs::write(
        format!("{dir}/expected.json"),
        serde_json::to_string_pretty(&got).unwrap(),
    )
    .unwrap();
}

/// #1408: the projection must map high-volume evidence to SPANS, never to span-events on a single
/// span. The OTel span-event count limit (default 128) silently drops events beyond the cap, and
/// (per the characterization in docs/reference/otel-span-event-limit.md) drops the OLDEST first. By
/// emitting one span per tool/decision and carrying detail in attributes, the projection never relies
/// on span-events, so that limit does not apply to it. This test guards that invariant: a future
/// change that introduced an `events` array on a span would regress it.
#[test]
fn projection_maps_volume_to_spans_not_span_events() {
    let tools: Vec<String> = (0..200).map(|i| format!("tool_{i}")).collect();
    let surface = json!({
        "schema": "assay.runner.capability_surface.v0",
        "mcp_tools": tools,
        "policy_decisions": []
    });
    let p = project(&surface, None, None);
    // High tool count becomes many spans, not many events on one span.
    let tool_spans = p
        .spans
        .iter()
        .filter(|s| s.name.starts_with("execute_tool "))
        .count();
    assert_eq!(
        tool_spans, 200,
        "each observed tool must project to its own span"
    );

    // No span carries an `events` array: the projection has no span-event surface to overflow.
    let value = serde_json::to_value(&p).unwrap();
    for span in value["spans"].as_array().unwrap() {
        assert!(
            span.get("events").is_none(),
            "projected spans must not carry span-events (would be subject to the OTel 128-event \
             drop limit); found events on span {span:?}"
        );
    }
}
