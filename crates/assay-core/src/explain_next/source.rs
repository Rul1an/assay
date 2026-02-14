use crate::model::Policy;

use super::diff::ExplainerState;
use super::model::{ExplainedStep, RuleEvaluation, StepVerdict, ToolCall, TraceExplanation};

/// Trace explainer
pub struct TraceExplainer {
    policy: Policy,
}

impl TraceExplainer {
    pub fn new(policy: Policy) -> Self {
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
