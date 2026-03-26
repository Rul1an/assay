use super::super::{CheckContext, CheckResult};
use super::event::scoped_events;
use super::finding::create_finding;
use crate::lint::packs::schema::PackRule;

/// Check: JSON path exists in events (optional value equality at each path).
///
/// When `value_equals` is set, matching uses `serde_json::Value` equality only — no string/bool
/// coercion, normalization, or schema widening (wrong JSON type ⇒ no match).
pub(in crate::lint::packs::checks) fn check_json_path_exists(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    paths: &[String],
    value_equals: Option<&serde_json::Value>,
) -> CheckResult {
    for event in scoped_events(rule, ctx) {
        let json = match serde_json::to_value(event) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for path in paths {
            match (value_pointer(&json, path), value_equals) {
                (Some(v), Some(required)) if v == required => {
                    return CheckResult {
                        passed: true,
                        finding: None,
                    };
                }
                (Some(_), Some(_)) => continue,
                (Some(_), None) => {
                    return CheckResult {
                        passed: true,
                        finding: None,
                    };
                }
                (None, _) => continue,
            }
        }
    }

    let detail = match value_equals {
        Some(req) => format!("No event has path {} equal to {}", paths.join(", "), req),
        None => format!("No event contains paths: {}", paths.join(", ")),
    };
    CheckResult {
        passed: false,
        finding: Some(create_finding(rule, ctx, detail, None)),
    }
}

pub(in crate::lint::packs::checks) fn value_pointer<'a>(
    value: &'a serde_json::Value,
    pointer: &str,
) -> Option<&'a serde_json::Value> {
    if pointer.is_empty() || pointer == "/" {
        return Some(value);
    }
    let normalized;
    let pointer = if pointer.starts_with('/') {
        pointer
    } else {
        normalized = format!("/{}", pointer);
        normalized.as_str()
    };
    value.pointer(pointer)
}
