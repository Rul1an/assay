//! Shared denial-marker classifier contract for privileged-mcp-action v0/v1.
//!
//! Exact triples only. Cross-pairs, wrong origin, string/missing code, and
//! code-only or origin-only shapes stay inert (`None`).

use assay_evidence::denial_marker::{
    classify_denial_marker, DenialMarkerVersion, DENIED_CALL_OBSERVATION_V0,
    DENIED_CALL_OBSERVATION_V1, PROXY_DENIED_V0, PROXY_ORIGIN,
};
use serde_json::{json, Value};

/// Application-range v1 deny code (outside JSON-RPC `-32768..=-32000`).
const V1_DENIED: i64 = -31999;

fn payload(schema: &str, code: Value, origin: Value) -> Value {
    json!({
        "schema": schema,
        "caller_visible_error": {
            "code": code,
            "origin": origin,
            "reason": "no_declared_allowance"
        }
    })
}

fn classify(schema: &str, code: Value, origin: Value) -> Option<DenialMarkerVersion> {
    classify_denial_marker(&payload(schema, code, origin))
}

#[test]
fn exact_v0_triple_classifies() {
    assert_eq!(
        classify(
            DENIED_CALL_OBSERVATION_V0,
            json!(PROXY_DENIED_V0),
            json!(PROXY_ORIGIN)
        ),
        Some(DenialMarkerVersion::V0)
    );
}

#[test]
fn exact_v1_triple_classifies() {
    assert_eq!(
        classify(
            DENIED_CALL_OBSERVATION_V1,
            json!(V1_DENIED),
            json!(PROXY_ORIGIN)
        ),
        Some(DenialMarkerVersion::V1)
    );
}

#[test]
fn abandoned_legacy_band_code_on_v1_schema_is_inert() {
    assert_eq!(
        classify(
            DENIED_CALL_OBSERVATION_V1,
            json!(-32019),
            json!(PROXY_ORIGIN)
        ),
        None
    );
}

#[test]
fn cross_pair_v0_schema_v1_code_is_inert() {
    assert_eq!(
        classify(
            DENIED_CALL_OBSERVATION_V0,
            json!(V1_DENIED),
            json!(PROXY_ORIGIN)
        ),
        None
    );
}

#[test]
fn cross_pair_v1_schema_v0_code_is_inert() {
    assert_eq!(
        classify(
            DENIED_CALL_OBSERVATION_V1,
            json!(PROXY_DENIED_V0),
            json!(PROXY_ORIGIN)
        ),
        None
    );
}

#[test]
fn wrong_origin_is_inert() {
    assert_eq!(
        classify(
            DENIED_CALL_OBSERVATION_V0,
            json!(PROXY_DENIED_V0),
            json!("upstream")
        ),
        None
    );
    assert_eq!(
        classify(
            DENIED_CALL_OBSERVATION_V1,
            json!(V1_DENIED),
            json!("upstream")
        ),
        None
    );
}

#[test]
fn string_code_is_inert() {
    assert_eq!(
        classify(
            DENIED_CALL_OBSERVATION_V0,
            json!("-32042"),
            json!(PROXY_ORIGIN)
        ),
        None
    );
    assert_eq!(
        classify(
            DENIED_CALL_OBSERVATION_V1,
            json!("-31999"),
            json!(PROXY_ORIGIN)
        ),
        None
    );
}

#[test]
fn missing_code_is_inert() {
    let mut v0 = payload(
        DENIED_CALL_OBSERVATION_V0,
        json!(PROXY_DENIED_V0),
        json!(PROXY_ORIGIN),
    );
    v0["caller_visible_error"]
        .as_object_mut()
        .expect("object")
        .remove("code");
    assert_eq!(classify_denial_marker(&v0), None);
}

#[test]
fn code_only_without_origin_is_inert() {
    let mut v0 = payload(
        DENIED_CALL_OBSERVATION_V0,
        json!(PROXY_DENIED_V0),
        json!(PROXY_ORIGIN),
    );
    v0["caller_visible_error"]
        .as_object_mut()
        .expect("object")
        .remove("origin");
    assert_eq!(classify_denial_marker(&v0), None);
}

#[test]
fn origin_only_without_matching_schema_is_inert() {
    assert_eq!(
        classify(
            "assay.enforcement_decision.v0",
            json!(PROXY_DENIED_V0),
            json!(PROXY_ORIGIN)
        ),
        None
    );
}

#[test]
fn unknown_schema_with_either_code_is_inert() {
    assert_eq!(
        classify(
            "assay.denied_call_observation.v2",
            json!(V1_DENIED),
            json!(PROXY_ORIGIN)
        ),
        None
    );
}
