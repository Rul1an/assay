//! Trace explanation and visualization
//!
//! Evaluates a trace against a policy and produces a step-by-step
//! explanation of what happened at each tool call.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single step in the explained trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainedStep {
    pub index: usize,
    pub tool: String,

    /// Tool arguments (if available)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,

    /// Verdict for this step
    pub verdict: StepVerdict,

    /// Rules that were evaluated
    pub rules_evaluated: Vec<RuleEvaluation>,

    /// Current state of stateful rules after this step
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state_snapshot: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StepVerdict {
    /// Tool call allowed
    Allowed,
    /// Tool call blocked by a rule
    Blocked,
    /// Tool call allowed but triggered a warning
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEvaluation {
    pub rule_id: String,
    pub rule_type: String,
    pub passed: bool,
    pub explanation: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Complete explanation of a trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceExplanation {
    pub policy_name: String,
    pub policy_version: String,
    pub total_steps: usize,
    pub allowed_steps: usize,
    pub blocked_steps: usize,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_block_index: Option<usize>,

    /// Detailed step-by-step explanation
    pub steps: Vec<ExplainedStep>,

    /// Summary of rules that blocked
    pub blocking_rules: Vec<String>,
}

/// Tool call input for explanation
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    /// Tool name
    #[serde(alias = "name", alias = "tool_name")]
    pub tool: String,

    /// Tool arguments
    #[serde(default)]
    pub args: Option<serde_json::Value>,
}

/// Trace explainer
pub struct TraceExplainer {
    policy: crate::model::Policy,
}

impl TraceExplainer {
    pub fn new(policy: crate::model::Policy) -> Self {
        Self { policy }
    }

    /// Explain a trace step by step
    pub fn explain(&self, trace: &[ToolCall]) -> TraceExplanation {
        let mut steps = Vec::new();
        let mut state = ExplainerState::new(&self.policy);
        let mut first_block_index = None;
        let mut blocking_rules = Vec::new();

        for (idx, call) in trace.iter().enumerate() {
            let (step, blocked_by) = self.explain_step(idx, call, &mut state);

            if step.verdict == StepVerdict::Blocked && first_block_index.is_none() {
                first_block_index = Some(idx);
            }

            if let Some(rule) = blocked_by {
                if !blocking_rules.contains(&rule) {
                    blocking_rules.push(rule);
                }
            }

            steps.push(step);
        }

        // Check end-of-trace constraints
        let end_violations = state.check_end_of_trace(&self.policy);
        if !end_violations.is_empty() && !steps.is_empty() {
            let last_idx = steps.len() - 1;
            for violation in end_violations {
                steps[last_idx].rules_evaluated.push(violation.clone());
                if !blocking_rules.contains(&violation.rule_id) {
                    blocking_rules.push(violation.rule_id);
                }
            }
        }

        let allowed_steps = steps
            .iter()
            .filter(|s| s.verdict == StepVerdict::Allowed)
            .count();
        let blocked_steps = steps
            .iter()
            .filter(|s| s.verdict == StepVerdict::Blocked)
            .count();

        TraceExplanation {
            policy_name: self.policy.name.clone(),
            policy_version: self.policy.version.clone(),
            total_steps: steps.len(),
            allowed_steps,
            blocked_steps,
            first_block_index,
            steps,
            blocking_rules,
        }
    }

    fn explain_step(
        &self,
        idx: usize,
        call: &ToolCall,
        state: &mut ExplainerState,
    ) -> (ExplainedStep, Option<String>) {
        let mut rules_evaluated = Vec::new();
        let mut verdict = StepVerdict::Allowed;
        let mut blocked_by = None;

        // Check static constraints (allow/deny lists)
        if let Some(eval) = self.check_static_constraints(&call.tool) {
            if !eval.passed {
                verdict = StepVerdict::Blocked;
                blocked_by = Some(eval.rule_id.clone());
            }
            rules_evaluated.push(eval);
        }

        // Check each sequence rule
        for (rule_idx, rule) in self.policy.sequences.iter().enumerate() {
            let eval = state.evaluate_rule(rule_idx, rule, &call.tool, idx);

            if !eval.passed && verdict != StepVerdict::Blocked {
                verdict = StepVerdict::Blocked;
                blocked_by = Some(eval.rule_id.clone());
            }

            rules_evaluated.push(eval);
        }

        // Update state after evaluation
        state.update(&call.tool, idx, &self.policy);

        let step = ExplainedStep {
            index: idx,
            tool: call.tool.clone(),
            args: call.args.clone(),
            verdict,
            rules_evaluated,
            state_snapshot: state.snapshot(),
        };

        (step, blocked_by)
    }

    fn check_static_constraints(&self, tool: &str) -> Option<RuleEvaluation> {
        // Check deny list first
        if let Some(deny) = &self.policy.tools.deny {
            if deny.contains(&tool.to_string()) {
                return Some(RuleEvaluation {
                    rule_id: "deny_list".to_string(),
                    rule_type: "deny".to_string(),
                    passed: false,
                    explanation: format!("Tool '{}' is in deny list", tool),
                    context: None,
                });
            }
        }

        // Check allow list
        if let Some(allow) = &self.policy.tools.allow {
            if !allow.contains(&tool.to_string()) && !self.is_alias_member(tool) {
                return Some(RuleEvaluation {
                    rule_id: "allow_list".to_string(),
                    rule_type: "allow".to_string(),
                    passed: false,
                    explanation: format!("Tool '{}' is not in allow list", tool),
                    context: None,
                });
            }
        }

        None
    }

    fn is_alias_member(&self, tool: &str) -> bool {
        for members in self.policy.aliases.values() {
            if members.contains(&tool.to_string()) {
                return true;
            }
        }
        false
    }
}

/// Internal state tracking for stateful rules
struct ExplainerState {
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
    fn new(policy: &crate::model::Policy) -> Self {
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

    fn evaluate_rule(
        &mut self,
        rule_idx: usize,
        rule: &crate::model::SequenceRule,
        tool: &str,
        idx: usize,
    ) -> RuleEvaluation {
        match rule {
            crate::model::SequenceRule::Require { tool: req_tool } => {
                // Require is checked at end of trace, always passes during
                RuleEvaluation {
                    rule_id: format!("require_{}", req_tool.to_lowercase()),
                    rule_type: "require".to_string(),
                    passed: true,
                    explanation: format!("Require '{}' (checked at end)", req_tool),
                    context: None,
                }
            }

            crate::model::SequenceRule::Eventually {
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

            crate::model::SequenceRule::MaxCalls {
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

            crate::model::SequenceRule::Before { first, then } => {
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

            crate::model::SequenceRule::After {
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

            crate::model::SequenceRule::NeverAfter { trigger, forbidden } => {
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

            crate::model::SequenceRule::Sequence { tools, strict } => {
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

            crate::model::SequenceRule::Blocklist { pattern } => {
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

    fn update(&mut self, tool: &str, idx: usize, policy: &crate::model::Policy) {
        // Update call counts
        *self.call_counts.entry(tool.to_string()).or_insert(0) += 1;

        // Update seen flags
        self.tool_seen_flags.insert(tool.to_string(), true);

        // Update rule-specific state
        for (rule_idx, rule) in policy.sequences.iter().enumerate() {
            match rule {
                crate::model::SequenceRule::NeverAfter { trigger, .. } => {
                    if self.matches(tool, trigger)
                        && !self.never_after_triggered.contains_key(&rule_idx)
                    {
                        self.never_after_triggered.insert(rule_idx, idx);
                    }
                }
                crate::model::SequenceRule::After {
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
                crate::model::SequenceRule::Sequence { tools, .. } => {
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

    fn check_end_of_trace(&self, policy: &crate::model::Policy) -> Vec<RuleEvaluation> {
        let mut violations = Vec::new();

        for (rule_idx, rule) in policy.sequences.iter().enumerate() {
            match rule {
                crate::model::SequenceRule::Require { tool } => {
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
                crate::model::SequenceRule::After {
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
                                 rule_id: format!("after_{}_then_{}", trigger.to_lowercase(), then.to_lowercase()),
                                 rule_type: "after".to_string(),
                                 passed: false,
                                 explanation: format!("'{}' triggered at {}, but '{}' never called within {} steps (trace ended)", trigger, trigger_idx, then, within),
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

    fn snapshot(&self) -> HashMap<String, String> {
        let mut snap = HashMap::new();

        for (tool, count) in &self.call_counts {
            if *count > 0 {
                snap.insert(format!("calls:{}", tool), count.to_string());
            }
        }

        snap
    }
}

impl TraceExplanation {
    /// Format as terminal output with colors
    pub fn to_terminal(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!(
            "Policy: {} (v{})",
            self.policy_name, self.policy_version
        ));
        lines.push(format!(
            "Trace: {} steps ({} allowed, {} blocked)\n",
            self.total_steps, self.allowed_steps, self.blocked_steps
        ));

        lines.push("Timeline:".to_string());

        for step in &self.steps {
            let icon = match step.verdict {
                StepVerdict::Allowed => "✅",
                StepVerdict::Blocked => "❌",
                StepVerdict::Warning => "⚠️",
            };

            let args_str = step
                .args
                .as_ref()
                .map(|a| format!("({})", summarize_args(a)))
                .unwrap_or_default();

            let status = match step.verdict {
                StepVerdict::Allowed => "allowed".to_string(),
                StepVerdict::Blocked => "BLOCKED".to_string(),
                StepVerdict::Warning => "warning".to_string(),
            };

            lines.push(format!(
                "  [{}] {}{:<40} {} {}",
                step.index, step.tool, args_str, icon, status
            ));

            // Show blocking rule details
            if step.verdict == StepVerdict::Blocked {
                for eval in &step.rules_evaluated {
                    if !eval.passed {
                        lines.push(format!("      └── Rule: {}", eval.rule_id));
                        lines.push(format!("      └── Reason: {}", eval.explanation));
                    }
                }
            }
        }

        if !self.blocking_rules.is_empty() {
            lines.push(String::new());
            lines.push("Blocking Rules:".to_string());
            for rule in &self.blocking_rules {
                lines.push(format!("  - {}", rule));
            }
        }

        lines.join("\n")
    }

    /// Format as markdown
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        let status = if self.blocked_steps == 0 {
            "✅ PASS"
        } else {
            "❌ BLOCKED"
        };

        md.push_str(&format!("## Trace Explanation {}\n\n", status));
        md.push_str(&format!(
            "**Policy:** {} (v{})\n\n",
            self.policy_name, self.policy_version
        ));
        md.push_str("| Steps | Allowed | Blocked |\n");
        md.push_str("|-------|---------|----------|\n");
        md.push_str(&format!(
            "| {} | {} | {} |\n\n",
            self.total_steps, self.allowed_steps, self.blocked_steps
        ));

        md.push_str("### Timeline\n\n");
        md.push_str("| # | Tool | Verdict | Details |\n");
        md.push_str("|---|------|---------|----------|\n");

        for step in &self.steps {
            let icon = match step.verdict {
                StepVerdict::Allowed => "✅",
                StepVerdict::Blocked => "❌",
                StepVerdict::Warning => "⚠️",
            };

            let details = if step.verdict == StepVerdict::Blocked {
                step.rules_evaluated
                    .iter()
                    .filter(|e| !e.passed)
                    .map(|e| e.explanation.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            } else {
                String::new()
            };

            md.push_str(&format!(
                "| {} | `{}` | {} | {} |\n",
                step.index, step.tool, icon, details
            ));
        }

        if !self.blocking_rules.is_empty() {
            md.push_str("\n### Blocking Rules\n\n");
            for rule in &self.blocking_rules {
                md.push_str(&format!("- `{}`\n", rule));
            }
        }

        md
    }

    /// Format as HTML
    pub fn to_html(&self) -> String {
        let mut html = String::new();

        html.push_str("<!DOCTYPE html>\n<html><head>\n");
        html.push_str("<meta charset=\"utf-8\">\n");
        html.push_str("<title>Trace Explanation</title>\n");
        html.push_str("<style>\n");
        html.push_str("body { font-family: system-ui, sans-serif; max-width: 900px; margin: 2rem auto; padding: 0 1rem; }\n");
        html.push_str(".step { padding: 0.5rem; margin: 0.25rem 0; border-radius: 4px; }\n");
        html.push_str(".allowed { background: #d4edda; }\n");
        html.push_str(".blocked { background: #f8d7da; }\n");
        html.push_str(".warning { background: #fff3cd; }\n");
        html.push_str(".rule-detail { margin-left: 2rem; color: #666; font-size: 0.9em; }\n");
        html.push_str(
            "code { background: #f4f4f4; padding: 0.2rem 0.4rem; border-radius: 3px; }\n",
        );
        html.push_str("</style>\n</head><body>\n");

        let status = if self.blocked_steps == 0 {
            "✅ PASS"
        } else {
            "❌ BLOCKED"
        };
        html.push_str(&format!("<h1>Trace Explanation {}</h1>\n", status));
        html.push_str(&format!(
            "<p><strong>Policy:</strong> {} (v{})</p>\n",
            self.policy_name, self.policy_version
        ));
        html.push_str(&format!(
            "<p><strong>Summary:</strong> {} steps ({} allowed, {} blocked)</p>\n",
            self.total_steps, self.allowed_steps, self.blocked_steps
        ));

        html.push_str("<h2>Timeline</h2>\n");

        for step in &self.steps {
            let class = match step.verdict {
                StepVerdict::Allowed => "allowed",
                StepVerdict::Blocked => "blocked",
                StepVerdict::Warning => "warning",
            };

            let icon = match step.verdict {
                StepVerdict::Allowed => "✅",
                StepVerdict::Blocked => "❌",
                StepVerdict::Warning => "⚠️",
            };

            html.push_str(&format!("<div class=\"step {}\">\n", class));
            html.push_str(&format!(
                "  <strong>[{}]</strong> <code>{}</code> {}\n",
                step.index, step.tool, icon
            ));

            if step.verdict == StepVerdict::Blocked {
                for eval in &step.rules_evaluated {
                    if !eval.passed {
                        html.push_str(&format!(
                            "  <div class=\"rule-detail\">Rule: <code>{}</code> — {}</div>\n",
                            eval.rule_id, eval.explanation
                        ));
                    }
                }
            }

            html.push_str("</div>\n");
        }

        html.push_str("</body></html>");
        html
    }
}

fn summarize_args(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) => map
            .iter()
            .take(2)
            .map(|(k, v)| {
                let v_str = match v {
                    serde_json::Value::String(s) => {
                        if s.len() > 20 {
                            format!("\"{}...\"", &s[..20])
                        } else {
                            format!("\"{}\"", s)
                        }
                    }
                    _ => v.to_string(),
                };
                format!("{}: {}", k, v_str)
            })
            .collect::<Vec<_>>()
            .join(", "),
        _ => args.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Policy, SequenceRule, ToolsPolicy};
    use crate::on_error::ErrorPolicy;

    fn make_policy(rules: Vec<SequenceRule>) -> Policy {
        Policy {
            version: "1.1".to_string(),
            name: "test".to_string(),
            metadata: None,
            tools: ToolsPolicy::default(),
            sequences: rules,
            aliases: std::collections::HashMap::new(),
            on_error: ErrorPolicy::default(),
        }
    }

    #[test]
    fn test_explain_simple_trace() {
        let policy = make_policy(vec![SequenceRule::Before {
            first: "Search".to_string(),
            then: "Create".to_string(),
        }]);

        let explainer = TraceExplainer::new(policy);
        let trace = vec![
            ToolCall {
                tool: "Search".to_string(),
                args: None,
            },
            ToolCall {
                tool: "Create".to_string(),
                args: None,
            },
        ];

        let explanation = explainer.explain(&trace);

        assert_eq!(explanation.total_steps, 2);
        assert_eq!(explanation.allowed_steps, 2);
        assert_eq!(explanation.blocked_steps, 0);
    }

    #[test]
    fn test_explain_blocked_trace() {
        let policy = make_policy(vec![SequenceRule::Before {
            first: "Search".to_string(),
            then: "Create".to_string(),
        }]);

        let explainer = TraceExplainer::new(policy);
        let trace = vec![
            ToolCall {
                tool: "Create".to_string(),
                args: None,
            }, // Blocked - no Search first
        ];

        let explanation = explainer.explain(&trace);

        assert_eq!(explanation.blocked_steps, 1);
        assert_eq!(explanation.first_block_index, Some(0));
        assert!(!explanation.blocking_rules.is_empty());
    }

    #[test]
    fn test_explain_max_calls() {
        let policy = make_policy(vec![SequenceRule::MaxCalls {
            tool: "API".to_string(),
            max: 2,
        }]);

        let explainer = TraceExplainer::new(policy);
        let trace = vec![
            ToolCall {
                tool: "API".to_string(),
                args: None,
            },
            ToolCall {
                tool: "API".to_string(),
                args: None,
            },
            ToolCall {
                tool: "API".to_string(),
                args: None,
            }, // Blocked
        ];

        let explanation = explainer.explain(&trace);

        assert_eq!(explanation.allowed_steps, 2);
        assert_eq!(explanation.blocked_steps, 1);
        assert_eq!(explanation.first_block_index, Some(2));
    }

    #[test]
    fn test_terminal_output() {
        let policy = make_policy(vec![]);
        let explainer = TraceExplainer::new(policy);
        let trace = vec![ToolCall {
            tool: "Search".to_string(),
            args: None,
        }];

        let explanation = explainer.explain(&trace);
        let output = explanation.to_terminal();

        assert!(output.contains("Timeline:"));
        assert!(output.contains("[0]"));
        assert!(output.contains("Search"));
    }
}
