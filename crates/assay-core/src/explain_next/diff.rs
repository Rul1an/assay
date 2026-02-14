use std::collections::HashMap;

use crate::model::{Policy, SequenceRule};

use super::model::RuleEvaluation;

/// Internal state tracking for stateful rules.
pub(crate) struct ExplainerState {
    /// Tools seen so far
    tools_seen: Vec<String>,

    /// Call counts per tool
    call_counts: HashMap<String, u32>,

    /// Whether specific tools have been seen (for before/after)
    tool_seen_flags: HashMap<String, bool>,

    /// Triggered state for never_after rules
    never_after_triggered: HashMap<usize, usize>, // rule_idx -> trigger_idx

    /// Pending "after" constraints: rule_idx -> (trigger_idx, deadline)
    pending_after: HashMap<usize, (usize, usize)>,

    /// Sequence progress: rule_idx -> current position in sequence
    sequence_progress: HashMap<usize, usize>,

    /// Aliases for resolution
    aliases: HashMap<String, Vec<String>>,
}

impl ExplainerState {
    pub(crate) fn new(policy: &Policy) -> Self {
        Self {
            tools_seen: Vec::new(),
            call_counts: HashMap::new(),
            tool_seen_flags: HashMap::new(),
            never_after_triggered: HashMap::new(),
            pending_after: HashMap::new(),
            sequence_progress: HashMap::new(),
            aliases: policy.aliases.clone(),
        }
    }

    fn resolve_alias(&self, tool: &str) -> Vec<String> {
        if let Some(members) = self.aliases.get(tool) {
            members.clone()
        } else {
            vec![tool.to_string()]
        }
    }

    fn matches(&self, tool: &str, target: &str) -> bool {
        let targets = self.resolve_alias(target);
        targets.contains(&tool.to_string())
    }

    pub(crate) fn evaluate_rule(
        &mut self,
        rule_idx: usize,
        rule: &SequenceRule,
        tool: &str,
        idx: usize,
    ) -> RuleEvaluation {
        match rule {
            SequenceRule::Require { tool: req_tool } => {
                // Require is checked at end of trace, always passes during
                RuleEvaluation {
                    rule_id: format!("require_{}", req_tool.to_lowercase()),
                    rule_type: "require".to_string(),
                    passed: true,
                    explanation: format!("Require '{}' (checked at end)", req_tool),
                    context: None,
                }
            }

            SequenceRule::Eventually {
                tool: ev_tool,
                within,
            } => {
                let targets = self.resolve_alias(ev_tool);
                let seen = self.tools_seen.iter().any(|t| targets.contains(t))
                    || targets.contains(&tool.to_string());

                let current_idx = idx as u32;
                let passed = seen || current_idx < *within;

                let explanation = if seen {
                    format!("'{}' already seen ✓", ev_tool)
                } else if current_idx < *within {
                    format!(
                        "'{}' required within {} calls (at {}/{})",
                        ev_tool,
                        within,
                        idx + 1,
                        within
                    )
                } else {
                    format!("'{}' not seen within first {} calls", ev_tool, within)
                };

                RuleEvaluation {
                    rule_id: format!("eventually_{}_{}", ev_tool.to_lowercase(), within),
                    rule_type: "eventually".to_string(),
                    passed,
                    explanation,
                    context: Some(serde_json::json!({
                        "required_tool": ev_tool,
                        "within": within,
                        "current_index": idx,
                        "seen": seen
                    })),
                }
            }

            SequenceRule::MaxCalls {
                tool: max_tool,
                max,
            } => {
                let targets = self.resolve_alias(max_tool);
                let current_count = if targets.contains(&tool.to_string()) {
                    self.call_counts.get(tool).copied().unwrap_or(0) + 1
                } else {
                    targets
                        .iter()
                        .map(|t| self.call_counts.get(t).copied().unwrap_or(0))
                        .sum()
                };

                let passed = current_count <= *max;

                let explanation = if passed {
                    format!("'{}' call {}/{}", max_tool, current_count, max)
                } else {
                    format!(
                        "'{}' exceeded max calls ({} > {})",
                        max_tool, current_count, max
                    )
                };

                RuleEvaluation {
                    rule_id: format!("max_calls_{}_{}", max_tool.to_lowercase(), max),
                    rule_type: "max_calls".to_string(),
                    passed,
                    explanation,
                    context: Some(serde_json::json!({
                        "tool": max_tool,
                        "max": max,
                        "current_count": current_count
                    })),
                }
            }

            SequenceRule::Before { first, then } => {
                let is_then = self.matches(tool, then);
                let first_seen = self.tool_seen_flags.get(first).copied().unwrap_or(false)
                    || self.tools_seen.iter().any(|t| self.matches(t, first));

                let passed = !is_then || first_seen;

                let explanation = if !is_then {
                    format!("Not '{}', rule not applicable", then)
                } else if first_seen {
                    format!("'{}' was called first ✓", first)
                } else {
                    format!("'{}' requires '{}' first", then, first)
                };

                RuleEvaluation {
                    rule_id: format!(
                        "before_{}_then_{}",
                        first.to_lowercase(),
                        then.to_lowercase()
                    ),
                    rule_type: "before".to_string(),
                    passed,
                    explanation,
                    context: Some(serde_json::json!({
                        "first": first,
                        "then": then,
                        "first_seen": first_seen,
                        "is_then_call": is_then
                    })),
                }
            }

            SequenceRule::After {
                trigger,
                then,
                within,
            } => {
                let is_trigger = self.matches(tool, trigger);
                let is_then = self.matches(tool, then);

                // Check if we're past deadline
                let mut passed = true;
                let explanation;

                if let Some((trigger_idx, deadline)) = self.pending_after.get(&rule_idx) {
                    if is_then {
                        if idx <= *deadline {
                            explanation = format!("'{}' satisfies after '{}' ✓", then, trigger);
                        } else {
                            passed = false;
                            explanation = format!(
                                "'{}' called too late after '{}' (at {}, deadline {})",
                                then, trigger, idx, deadline
                            );
                        }
                    } else if idx > *deadline {
                        passed = false;
                        explanation = format!(
                            "'{}' required within {} calls after '{}' (triggered at {})",
                            then, within, trigger, trigger_idx
                        );
                    } else {
                        explanation = format!(
                            "Pending: '{}' needed within {} more calls",
                            then,
                            deadline - idx
                        );
                    }
                } else if is_trigger {
                    explanation = format!(
                        "'{}' triggered, '{}' required within {}",
                        trigger, then, within
                    );
                } else {
                    explanation = format!("After rule: waiting for '{}'", trigger);
                }

                RuleEvaluation {
                    rule_id: format!(
                        "after_{}_then_{}",
                        trigger.to_lowercase(),
                        then.to_lowercase()
                    ),
                    rule_type: "after".to_string(),
                    passed,
                    explanation,
                    context: Some(serde_json::json!({
                        "trigger": trigger,
                        "then": then,
                        "within": within
                    })),
                }
            }

            SequenceRule::NeverAfter { trigger, forbidden } => {
                let is_trigger = self.matches(tool, trigger);
                let is_forbidden = self.matches(tool, forbidden);
                let triggered = self.never_after_triggered.contains_key(&rule_idx);

                let passed = !(triggered && is_forbidden);

                let explanation = if !triggered && is_trigger {
                    format!("'{}' triggered, '{}' now forbidden", trigger, forbidden)
                } else if triggered && is_forbidden {
                    let trigger_idx = self.never_after_triggered.get(&rule_idx).unwrap();
                    format!(
                        "'{}' forbidden after '{}' (triggered at index {})",
                        forbidden, trigger, trigger_idx
                    )
                } else if triggered {
                    format!(
                        "'{}' forbidden (trigger at {})",
                        forbidden,
                        self.never_after_triggered.get(&rule_idx).unwrap()
                    )
                } else {
                    format!("Waiting for trigger '{}'", trigger)
                };

                RuleEvaluation {
                    rule_id: format!(
                        "never_after_{}_forbidden_{}",
                        trigger.to_lowercase(),
                        forbidden.to_lowercase()
                    ),
                    rule_type: "never_after".to_string(),
                    passed,
                    explanation,
                    context: Some(serde_json::json!({
                        "trigger": trigger,
                        "forbidden": forbidden,
                        "triggered": triggered || is_trigger
                    })),
                }
            }

            SequenceRule::Sequence { tools, strict } => {
                let seq_idx = self.sequence_progress.get(&rule_idx).copied().unwrap_or(0);

                let mut passed = true;
                let explanation;

                if seq_idx < tools.len() {
                    let expected = &tools[seq_idx];
                    let is_expected = self.matches(tool, expected);

                    if *strict {
                        // In strict mode, if sequence started, next must be expected
                        if seq_idx > 0 && !is_expected {
                            passed = false;
                            explanation = format!(
                                "Strict sequence: expected '{}' but got '{}'",
                                expected, tool
                            );
                        } else if is_expected {
                            explanation = format!(
                                "Sequence step {}/{}: '{}' ✓",
                                seq_idx + 1,
                                tools.len(),
                                tool
                            );
                        } else {
                            explanation = format!("Waiting for sequence start: '{}'", tools[0]);
                        }
                    } else {
                        // Non-strict: check for out-of-order
                        let future_match = tools
                            .iter()
                            .skip(seq_idx + 1)
                            .position(|t| self.matches(tool, t));

                        if future_match.is_some() {
                            passed = false;
                            explanation = format!(
                                "Sequence order violated: '{}' before '{}'",
                                tool, expected
                            );
                        } else if is_expected {
                            explanation = format!(
                                "Sequence step {}/{}: '{}' ✓",
                                seq_idx + 1,
                                tools.len(),
                                tool
                            );
                        } else {
                            explanation = format!(
                                "Sequence: waiting for '{}' ({}/{})",
                                expected,
                                seq_idx,
                                tools.len()
                            );
                        }
                    }
                } else {
                    explanation = "Sequence complete ✓".to_string();
                }

                RuleEvaluation {
                    rule_id: format!("sequence_{}", tools.join("_").to_lowercase()),
                    rule_type: "sequence".to_string(),
                    passed,
                    explanation,
                    context: Some(serde_json::json!({
                        "tools": tools,
                        "strict": strict,
                        "progress": seq_idx
                    })),
                }
            }

            SequenceRule::Blocklist { pattern } => {
                let passed = !tool.contains(pattern);

                let explanation = if passed {
                    format!("'{}' does not match blocklist '{}'", tool, pattern)
                } else {
                    format!("'{}' matches blocklist pattern '{}'", tool, pattern)
                };

                RuleEvaluation {
                    rule_id: format!("blocklist_{}", pattern.to_lowercase()),
                    rule_type: "blocklist".to_string(),
                    passed,
                    explanation,
                    context: None,
                }
            }
        }
    }

    pub(crate) fn update(&mut self, tool: &str, idx: usize, policy: &Policy) {
        // Update call counts
        *self.call_counts.entry(tool.to_string()).or_insert(0) += 1;

        // Update seen flags
        self.tool_seen_flags.insert(tool.to_string(), true);

        // Update rule-specific state
        for (rule_idx, rule) in policy.sequences.iter().enumerate() {
            match rule {
                SequenceRule::NeverAfter { trigger, .. } => {
                    if self.matches(tool, trigger)
                        && !self.never_after_triggered.contains_key(&rule_idx)
                    {
                        self.never_after_triggered.insert(rule_idx, idx);
                    }
                }
                SequenceRule::After {
                    trigger, within, ..
                } => {
                    if self.matches(tool, trigger) {
                        // Start/restart the deadline timer on trigger
                        // Note: If triggered multiple times, this implementation updates to the LATEST trigger.
                        // This matches "within N calls after [any] trigger".
                        self.pending_after
                            .insert(rule_idx, (idx, idx + *within as usize));
                    }
                }
                SequenceRule::Sequence { tools, .. } => {
                    let seq_idx = self.sequence_progress.get(&rule_idx).copied().unwrap_or(0);
                    if seq_idx < tools.len() && self.matches(tool, &tools[seq_idx]) {
                        self.sequence_progress.insert(rule_idx, seq_idx + 1);
                    }
                }
                _ => {}
            }
        }

        // Add to tools seen
        self.tools_seen.push(tool.to_string());
    }

    pub(crate) fn check_end_of_trace(&self, policy: &Policy) -> Vec<RuleEvaluation> {
        let mut violations = Vec::new();

        for (rule_idx, rule) in policy.sequences.iter().enumerate() {
            match rule {
                SequenceRule::Require { tool } => {
                    let requirements = self.resolve_alias(tool);
                    let ok = self.tools_seen.iter().any(|t| requirements.contains(t));

                    if !ok {
                        violations.push(RuleEvaluation {
                            rule_id: format!("require_{}", tool.to_lowercase()),
                            rule_type: "require".to_string(),
                            passed: false,
                            explanation: format!("Required tool '{}' never called", tool),
                            context: None,
                        });
                    }
                }
                SequenceRule::After {
                    trigger,
                    then,
                    within,
                } => {
                    // If we have a pending deadline that wasn't satisfied
                    if let Some((trigger_idx, deadline)) = self.pending_after.get(&rule_idx) {
                        // Check if we saw 'then' AFTER the trigger
                        // Note: self.tools_seen contains all calls.
                        // We need to see if 'then' appeared between trigger_idx+1 and end (or deadline).
                        let then_targets = self.resolve_alias(then);
                        let seen_after = self
                            .tools_seen
                            .iter()
                            .skip(*trigger_idx + 1)
                            .any(|t| then_targets.contains(t));

                        if !seen_after {
                            violations.push(RuleEvaluation {
                                rule_id: format!(
                                    "after_{}_then_{}",
                                    trigger.to_lowercase(),
                                    then.to_lowercase()
                                ),
                                rule_type: "after".to_string(),
                                passed: false,
                                explanation: format!(
                                    "'{}' triggered at {}, but '{}' never called within {} steps (trace ended)",
                                    trigger, trigger_idx, then, within
                                ),
                                context: Some(serde_json::json!({
                                    "trigger": trigger,
                                    "deadline": deadline,
                                    "trace_len": self.tools_seen.len()
                                })),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        violations
    }

    pub(crate) fn snapshot(&self) -> HashMap<String, String> {
        let mut snap = HashMap::new();

        for (tool, count) in &self.call_counts {
            if *count > 0 {
                snap.insert(format!("calls:{}", tool), count.to_string());
            }
        }

        snap
    }
}
