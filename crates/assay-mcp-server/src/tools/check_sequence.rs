use super::{ToolContext, ToolError};
use anyhow::{Context, Result};
use assay_core::model::SequenceRule;
use assay_core::sequence_eval::{evaluate_rules, RuleEvaluation, SequenceCall, TraceExtent};
use serde_json::{json, Value};

pub async fn check_sequence(ctx: &ToolContext, args: &Value) -> Result<Value> {
    // 1. Unpack args & Check Limits
    let history_val = args.get("history").context("Missing 'history' argument")?;
    let history: Vec<String> =
        serde_json::from_value(history_val.clone()).context("Invalid 'history' format")?;

    let next_tool = args
        .get("next_tool")
        .and_then(|v| v.as_str())
        .context("Missing 'next_tool' argument")?;
    let policy_rel_path = args
        .get("policy")
        .and_then(|v| v.as_str())
        .context("Missing 'policy' argument")?;

    if history.len() > ctx.cfg.max_tool_calls {
        return ToolError::new("E_LIMIT_EXCEEDED", "history too long").result();
    }
    if next_tool.len() > ctx.cfg.max_field_bytes {
        return ToolError::new("E_LIMIT_EXCEEDED", "next_tool too long").result();
    }
    if policy_rel_path.len() > ctx.cfg.max_field_bytes {
        return ToolError::new("E_LIMIT_EXCEEDED", "policy path too long").result();
    }

    // 2. Load Policy
    let policy_path = match ctx.resolve_policy_path(policy_rel_path).await {
        Ok(p) => p,
        Err(e) => return e.result(),
    };

    let policy_bytes = match ctx.read_policy_bounded(policy_rel_path).await {
        Ok(b) => b,
        Err(e) => return e.result(),
    };

    let sha = crate::cache::sha256_hex(&policy_bytes);
    let cache_key = crate::cache::key(policy_path.to_str().unwrap_or(""), &sha);

    let policy = if let Some(p) = ctx.caches.sequence.get(&cache_key) {
        tracing::debug!(event="cache_hit", key=%cache_key, cache="sequence");
        p
    } else {
        tracing::debug!(event="cache_miss", key=%cache_key, cache="sequence");
        // Compile (Parse)
        // Try parsing as v1.1 Policy first
        let policy_item =
            if let Ok(pol) = serde_yaml::from_slice::<assay_core::model::Policy>(&policy_bytes) {
                crate::cache::SequencePolicy::V1_1(Box::new(pol))
            } else if let Ok(rules) =
                serde_yaml::from_slice::<Vec<assay_core::model::SequenceRule>>(&policy_bytes)
            {
                crate::cache::SequencePolicy::Rules(rules)
            } else if let Ok(seq) = serde_yaml::from_slice::<Vec<String>>(&policy_bytes) {
                crate::cache::SequencePolicy::Legacy(seq)
            } else {
                return ToolError::new("E_POLICY_PARSE", "Invalid sequence policy format").result();
            };

        let arc = std::sync::Arc::new(policy_item);
        ctx.caches.sequence.insert(cache_key, arc.clone());
        arc
    };

    // 3. Synthesize Trace for Validation
    // history + next_tool
    let mut actual_names = history.clone();
    actual_names.push(next_tool.to_string());

    // 4. Validate
    match &*policy {
        crate::cache::SequencePolicy::Legacy(expected_seq) => {
            if actual_names == *expected_seq {
                Ok(serde_json::json!({ "allowed": true, "violations": [], "suggested_fix": null }))
            } else {
                Ok(serde_json::json!({
                    "allowed": false,
                    "violations": [{
                        "constraint": "sequence_exact_match",
                        "suggestion": format!("Expected {:?}, found {:?}", expected_seq, actual_names)
                    }],
                    "suggested_fix": null
                }))
            }
        }
        crate::cache::SequencePolicy::Rules(rules) => validate_rules(rules, &actual_names, None),
        crate::cache::SequencePolicy::V1_1(pol) => {
            validate_rules(&pol.sequences, &actual_names, Some(pol))
        }
    }
}

/// Whether the shared evaluator finds any violation. Exists for `tests/sequence_eval_parity.rs`.
pub fn validate_rules_for_parity(
    rules: &[SequenceRule],
    actual_names: &[String],
    policy_context: Option<&assay_core::model::Policy>,
) -> bool {
    validate_rules(rules, actual_names, policy_context)
        .ok()
        .and_then(|v| {
            v.get("violations")
                .map(|x| !x.as_array().is_none_or(|a| a.is_empty()))
        })
        .unwrap_or(false)
}

/// Live-proxy reading of the sequence-rule language: one call to the owner, then a
/// presentation map into the published tool JSON.
///
/// `assay-mcp-server` already depends on `assay-core`, and `assay-metrics` already calls
/// [`evaluate_rules`]. The remaining work was this map, not a second copy of the rule.
/// `TraceExtent::Partial` is the proxy question: history-so-far, more calls still possible.
fn validate_rules(
    rules: &[SequenceRule],
    actual_names: &[String],
    policy_context: Option<&assay_core::model::Policy>,
) -> Result<Value> {
    let calls: Vec<SequenceCall> = actual_names.iter().map(SequenceCall::named).collect();
    let evaluations = evaluate_rules(rules, &calls, policy_context, TraceExtent::Partial);
    let violations: Vec<Value> = rules
        .iter()
        .zip(evaluations.iter())
        .filter(|(_, ev)| ev.is_violation())
        .map(|(rule, ev)| published_violation(rule, ev))
        .collect();

    if violations.is_empty() {
        Ok(json!({ "allowed": true, "violations": [], "suggested_fix": null }))
    } else {
        Ok(json!({ "allowed": false, "violations": violations, "suggested_fix": null }))
    }
}

fn published_constraint(rule: &SequenceRule) -> &'static str {
    match rule {
        SequenceRule::Require { .. } => "sequence_rule",
        SequenceRule::Eventually { .. } => "eventually",
        SequenceRule::MaxCalls { .. } => "max_calls",
        SequenceRule::Before { .. } => "before",
        SequenceRule::After { .. } => "after",
        SequenceRule::NeverAfter { .. } => "never_after",
        SequenceRule::Sequence { strict: true, .. } => "sequence_strict",
        SequenceRule::Sequence { strict: false, .. } => "sequence_order",
        SequenceRule::Blocklist { .. } => "blocklist",
    }
}

fn published_tool(rule: &SequenceRule) -> Option<String> {
    match rule {
        SequenceRule::Require { tool }
        | SequenceRule::Eventually { tool, .. }
        | SequenceRule::MaxCalls { tool, .. } => Some(tool.to_string()),
        SequenceRule::Before { then, .. } | SequenceRule::After { then, .. } => {
            Some(then.to_string())
        }
        SequenceRule::NeverAfter { forbidden, .. } => Some(forbidden.to_string()),
        SequenceRule::Sequence { .. } | SequenceRule::Blocklist { .. } => None,
    }
}

/// Envelope keys the tool already published (`rule_type`, `constraint`, `message`), plus
/// `spanned` from the owner so span and prose cannot drift from the verdict.
fn published_violation(rule: &SequenceRule, ev: &RuleEvaluation) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("rule_type".into(), json!(ev.kind));
    obj.insert("constraint".into(), json!(published_constraint(rule)));
    obj.insert(
        "message".into(),
        json!(ev.reason.clone().unwrap_or_default()),
    );
    obj.insert("spanned".into(), json!(ev.spanned));
    if let Some(tool) = published_tool(rule) {
        obj.insert("tool".into(), json!(tool));
    }
    if let Some(&idx) = ev.spanned.last() {
        obj.insert("event_index".into(), json!(idx));
    }
    if let SequenceRule::MaxCalls { max, .. } = rule {
        obj.insert(
            "context".into(),
            json!({ "max": max, "actual": ev.spanned.len() as u32 }),
        );
    }
    Value::Object(obj)
}

#[cfg(test)]
mod after_obligation_tests {
    use assay_core::model::SequenceRule;

    fn n(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    fn violates(trace: &[&str], within: u32) -> bool {
        let rules = vec![SequenceRule::After {
            trigger: "T".into(),
            then: "A".into(),
            within,
        }];
        let v = super::validate_rules(&rules, &n(trace), None).unwrap();
        !v["violations"].as_array().unwrap().is_empty()
    }

    /// A `then` one call past the window does not answer the obligation. The satisfy-check ran
    /// before the deadline check, so this cleared it and the proxy allowed the sequence.
    #[test]
    fn a_late_then_does_not_answer_the_obligation() {
        assert!(violates(&["T", "X", "A"], 1));
    }

    /// A second trigger does not discharge the first one's unanswered obligation.
    #[test]
    fn a_new_trigger_does_not_clear_an_unanswered_one() {
        assert!(violates(&["T", "T", "A"], 1));
    }

    /// The case that distinguishes "check every obligation" from "check the first".
    ///
    /// `[T, A, T, X]` answers the trigger at 0 and leaves the one at 2 unanswered past its
    /// window. Checking only the first trigger reports clean here, and the previous test
    /// cannot see that -- its first trigger is already unanswered, so truncating to one
    /// obligation still produces a violation and the test stays green on broken code.
    #[test]
    fn a_later_obligation_is_checked_even_when_the_first_was_answered() {
        assert!(violates(&["T", "A", "T", "X"], 1));
    }

    /// The cases that were already right stay right.
    #[test]
    fn answered_within_the_window_still_holds() {
        assert!(!violates(&["T", "A"], 1));
        assert!(!violates(&["T", "X", "A"], 2));
        assert!(!violates(&["X", "Y"], 1));
    }

    /// #2228 control: both evaluators already agree this is a violation. The copy's JSON
    /// has no `spanned` and a shorter message, so nothing notices the shared record's span
    /// `[0]` or the "and no call answered it by index 1" suffix. After the call-through,
    /// the published violation must carry both from `evaluate_rules`.
    #[test]
    fn after_closed_window_carries_shared_span_and_prose() {
        use assay_core::sequence_eval::{evaluate_rules, SequenceCall, TraceExtent};

        let rules = vec![SequenceRule::After {
            trigger: "A".into(),
            then: "B".into(),
            within: 1,
        }];
        let names = n(&["A", "C"]);
        let calls: Vec<SequenceCall> = names.iter().map(SequenceCall::named).collect();
        let shared = evaluate_rules(&rules, &calls, None, TraceExtent::Partial);
        assert!(
            shared[0].is_violation(),
            "control: the shared evaluator reports a violation"
        );

        let published = super::validate_rules(&rules, &names, None).unwrap();
        let violations = published["violations"]
            .as_array()
            .expect("violations array");
        assert!(
            !violations.is_empty(),
            "control: the published tool also reports a violation"
        );
        assert_eq!(
            violations[0]["spanned"],
            serde_json::json!(shared[0].spanned),
            "span must come from sequence_eval, not a second copy"
        );
        assert_eq!(
            violations[0]["message"].as_str(),
            shared[0].reason.as_deref(),
            "prose must come from sequence_eval, not a second copy"
        );
    }
}
