//! Read-only projection of assay runtime evidence into OpenTelemetry GenAI + OpenInference
//! attributes.
//!
//! This is a one-directional, lossy projection: assay artifacts are the source of truth, and this
//! emits a standards-shaped *view* of them so an OTel/OpenInference backend can read assay evidence
//! without learning assay's vocabulary. It never reverses: nothing here is parsed back, and the
//! standard fields are a projection, never the authority.
//!
//! Three rules make the projection honest, which is the whole point of doing it this way:
//!
//! 1. Every standard field that a consumer could over-read carries a paired `assay.*` qualifier
//!    (`assay.claim_class`, the observation fields, the enforcement fields), so a backend that only
//!    knows `gen_ai.*` gets a familiar trace and a backend that reads `assay.*` keeps observed-vs-
//!    enforced and asserted-vs-verified intact.
//! 2. Enforcement is projected as its own attribute set and is **never** folded into a tool span
//!    (which is observed, not enforced). Enforcement truth comes from its own carrier
//!    (`assay.enforcement_health.v0`) and is absent when not supplied: absence makes no claim.
//! 3. Things the standard vocabulary cannot express (observed egress endpoints, filesystem paths)
//!    stay in `assay.*`. That is the lossy part, stated rather than hidden.
//!
//! Pinned to a specific GenAI semantic-conventions version. Both OTel GenAI and OpenInference are
//! still evolving, so the projection pins a version and a later bump is an explicit change, never a
//! silent reinterpretation.

use serde::Serialize;
use serde_json::{Map, Value};

/// Schema id of this projection artifact.
pub const PROJECTION_SCHEMA: &str = "assay.otel_projection.v0";

/// Pinned OTel GenAI semantic-conventions version this projection targets. Matches the version the
/// rest of the `otel` module pins (see `semconv::V1_28_0`). A bump is an explicit, separate change.
pub const SEMCONV_VERSION: &str = "1.28.0";

/// A single projected span (OTel span shape with OpenInference `span.kind` carried as an attribute,
/// since OpenInference is built on OTel and the two coexist).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectedSpan {
    pub name: String,
    /// OTel span kind. Tool execution and guardrail decisions are application-owned, so `INTERNAL`.
    pub kind: String,
    pub attributes: Map<String, Value>,
}

/// The full projection: run-level resource attributes, per-event spans, and the explicit non-claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Projection {
    pub schema: String,
    pub semconv_version: String,
    pub resource_attributes: Map<String, Value>,
    pub spans: Vec<ProjectedSpan>,
    /// Stated boundaries that travel with the projection so a consumer cannot over-read it.
    pub non_claims: Vec<String>,
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

fn str_array(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Project assay artifacts into the OTel GenAI + OpenInference attribute view.
///
/// `capability_surface` is an `assay.runner.capability_surface.v0` value. `observation_health` and
/// `enforcement_health` are optional and read from their own artifacts; an absent enforcement_health
/// means no enforcement claim is made (not "enforcement was absent" — that distinction lives in the
/// carrier itself).
pub fn project(
    capability_surface: &Value,
    observation_health: Option<&Value>,
    enforcement_health: Option<&Value>,
) -> Projection {
    let mut resource: Map<String, Value> = Map::new();
    resource.insert("service.name".into(), Value::String("assay".into()));
    resource.insert(
        "assay.otel_projection.schema".into(),
        Value::String(PROJECTION_SCHEMA.into()),
    );
    resource.insert(
        "assay.semconv.version".into(),
        Value::String(SEMCONV_VERSION.into()),
    );

    // ---- capability surface: tools and decisions become spans; raw sets stay in assay.* ----
    let mut spans: Vec<ProjectedSpan> = Vec::new();

    for tool in str_array(capability_surface, "mcp_tools") {
        let mut attrs = Map::new();
        attrs.insert(
            "gen_ai.operation.name".into(),
            Value::String("execute_tool".into()),
        );
        attrs.insert("gen_ai.tool.name".into(), Value::String(tool.clone()));
        // OpenInference span kind, carried alongside gen_ai.* (the two coexist).
        attrs.insert(
            "openinference.span.kind".into(),
            Value::String("TOOL".into()),
        );
        attrs.insert("tool.name".into(), Value::String(tool.clone()));
        // Honesty qualifier: a capability-surface tool is OBSERVED, not proven-enforced.
        attrs.insert("assay.claim_class".into(), Value::String("observed".into()));
        spans.push(ProjectedSpan {
            name: format!("execute_tool {tool}"),
            kind: "INTERNAL".into(),
            attributes: attrs,
        });
    }

    // `policy_decisions` entries follow `<decision>:<key>` (e.g. "allow:read_file", "deny:write_file").
    for decision in str_array(capability_surface, "policy_decisions") {
        let (verdict, key) = match decision.split_once(':') {
            Some((v, k)) => (v.to_string(), k.to_string()),
            None => (decision.clone(), String::new()),
        };
        let mut attrs = Map::new();
        // A policy decision maps to an OpenInference GUARDRAIL span (the gating step).
        attrs.insert(
            "openinference.span.kind".into(),
            Value::String("GUARDRAIL".into()),
        );
        attrs.insert("assay.decision".into(), Value::String(verdict));
        if !key.is_empty() {
            attrs.insert("assay.tool".into(), Value::String(key.clone()));
        }
        attrs.insert("assay.claim_class".into(), Value::String("observed".into()));
        spans.push(ProjectedSpan {
            name: format!("guardrail {key}"),
            kind: "INTERNAL".into(),
            attributes: attrs,
        });
    }

    // Observed sets the standard vocabulary has no field for stay in assay.* (the lossy part).
    for (src_key, attr_key) in [
        ("network_endpoints", "assay.capability.network_endpoints"),
        ("filesystem_paths", "assay.capability.filesystem_paths"),
        ("process_execs", "assay.capability.process_execs"),
    ] {
        let set = str_array(capability_surface, src_key);
        if !set.is_empty() {
            resource.insert(
                attr_key.into(),
                Value::Array(set.into_iter().map(Value::String).collect()),
            );
        }
    }

    // ---- observation_health: how complete the observation was (not whether it was enforced) ----
    if let Some(oh) = observation_health {
        for (src_key, attr_key) in [
            ("kernel_layer", "assay.observation.kernel_layer"),
            (
                "network_protocol_coverage",
                "assay.observation.network_protocol_coverage",
            ),
            ("policy_layer", "assay.observation.policy_layer"),
        ] {
            if let Some(v) = str_field(oh, src_key) {
                resource.insert(attr_key.into(), Value::String(v));
            }
        }
    }

    // ---- enforcement_health: its OWN attribute set, never folded into a tool span ----
    // Absent enforcement_health => no enforcement attributes at all => no enforcement claim.
    if let Some(eh) = enforcement_health {
        if let Some(v) = str_field(eh, "network_enforcement") {
            resource.insert("assay.enforcement.network".into(), Value::String(v));
        }
        if let Some(v) = eh.get("attach_confirmed").and_then(|x| x.as_bool()) {
            resource.insert("assay.enforcement.attach_confirmed".into(), Value::Bool(v));
        }
        if let Some(v) = str_field(eh, "scope") {
            resource.insert("assay.enforcement.scope".into(), Value::String(v));
        }
        for (src_key, attr_key) in [
            ("blocked_count", "assay.enforcement.blocked_count"),
            ("allowed_count", "assay.enforcement.allowed_count"),
        ] {
            if let Some(n) = eh.get(src_key).and_then(|x| x.as_u64()) {
                resource.insert(attr_key.into(), Value::Number(n.into()));
            }
        }
    }

    let non_claims = vec![
        "This projection is a one-directional, lossy view; the assay artifacts remain authoritative."
            .to_string(),
        "gen_ai.* and openinference.* fields are a projection, not the source of truth.".to_string(),
        "Observed is not enforced: capability-surface tools carry assay.claim_class=observed, and \
         enforcement is a separate attribute set read only from assay.enforcement_health.v0."
            .to_string(),
        "Absence of enforcement attributes means no enforcement_health was supplied, not that \
         enforcement was absent."
            .to_string(),
        format!("Pinned to GenAI semantic conventions {SEMCONV_VERSION}; a version bump is explicit."),
    ];

    Projection {
        schema: PROJECTION_SCHEMA.to_string(),
        semconv_version: SEMCONV_VERSION.to_string(),
        resource_attributes: resource,
        spans,
        non_claims,
    }
}
