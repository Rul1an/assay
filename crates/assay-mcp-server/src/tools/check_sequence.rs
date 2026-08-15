use super::{ToolContext, ToolError};
use anyhow::{Context, Result};
use serde_json::Value;

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

/// Whether this evaluator finds any violation. Exists only for `tests/sequence_eval_parity.rs`.
///
/// This copy of the rule language has not called through to `assay_core::sequence_eval` yet,
/// because its JSON violation shape is a published tool contract. Until it does, the parity test
/// is what keeps the two from drifting, and it needs a verdict it can compare. Deliberately
/// narrow: a bool, not the violation payload, so nothing outside this crate grows a dependency
/// on the shape while the port is pending.
pub fn validate_rules_for_parity(
    rules: &[assay_core::model::SequenceRule],
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

// TODO(sequence-v11): call `assay_core::sequence_eval::evaluate_rules` instead of this copy.
// Blocked on preserving the JSON violation shape, which is a published tool contract.
fn validate_rules(
    rules: &[assay_core::model::SequenceRule],
    actual_names: &[String],
    policy_context: Option<&assay_core::model::Policy>,
) -> Result<Value> {
    let mut violations = Vec::new();

    // Helper to resolve aliases
    let resolve = |tool: &str| -> Vec<String> {
        if let Some(ctx) = policy_context {
            ctx.resolve_alias(tool)
        } else {
            vec![tool.to_string()]
        }
    };

    // Helper to check if tool matches any target
    let matches_any =
        |tool_name: &str, targets: &[String]| -> bool { targets.iter().any(|t| t == tool_name) };

    for rule in rules {
        match rule {
            // ===== REQUIRE (v1.0 legacy) =====
            assay_core::model::SequenceRule::Require { tool } => {
                let targets = resolve(tool.tool());
                // Requirement: At least ONE of the alias members must be present in history
                let found = targets.iter().any(|t| actual_names.contains(t));
                if !found {
                    let msg = if targets.len() > 1 {
                        format!(
                            "required tool '{}' (aliases: {:?}) not found",
                            tool, targets
                        )
                    } else {
                        format!("required tool '{}' not found", tool)
                    };
                    violations.push(serde_json::json!({
                        "rule_type": "require",
                        "constraint": "sequence_rule",
                        "message": msg
                    }));
                }
            }

            // ===== EVENTUALLY: tool must appear within first N calls =====
            assay_core::model::SequenceRule::Eventually { tool, within } => {
                let targets = resolve(tool.tool());
                let found_idx = actual_names.iter().position(|n| matches_any(n, &targets));

                if let Some(idx) = found_idx {
                    // 0-based index. "within: 3" means indices 0, 1, 2 are valid.
                    if (idx as u32) >= *within {
                        violations.push(serde_json::json!({
                            "rule_type": "eventually",
                            "tool": tool,
                            "event_index": idx,
                            "constraint": "eventually",
                            "message": format!(
                                "tool '{}' appeared at index {} but must appear within first {} calls",
                                tool, idx, within
                            )
                        }));
                    }
                } else {
                    // Not found. If trace length > within, we've missed the deadline.
                    // `>=`, not `>`. The window is indices `0..within-1`, so it is spent once the
                    // trace reaches `within` calls -- not one call later. At `within: 2` a
                    // two-call trace has already used both chances.
                    if (actual_names.len() as u32) >= *within {
                        violations.push(serde_json::json!({
                            "rule_type": "eventually",
                            "tool": tool,
                            "event_index": actual_names.len() - 1,
                            "constraint": "eventually",
                            "message": format!(
                                "tool '{}' required within first {} calls but not found (trace length: {})",
                                tool, within, actual_names.len()
                            )
                        }));
                    }
                }
            }

            // ===== MAX_CALLS: tool can be called at most N times =====
            assay_core::model::SequenceRule::MaxCalls { tool, max } => {
                let targets = resolve(tool.tool());
                let mut count = 0u32;
                let mut violation_idx = None;

                for (idx, name) in actual_names.iter().enumerate() {
                    if matches_any(name, &targets) {
                        count += 1;
                        if count > *max && violation_idx.is_none() {
                            violation_idx = Some(idx);
                        }
                    }
                }

                if let Some(idx) = violation_idx {
                    violations.push(serde_json::json!({
                        "rule_type": "max_calls",
                        "tool": tool,
                        "event_index": idx,
                        "constraint": "max_calls",
                        "message": format!(
                            "tool '{}' exceeded max calls ({} > {})",
                            tool, count, max
                        ),
                        "context": {
                            "max": max,
                            "actual": count
                        }
                    }));
                }
            }

            // ===== BEFORE: first must be called before then =====
            assay_core::model::SequenceRule::Before { first, then } => {
                let first_targets = resolve(first.tool());
                let then_targets = resolve(then.tool());

                // Check positions
                let first_idx = actual_names
                    .iter()
                    .position(|n| matches_any(n, &first_targets));
                let then_idx = actual_names
                    .iter()
                    .position(|n| matches_any(n, &then_targets));

                // Only check if 'then' was called
                if let Some(t_idx) = then_idx {
                    if let Some(f_idx) = first_idx {
                        if f_idx > t_idx {
                            violations.push(serde_json::json!({
                                "rule_type": "before",
                                "tool": then,
                                "event_index": t_idx,
                                "constraint": "before",
                                "message": format!(
                                    "tool '{}' at index {} requires '{}' to be called first (found at index {})",
                                    then, t_idx, first, f_idx
                                ),
                                "context": {
                                    "required_tool": first,
                                    "required_tool_seen": true,
                                    "required_tool_index": f_idx
                                }
                            }));
                        }
                    } else {
                        violations.push(serde_json::json!({
                            "rule_type": "before",
                            "tool": then,
                            "event_index": t_idx,
                            "constraint": "before",
                            "message": format!(
                                "tool '{}' at index {} requires '{}' to be called first",
                                then, t_idx, first
                            ),
                            "context": {
                                "required_tool": first,
                                "required_tool_seen": false
                            }
                        }));
                    }
                }
            }

            // ===== AFTER: after trigger, then must occur within N calls =====
            assay_core::model::SequenceRule::After {
                trigger,
                then,
                within,
            } => {
                let trigger_targets = resolve(trigger.tool());
                let then_targets = resolve(then.tool());

                // Each trigger is its own obligation, checked against its own window.
                //
                // This used to carry one mutable `pending_deadline` slot, and leaked a violation
                // through it two ways. The satisfy-check ran before the deadline check, so a
                // `then` arriving one call past the window cleared the obligation instead of
                // failing it: `[T, X, A]` at `within: 1` reported no violation. And a new trigger
                // overwrote an unsatisfied one, so `[T, T, A]` reported none either, while the
                // trigger at index 0 was never answered. This is the enforcing path, so both were
                // permissive: the proxy allowed a call sequence the policy forbids.
                //
                // The JSON below is unchanged -- same keys, same two message forms, same
                // conditions for choosing between them -- because it is a published tool contract.
                let trigger_indices: Vec<usize> = actual_names
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| matches_any(n, &trigger_targets))
                    .map(|(i, _)| i)
                    .collect();

                for trigger_idx in trigger_indices {
                    let deadline = trigger_idx + (*within as usize);
                    let answered = actual_names
                        .iter()
                        .enumerate()
                        .skip(trigger_idx + 1)
                        .take_while(|(j, _)| *j <= deadline)
                        .any(|(_, n)| matches_any(n, &then_targets));
                    if answered {
                        continue;
                    }

                    if actual_names.len() > deadline {
                        // The window closed inside the trace.
                        violations.push(serde_json::json!({
                            "rule_type": "after",
                            "tool": then,
                            "event_index": deadline + 1,
                            "constraint": "after",
                            "message": format!(
                                "tool '{}' required within {} calls after '{}' (triggered at index {})",
                                then, within, trigger, trigger_idx
                            ),
                            "context": {
                                "trigger": trigger,
                                "trigger_index": trigger_idx,
                                "within": within
                            }
                        }));
                    }
                    // No `else`. A trace that ended inside the window has not missed the
                    // deadline: this evaluator validates history-so-far, and the next call can
                    // still answer the obligation. The original guarded its end-of-trace
                    // violation on `len > deadline` for exactly this reason, and an earlier
                    // version of this rewrite dropped the guard -- which made the proxy stricter
                    // than the shared evaluator and tripped the parity test in #2112 on 58 cases.
                }
            }

            // ===== NEVER_AFTER: after trigger, forbidden is permanently denied =====
            assay_core::model::SequenceRule::NeverAfter { trigger, forbidden } => {
                let trigger_targets = resolve(trigger.tool());
                let forbidden_targets = resolve(forbidden.tool());

                let mut triggered = false;
                let mut trigger_idx = 0usize;

                for (idx, name) in actual_names.iter().enumerate() {
                    // Check forbidden first if already triggered
                    if triggered && matches_any(name, &forbidden_targets) {
                        violations.push(serde_json::json!({
                            "rule_type": "never_after",
                            "tool": forbidden,
                            "event_index": idx,
                            "constraint": "never_after",
                            "message": format!(
                                "tool '{}' at index {} is forbidden after '{}' (triggered at index {})",
                                forbidden, idx, trigger, trigger_idx
                            ),
                            "context": {
                                "trigger": trigger,
                                "trigger_index": trigger_idx
                            }
                        }));
                        break; // One violation is enough
                    }

                    // Check for trigger
                    // Note: if trigger and forbidden are same, it triggers immediately on next call?
                    // Or on itself? "after trigger". Usually implies strict >.
                    // But if triggered is set, we check forbidden.
                    // If we set triggered AFTER checking forbidden for current item:
                    if !triggered && matches_any(name, &trigger_targets) {
                        triggered = true;
                        trigger_idx = idx;
                    }
                }
            }

            // ===== SEQUENCE: exact ordering (with optional strict mode) =====
            assay_core::model::SequenceRule::Sequence { tools, strict } => {
                // Resolve all tools through aliases
                let tool_targets: Vec<Vec<String>> =
                    tools.iter().map(|t| resolve(t.tool())).collect();

                if *strict {
                    // Strict mode: tools must appear consecutively in exact order
                    let mut seq_idx = 0usize;
                    let mut started = false;
                    let mut start_idx = 0usize;

                    for (idx, name) in actual_names.iter().enumerate() {
                        if seq_idx < tool_targets.len() && matches_any(name, &tool_targets[seq_idx])
                        {
                            if !started {
                                started = true;
                                start_idx = idx;
                            }
                            seq_idx += 1;
                        } else if started && seq_idx < tool_targets.len() {
                            // We started the sequence but this tool doesn't match next expected
                            violations.push(serde_json::json!({
                                "rule_type": "sequence",
                                "tool": name,
                                "event_index": idx,
                                "constraint": "sequence_strict",
                                "message": format!(
                                    "strict sequence violated: expected '{}' at index {} but found '{}'",
                                    tools[seq_idx], idx, name
                                ),
                                "context": {
                                    "expected": tools[seq_idx],
                                    "actual": name,
                                    "sequence_start": start_idx
                                }
                            }));
                            break;
                        }
                    }

                    // Note: Check for completeness is usually for "trace end".
                    // But check_sequence validates partial traces too.
                    // If incomplete, it's NOT a violation unless we know trace ended.
                    // But Assay doesn't know if trace ended.
                    // So we only enforce negative constraints (wrong order, intervening tools).
                    // We DO NOT enforce "missing tail of sequence" here unless explicit?
                    // The RFC says "All tools in sequence must appear for trace to pass".
                    // But if we are mid-trace, we can't fail yet.
                    // So we skip the "incomplete" check for now in strict mode too.
                } else {
                    // Non-strict: tools must appear in order but other tools can be between
                    let mut seq_idx = 0usize;
                    // let mut out_of_order_detected = false;

                    for (idx, name) in actual_names.iter().enumerate() {
                        if seq_idx < tool_targets.len() && matches_any(name, &tool_targets[seq_idx])
                        {
                            seq_idx += 1;
                        } else {
                            // Check for out-of-order: a later sequence member appearing before current
                            for (future_seq_idx, future_targets) in
                                tool_targets.iter().enumerate().skip(seq_idx + 1)
                            {
                                if matches_any(name, future_targets) {
                                    violations.push(serde_json::json!({
                                        "rule_type": "sequence",
                                        "tool": name,
                                        "event_index": idx,
                                        "constraint": "sequence_order",
                                        "message": format!(
                                            "sequence order violated: '{}' at index {} appeared before '{}'",
                                            tools[future_seq_idx], idx, tools[seq_idx]
                                        ),
                                        "context": {
                                            "expected_next": tools[seq_idx],
                                            "found_later": tools[future_seq_idx]
                                        }
                                    }));
                                    // out_of_order_detected = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // ===== BLOCKLIST (v1.0 legacy pattern matching) =====
            assay_core::model::SequenceRule::Blocklist { pattern } => {
                for (idx, name) in actual_names.iter().enumerate() {
                    if name.contains(pattern) {
                        violations.push(serde_json::json!({
                            "rule_type": "blocklist",
                            "tool": name,
                            "event_index": idx,
                            "constraint": "blocklist",
                            "message": format!(
                                "tool '{}' at index {} matches blocklist pattern '{}'",
                                name, idx, pattern
                            )
                        }));
                    }
                }
            }
        }
    }

    if violations.is_empty() {
        Ok(serde_json::json!({ "allowed": true, "violations": [], "suggested_fix": null }))
    } else {
        Ok(serde_json::json!({ "allowed": false, "violations": violations, "suggested_fix": null }))
    }
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
}
