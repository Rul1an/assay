//! K1-A Phase 1 `payload.handoff` computation (adapter-emitted, Assay-namespaced).
//!
//! v1 is intentionally narrow: the only positive path is a canonical
//! `assay.adapter.a2a.task.requested` event with typed `task.kind == "delegation"`.
//! All other packets emit the default handoff object.

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HandoffFields {
    pub visible: bool,
    pub source_kind: &'static str,
    pub task_ref_visible: bool,
    pub message_ref_visible: bool,
}

impl Default for HandoffFields {
    fn default() -> Self {
        Self {
            visible: false,
            source_kind: "unknown",
            task_ref_visible: false,
            message_ref_visible: false,
        }
    }
}

#[must_use]
pub(super) fn compute_handoff_fields(
    canonical_event_type: &str,
    task_kind: Option<&str>,
    typed_task_id_present: bool,
    typed_message_id_present: bool,
) -> HandoffFields {
    let mut out = HandoffFields::default();

    if canonical_event_type == "assay.adapter.a2a.task.requested"
        && matches!(task_kind, Some("delegation"))
    {
        out.visible = true;
        out.source_kind = "typed_payload";
        out.task_ref_visible = typed_task_id_present;
        out.message_ref_visible = typed_message_id_present;
    }

    out
}

#[must_use]
pub(super) fn handoff_object(
    canonical_event_type: &str,
    task_kind: Option<&str>,
    typed_task_id_present: bool,
    typed_message_id_present: bool,
) -> Value {
    let h = compute_handoff_fields(
        canonical_event_type,
        task_kind,
        typed_task_id_present,
        typed_message_id_present,
    );
    let mut m = Map::new();
    m.insert("visible".to_string(), Value::Bool(h.visible));
    m.insert(
        "source_kind".to_string(),
        Value::String(h.source_kind.to_string()),
    );
    m.insert(
        "task_ref_visible".to_string(),
        Value::Bool(h.task_ref_visible),
    );
    m.insert(
        "message_ref_visible".to_string(),
        Value::Bool(h.message_ref_visible),
    );
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_requested_delegation_sets_typed_payload_visibility() {
        assert_eq!(
            compute_handoff_fields(
                "assay.adapter.a2a.task.requested",
                Some("delegation"),
                true,
                true,
            ),
            HandoffFields {
                visible: true,
                source_kind: "typed_payload",
                task_ref_visible: true,
                message_ref_visible: true,
            }
        );
    }

    #[test]
    fn lenient_missing_task_id_still_keeps_route_visible() {
        assert_eq!(
            compute_handoff_fields(
                "assay.adapter.a2a.task.requested",
                Some("delegation"),
                false,
                true,
            ),
            HandoffFields {
                visible: true,
                source_kind: "typed_payload",
                task_ref_visible: false,
                message_ref_visible: true,
            }
        );
    }

    #[test]
    fn task_updated_is_not_positive_in_v1() {
        assert_eq!(
            compute_handoff_fields(
                "assay.adapter.a2a.task.updated",
                Some("delegation"),
                true,
                true,
            ),
            HandoffFields::default()
        );
    }

    #[test]
    fn non_delegation_task_requested_stays_default() {
        assert_eq!(
            compute_handoff_fields(
                "assay.adapter.a2a.task.requested",
                Some("analysis"),
                true,
                true,
            ),
            HandoffFields::default()
        );
    }
}
