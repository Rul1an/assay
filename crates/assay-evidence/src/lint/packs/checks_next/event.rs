use super::super::{CheckContext, CheckResult};
use super::finding::create_finding;
use super::json_path::value_pointer;
use crate::lint::packs::schema::PackRule;
use globset::{Glob, GlobMatcher};

/// G3 v1: same predicate as Trust Basis `authorization_context_visible` (verified).
///
/// Honors `rule.event_types` like other checks: only scoped events are considered.
pub(in crate::lint::packs::checks) fn check_g3_authorization_context_present(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
) -> CheckResult {
    let passed = scoped_events(rule, ctx).into_iter().any(|event| {
        crate::g3_authorization_context::decision_event_satisfies_g3_authorization_context_visible(
            event,
        )
    });
    if passed {
        CheckResult {
            passed: true,
            finding: None,
        }
    } else {
        CheckResult {
            passed: false,
            finding: Some(create_finding(
                rule,
                ctx,
                "No assay.tool.decision event satisfies G3 v1 policy-projected authorization context (principal + allowlisted auth_scheme + auth_issuer with G3 string discipline).".to_string(),
                None,
            )),
        }
    }
}

/// Check: bundle contains minimum number of events.
pub(in crate::lint::packs::checks) fn check_event_count(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    min: usize,
) -> CheckResult {
    let count = scoped_events(rule, ctx).len();
    if count >= min {
        CheckResult {
            passed: true,
            finding: None,
        }
    } else {
        CheckResult {
            passed: false,
            finding: Some(create_finding(
                rule,
                ctx,
                scoped_event_count_message(rule, count, min),
                None,
            )),
        }
    }
}

/// Check: matching start/finish event pairs exist.
pub(in crate::lint::packs::checks) fn check_event_pairs(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    start_pattern: &str,
    finish_pattern: &str,
) -> CheckResult {
    let start_matcher = match compile_glob(start_pattern) {
        Some(m) => m,
        None => {
            return CheckResult {
                passed: false,
                finding: Some(create_finding(
                    rule,
                    ctx,
                    format!("Invalid start pattern: {}", start_pattern),
                    None,
                )),
            };
        }
    };

    let finish_matcher = match compile_glob(finish_pattern) {
        Some(m) => m,
        None => {
            return CheckResult {
                passed: false,
                finding: Some(create_finding(
                    rule,
                    ctx,
                    format!("Invalid finish pattern: {}", finish_pattern),
                    None,
                )),
            };
        }
    };

    let scoped_events = scoped_events(rule, ctx);
    let has_start = scoped_events
        .iter()
        .any(|e| start_matcher.is_match(&e.type_));
    let has_finish = scoped_events
        .iter()
        .any(|e| finish_matcher.is_match(&e.type_));

    if has_start && has_finish {
        CheckResult {
            passed: true,
            finding: None,
        }
    } else {
        let missing = match (has_start, has_finish) {
            (false, false) => format!(
                "Missing both start ({}) and finish ({}) events",
                start_pattern, finish_pattern
            ),
            (false, true) => format!("Missing start event matching '{}'", start_pattern),
            (true, false) => format!("Missing finish event matching '{}'", finish_pattern),
            _ => unreachable!(),
        };
        CheckResult {
            passed: false,
            finding: Some(create_finding(rule, ctx, missing, None)),
        }
    }
}

/// Check: at least one event contains specified fields (JSON Pointer paths).
pub(in crate::lint::packs::checks) fn check_event_field_present(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    paths: &[String],
) -> CheckResult {
    for event in scoped_events(rule, ctx) {
        let json = match serde_json::to_value(event) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for path in paths {
            if value_pointer(&json, path).is_some() {
                return CheckResult {
                    passed: true,
                    finding: None,
                };
            }
        }
    }

    CheckResult {
        passed: false,
        finding: Some(create_finding(
            rule,
            ctx,
            format!(
                "No event contains any of the required fields: {}",
                paths.join(", ")
            ),
            None,
        )),
    }
}

/// Check: at least one event of specified type exists.
pub(in crate::lint::packs::checks) fn check_event_type_exists(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    pattern: &str,
) -> CheckResult {
    let matcher = match compile_glob(pattern) {
        Some(m) => m,
        None => {
            return CheckResult {
                passed: false,
                finding: Some(create_finding(
                    rule,
                    ctx,
                    format!("Invalid pattern: {}", pattern),
                    None,
                )),
            };
        }
    };

    if scoped_events(rule, ctx)
        .into_iter()
        .any(|e| matcher.is_match(&e.type_))
    {
        CheckResult {
            passed: true,
            finding: None,
        }
    } else {
        CheckResult {
            passed: false,
            finding: Some(create_finding(
                rule,
                ctx,
                format!("No event found matching type pattern '{}'", pattern),
                None,
            )),
        }
    }
}

/// Compile a glob pattern.
pub(in crate::lint::packs::checks) fn compile_glob(pattern: &str) -> Option<GlobMatcher> {
    Glob::new(pattern).ok().map(|g| g.compile_matcher())
}

pub(super) fn scoped_events<'a>(
    rule: &PackRule,
    ctx: &'a CheckContext<'a>,
) -> Vec<&'a crate::types::EvidenceEvent> {
    match &rule.event_types {
        Some(event_types) if !event_types.is_empty() => ctx
            .events
            .iter()
            .filter(|event| event_types.iter().any(|expected| expected == &event.type_))
            .collect(),
        _ => ctx.events.iter().collect(),
    }
}

fn scoped_event_count_message(rule: &PackRule, count: usize, min: usize) -> String {
    match &rule.event_types {
        Some(event_types) if !event_types.is_empty() => format!(
            "Scoped events for event_types [{}] contain {} events (minimum: {})",
            event_types.join(", "),
            count,
            min
        ),
        _ => format!("Bundle contains {} events (minimum: {})", count, min),
    }
}
