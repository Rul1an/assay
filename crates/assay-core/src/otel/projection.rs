//! Read-only projection of assay runtime evidence into OpenTelemetry GenAI + OpenInference
//! attributes.
//!
//! This is a one-directional, lossy projection: assay artifacts are the source of truth, and this
//! emits a standards-shaped *view* of them so an OTel/OpenInference backend can read assay evidence
//! without learning assay's vocabulary. Nothing here is parsed back; the standard fields are a
//! projection, never the authority. The output carries `lossy: true` and `source_of_truth` so a
//! consumer cannot mistake the view for the record.
//!
//! Rules that make the projection honest, which is the whole point of doing it this way:
//!
//! 1. Every standard field a consumer could over-read carries a paired `assay.*` qualifier
//!    (`assay.claim_class`, the observation fields), so a backend that only knows `gen_ai.*` gets a
//!    familiar trace and a backend that reads `assay.*` keeps observed-vs-enforced intact.
//! 2. Enforcement is its OWN span (a guardrail-style enforcement span), never attributes hung next to
//!    an (observed) tool span. Otherwise a downstream tool reads "tool ran" and misses that the
//!    load-bearing claim was "enforcement was active / blocked / failed". Enforcement is absent when
//!    no `enforcement_health` is supplied: absence makes no claim.
//! 3. Things the standard vocabulary cannot express (observed egress endpoints, paths) stay in
//!    `assay.*`. That is the lossy part, stated rather than hidden.
//!
//! Both OTel GenAI and OpenInference are still evolving upstream (GenAI semconv is Development), so the
//! projection pins versions and a bump is an explicit change, never a silent reinterpretation.

use serde::Serialize;
use serde_json::{Map, Value};

/// Schema id of this projection artifact.
pub const PROJECTION_SCHEMA: &str = "assay.otel_projection.v0";

/// Pinned OTel GenAI semconv target, flagged Development to match upstream status. Matches the
/// version the rest of the `otel` module pins (`semconv::V1_28_0`). A bump is an explicit change.
pub const OTEL_GENAI_SEMCONV: &str = "1.28.0-development";

/// OpenInference is pinned (its span-kind set is stable enough to target by name).
pub const OPENINFERENCE_SEMCONV: &str = "pinned";

/// The authoritative source the projection is a lossy view of.
pub const SOURCE_OF_TRUTH: &str = "assay artifacts";

/// Pinned semantic-convention targets for the projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Semconv {
    pub otel_genai: String,
    pub openinference: String,
}

/// A single projected span (OTel span shape with OpenInference `span.kind` carried as an attribute,
/// since OpenInference is built on OTel and the two coexist).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectedSpan {
    pub name: String,
    /// OTel span kind. Tool, guardrail, and enforcement spans are application-owned, so `INTERNAL`.
    pub kind: String,
    pub attributes: Map<String, Value>,
}

/// The full projection. `lossy` and `source_of_truth` are not decoration: they are the contract that
/// the standard fields are a view and the assay artifacts are the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Projection {
    pub schema: String,
    pub semconv: Semconv,
    pub spans: Vec<ProjectedSpan>,
    pub resource_attributes: Map<String, Value>,
    pub lossy: bool,
    pub source_of_truth: String,
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

    let mut spans: Vec<ProjectedSpan> = Vec::new();

    // ---- capability surface: tools become TOOL spans, decisions become GUARDRAIL spans ----
    for tool in str_array(capability_surface, "mcp_tools") {
        let mut attrs = Map::new();
        attrs.insert(
            "gen_ai.operation.name".into(),
            Value::String("execute_tool".into()),
        );
        attrs.insert("gen_ai.tool.name".into(), Value::String(tool.clone()));
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

    // ---- enforcement_health: its OWN guardrail-style span, never folded onto a tool span ----
    // Absent enforcement_health => no enforcement span => no enforcement claim.
    if let Some(eh) = enforcement_health {
        let scope = str_field(eh, "scope").unwrap_or_else(|| "unknown".into());
        let mut attrs = Map::new();
        attrs.insert(
            "openinference.span.kind".into(),
            Value::String("GUARDRAIL".into()),
        );
        // claim_class marks this as enforcement-truth, distinct from an observed tool.
        attrs.insert(
            "assay.claim_class".into(),
            Value::String("enforcement".into()),
        );
        if let Some(v) = str_field(eh, "network_enforcement") {
            attrs.insert("assay.enforcement.network".into(), Value::String(v));
        }
        if let Some(v) = eh.get("attach_confirmed").and_then(|x| x.as_bool()) {
            attrs.insert("assay.enforcement.attach_confirmed".into(), Value::Bool(v));
        }
        attrs.insert(
            "assay.enforcement.scope".into(),
            Value::String(scope.clone()),
        );
        for (src_key, attr_key) in [
            ("blocked_count", "assay.enforcement.blocked_count"),
            ("allowed_count", "assay.enforcement.allowed_count"),
        ] {
            if let Some(n) = eh.get(src_key).and_then(|x| x.as_u64()) {
                attrs.insert(attr_key.into(), Value::Number(n.into()));
            }
        }
        spans.push(ProjectedSpan {
            name: format!("enforcement {scope}"),
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

    // ---- observation_health: how complete the observation was (run-level context, not a span) ----
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

    let non_claims = vec![
        "This projection is a one-directional, lossy view; the assay artifacts remain authoritative."
            .to_string(),
        "gen_ai.* and openinference.* fields are a projection, not the source of truth.".to_string(),
        "Observed is not enforced: capability-surface tools carry assay.claim_class=observed, and \
         enforcement is a separate span read only from assay.enforcement_health.v0."
            .to_string(),
        "Absence of an enforcement span means no enforcement_health was supplied, not that \
         enforcement was absent."
            .to_string(),
        format!(
            "Pinned to OTel GenAI semconv {OTEL_GENAI_SEMCONV} and OpenInference {OPENINFERENCE_SEMCONV}; \
             a version bump is explicit."
        ),
    ];

    Projection {
        schema: PROJECTION_SCHEMA.to_string(),
        semconv: Semconv {
            otel_genai: OTEL_GENAI_SEMCONV.to_string(),
            openinference: OPENINFERENCE_SEMCONV.to_string(),
        },
        spans,
        resource_attributes: resource,
        lossy: true,
        source_of_truth: SOURCE_OF_TRUTH.to_string(),
        non_claims,
    }
}
