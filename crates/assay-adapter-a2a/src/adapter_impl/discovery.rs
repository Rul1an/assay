//! G4-A Phase 1 `payload.discovery` computation (adapter-emitted, Assay-namespaced).
//!
//! Frozen enum strings for `agent_card_source_kind` include `typed_payload` and `unmapped`, but
//! **v1 rules never produce them** — no typed upstream card path and no unmapped-key rules are
//! wired yet. Resolution still follows the §3 precedence order so future freezes can add sources
//! without reordering.
//!
//! **Emitted v1 values for `agent_card_source_kind`:** only `"attributes"` or `"unknown"` — do not
//! read `typed_payload` / `unmapped` as “supported today” until a later freeze wires matchers.

use serde_json::{Map, Value};

/// Resolve `agent_card_source_kind` from abstract precedence flags (highest wins).
///
/// v1 call sites pass `matched_typed_payload` and `matched_unmapped` as `false` until a freeze
/// adds real matchers.
#[must_use]
pub(super) fn resolve_agent_card_source_kind(
    matched_typed_payload: bool,
    matched_attributes: bool,
    matched_unmapped: bool,
) -> &'static str {
    if matched_typed_payload {
        "typed_payload"
    } else if matched_attributes {
        "attributes"
    } else if matched_unmapped {
        "unmapped"
    } else {
        "unknown"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiscoveryFields {
    pub agent_card_visible: bool,
    pub agent_card_source_kind: &'static str,
    pub extended_card_access_visible: bool,
}

impl Default for DiscoveryFields {
    fn default() -> Self {
        Self {
            agent_card_visible: false,
            agent_card_source_kind: "unknown",
            extended_card_access_visible: false,
        }
    }
}

fn agent_card_visible_from_attributes(g4: &Map<String, Value>) -> bool {
    let Some(ac) = g4.get("agent_card") else {
        return false;
    };
    let Value::Object(ac_obj) = ac else {
        return false;
    };
    matches!(ac_obj.get("visible"), Some(Value::Bool(true)))
}

fn extended_card_access_visible_from_attributes(g4: &Map<String, Value>) -> bool {
    let Some(ec) = g4.get("extended_card_access") else {
        return false;
    };
    let Value::Object(ec_obj) = ec else {
        return false;
    };
    matches!(ec_obj.get("visible"), Some(Value::Bool(true)))
}

/// Compute discovery fields from upstream `attributes` (raw JSON), before payload normalization.
///
/// Bounded meaning and paths are normative in `docs/architecture/G4-A-PHASE1-FREEZE.md` §2b and §4
/// (including: `extended_card_access_visible` is an **observed** flag only; `agent_card_source_kind`
/// in v1 is only `"attributes"` or `"unknown"` on the wire).
#[must_use]
pub(super) fn compute_discovery_fields(attributes: Option<&Value>) -> DiscoveryFields {
    let mut out = DiscoveryFields::default();

    let Some(Value::Object(attrs)) = attributes else {
        return out;
    };

    let Some(g4_val) = attrs.get("assay_g4") else {
        return out;
    };

    let Value::Object(g4) = g4_val else {
        return out;
    };

    out.agent_card_visible = agent_card_visible_from_attributes(g4);
    out.extended_card_access_visible = extended_card_access_visible_from_attributes(g4);

    let matched_attributes = out.agent_card_visible;
    let matched_typed = false;
    let matched_unmapped = false;
    out.agent_card_source_kind =
        resolve_agent_card_source_kind(matched_typed, matched_attributes, matched_unmapped);

    out
}

/// JSON object for `payload.discovery` (v1: `signature_material_visible` always `false`).
#[must_use]
pub(super) fn discovery_object(attributes: Option<&Value>) -> Value {
    let d = compute_discovery_fields(attributes);
    let mut m = Map::new();
    m.insert(
        "agent_card_visible".to_string(),
        Value::Bool(d.agent_card_visible),
    );
    m.insert(
        "agent_card_source_kind".to_string(),
        Value::String(d.agent_card_source_kind.to_string()),
    );
    m.insert(
        "extended_card_access_visible".to_string(),
        Value::Bool(d.extended_card_access_visible),
    );
    m.insert("signature_material_visible".to_string(), Value::Bool(false));
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn precedence_only_attributes_matches_attributes() {
        assert_eq!(
            resolve_agent_card_source_kind(false, true, false),
            "attributes"
        );
    }

    #[test]
    fn precedence_nothing_matches_unknown() {
        assert_eq!(
            resolve_agent_card_source_kind(false, false, false),
            "unknown"
        );
    }

    #[test]
    fn precedence_typed_wins_over_attributes() {
        assert_eq!(
            resolve_agent_card_source_kind(true, true, false),
            "typed_payload"
        );
    }

    #[test]
    fn precedence_unmapped_ranking_when_no_higher_match() {
        assert_eq!(
            resolve_agent_card_source_kind(false, false, true),
            "unmapped"
        );
    }

    #[test]
    fn v1_attributes_path_sets_agent_card_and_kind() {
        let attrs = json!({
            "assay_g4": { "agent_card": { "visible": true } },
            "priority": "high"
        });
        let d = compute_discovery_fields(Some(&attrs));
        assert!(d.agent_card_visible);
        assert_eq!(d.agent_card_source_kind, "attributes");
        assert!(!d.extended_card_access_visible);
    }

    #[test]
    fn extended_only_leaves_agent_card_unknown() {
        let attrs = json!({
            "assay_g4": { "extended_card_access": { "visible": true } }
        });
        let d = compute_discovery_fields(Some(&attrs));
        assert!(!d.agent_card_visible);
        assert_eq!(d.agent_card_source_kind, "unknown");
        assert!(d.extended_card_access_visible);
    }

    #[test]
    fn assay_g4_wrong_shape_no_promotion() {
        let attrs = json!({ "assay_g4": "not-an-object" });
        assert_eq!(
            compute_discovery_fields(Some(&attrs)),
            DiscoveryFields::default()
        );
    }

    #[test]
    fn agent_card_visible_false_does_not_set_kind_attributes() {
        let attrs = json!({
            "assay_g4": { "agent_card": { "visible": false } }
        });
        let d = compute_discovery_fields(Some(&attrs));
        assert!(!d.agent_card_visible);
        assert_eq!(d.agent_card_source_kind, "unknown");
    }
}
