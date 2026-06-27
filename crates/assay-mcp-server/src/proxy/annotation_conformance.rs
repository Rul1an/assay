//! Pure producer contract for `assay.tool_annotation_conformance.v0` (Increment 5a).
//!
//! This carrier compares untrusted MCP ToolAnnotations against Assay's own rule-based call
//! classification. It is deliberately orthogonal to the enforcement verdict: a mismatch is a
//! conformance signal, never a deny, and a consistent record is not trust certification.

use assay_mcp_server::tool_decision::{classify, sanitize};
use serde_json::{json, Value};

pub const TOOL_ANNOTATION_CONFORMANCE_SCHEMA: &str = "assay.tool_annotation_conformance.v0";

#[derive(Debug, Clone, Copy, Default)]
pub struct DeclaredToolAnnotations {
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub idempotent: Option<bool>,
    pub open_world: Option<bool>,
}

/// Extract the v0 declared annotation hints from a raw MCP `annotations` value. Absent, null,
/// non-object, or non-boolean hints all read as `None`: the carrier records what the server actually
/// declared, and an absent hint is undeclared, never the MCP schema default.
pub fn extract_declared_annotations(annotations: &Value) -> DeclaredToolAnnotations {
    let hint = |key: &str| annotations.get(key).and_then(Value::as_bool);
    DeclaredToolAnnotations {
        read_only: hint("readOnlyHint"),
        destructive: hint("destructiveHint"),
        idempotent: hint("idempotentHint"),
        open_world: hint("openWorldHint"),
    }
}

/// Whether the called tool's declared annotations were read from a completely observed manifest.
/// `Incomplete` (no complete manifest, an ambiguous one, or the tool absent) forces `inconclusive`:
/// the annotations were not observed, which is distinct from a server that declared none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationBasis {
    Complete,
    Incomplete,
}

impl ObservationBasis {
    fn as_str(self) -> &'static str {
        match self {
            ObservationBasis::Complete => "complete",
            ObservationBasis::Incomplete => "incomplete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservedBehavior {
    Mutating,
    Destructive,
}

impl ObservedBehavior {
    fn as_str(self) -> &'static str {
        match self {
            ObservedBehavior::Mutating => "mutating",
            ObservedBehavior::Destructive => "destructive",
        }
    }
}

fn observed_behavior(verb: Option<&str>) -> Option<ObservedBehavior> {
    match verb {
        // Destructive means the call may overwrite/remove/reconfigure existing state. Additive admin
        // actions are still mutating, but they do not contradict `destructiveHint:false`.
        Some("change_role" | "modify") => Some(ObservedBehavior::Destructive),
        Some("create" | "add" | "grant" | "invite") => Some(ObservedBehavior::Mutating),
        _ => None,
    }
}

fn push_axis(axes: &mut Vec<&'static str>, axis: &'static str) {
    if !axes.contains(&axis) {
        axes.push(axis);
    }
}

/// Build one record with a `Complete` observation basis and no recorded digest. Test-only
/// convenience that exercises the conformance logic against a known declaration.
#[cfg(test)]
pub fn conformance_for(declared: &DeclaredToolAnnotations, tool_name: &str, args: &Value) -> Value {
    build_tool_annotation_conformance_record(
        ObservationBasis::Complete,
        declared,
        tool_name,
        None,
        args,
    )
}

/// Build one `assay.tool_annotation_conformance.v0` record from the declared annotations and the
/// rule-based classification of the call. When `basis` is `Incomplete` the declared annotations were
/// not observed, so the record forces `inconclusive` with null declared hints and digest (the
/// classifier's observed behavior is still recorded) rather than reading absence as "undeclared".
pub fn build_tool_annotation_conformance_record(
    basis: ObservationBasis,
    declared: &DeclaredToolAnnotations,
    tool_name: &str,
    tool_digest: Option<&str>,
    args: &Value,
) -> Value {
    let classified = classify(tool_name, args);
    let behavior = if classified.state == "classified" {
        observed_behavior(classified.verb)
    } else {
        None
    };

    let complete = basis == ObservationBasis::Complete;
    let declared = if complete {
        *declared
    } else {
        DeclaredToolAnnotations::default()
    };
    let tool_digest = if complete { tool_digest } else { None };

    let mut assessed_axes: Vec<&'static str> = Vec::new();
    let mut mismatch_kind: Option<&'static str> = None;

    if complete {
        if let Some(read_only) = declared.read_only {
            if behavior.is_some() {
                push_axis(&mut assessed_axes, "read_only");
                if read_only {
                    mismatch_kind = Some("declared_read_only_observed_mutating");
                }
            }
        }

        if declared.read_only != Some(true) {
            if let Some(destructive) = declared.destructive {
                if let Some(observed) = behavior {
                    push_axis(&mut assessed_axes, "destructive");
                    if !destructive && observed == ObservedBehavior::Destructive {
                        mismatch_kind = Some("declared_non_destructive_observed_destructive");
                    }
                }
            }
        }
    }

    let conformance = if !complete || behavior.is_none() {
        "inconclusive"
    } else if assessed_axes.is_empty() {
        "undeclared"
    } else if mismatch_kind.is_some() {
        "mismatched"
    } else {
        "consistent"
    };

    let mut unassessed_axes: Vec<&'static str> = Vec::new();
    if declared.idempotent.is_some() {
        unassessed_axes.push("idempotent");
    }
    if declared.open_world.is_some() {
        unassessed_axes.push("open_world");
    }

    json!({
        "schema": TOOL_ANNOTATION_CONFORMANCE_SCHEMA,
        "observation_basis": basis.as_str(),
        "tool": {
            "name": sanitize(tool_name),
            "tool_digest": tool_digest,
            "action_class": classified.category,
        },
        "declared": {
            "read_only": declared.read_only,
            "destructive": declared.destructive,
            "idempotent": declared.idempotent,
            "open_world": declared.open_world,
        },
        "observed": {
            "classification_state": classified.state,
            "verb": classified.verb,
            "behavior_class": behavior.map(ObservedBehavior::as_str),
        },
        "conformance": conformance,
        "mismatch_kind": mismatch_kind,
        "assessed_axes": assessed_axes,
        "unassessed_axes": unassessed_axes,
        "non_claims": [
            "tool annotations are untrusted hints, not security guarantees",
            "consistent does not certify the server or the annotation as trustworthy",
            "mismatch is a conformance signal, not a maliciousness verdict or an enforcement decision",
            "observed behavior is Assay's call classification, not verification of the upstream side effect",
            "observation_basis incomplete means annotations were not observed, not that the server declared none",
            "idempotentHint and openWorldHint are recorded but not assessed in v0"
        ]
    })
}

#[cfg(test)]
mod tests;
