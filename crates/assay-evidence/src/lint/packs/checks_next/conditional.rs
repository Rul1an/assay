use super::super::{CheckContext, CheckResult};
use super::event::scoped_events;
use super::finding::{create_finding, event_location};
use super::json_path::value_pointer;
use crate::lint::packs::schema::{PackRule, SupportedConditionalCheck};
use crate::types::EvidenceEvent;

/// Check: if matching events satisfy a narrow condition, required path must
/// exist on the same event.
pub(in crate::lint::packs::checks) fn check_conditional(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    conditional: &SupportedConditionalCheck,
) -> CheckResult {
    let mut matched_events = 0usize;
    let mut missing_required_path = 0usize;
    let mut first_missing_event: Option<&EvidenceEvent> = None;

    for event in scoped_events(rule, ctx) {
        let json = match serde_json::to_value(event) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if !conditional
            .clauses
            .iter()
            .all(|clause| value_pointer(&json, &clause.path) == Some(&clause.equals))
        {
            continue;
        }

        matched_events += 1;

        if value_pointer(&json, &conditional.required_path).is_none() {
            missing_required_path += 1;
            if first_missing_event.is_none() {
                first_missing_event = Some(event);
            }
        }
    }

    if matched_events == 0 || missing_required_path == 0 {
        return CheckResult {
            passed: true,
            finding: None,
        };
    }

    let matched_label = if matched_events == 1 {
        "event"
    } else {
        "events"
    };
    let missing_label = if missing_required_path == 1 {
        "matching event was"
    } else {
        "matching events were"
    };

    CheckResult {
        passed: false,
        finding: Some(create_finding(
            rule,
            ctx,
            format!(
                "{} {} matched the condition, but {} {} missing required path: {}",
                matched_events,
                matched_label,
                missing_required_path,
                missing_label,
                conditional.required_path
            ),
            first_missing_event.map(event_location),
        )),
    }
}
