//! Integration tests for trace explanation
//!
//! Tests the full explain workflow from trace to visualization.

use assay_core::experimental::explain::{StepVerdict, ToolCall, TraceExplainer};
use assay_core::model::{Policy, SequenceRule, ToolsPolicy};
use assay_core::on_error::ErrorPolicy;
use std::collections::HashMap;

fn make_policy(rules: Vec<SequenceRule>) -> Policy {
    Policy {
        version: "1.1".to_string(),
        name: "test-policy".to_string(),
        metadata: None,
        tools: ToolsPolicy::default(),
        sequences: rules,
        aliases: HashMap::new(),
        on_error: ErrorPolicy::default(),
    }
}

fn make_policy_with_tools(
    rules: Vec<SequenceRule>,
    allow: Option<Vec<&str>>,
    deny: Option<Vec<&str>>,
) -> Policy {
    Policy {
        version: "1.1".to_string(),
        name: "test-policy".to_string(),
        metadata: None,
        tools: ToolsPolicy {
            allow: allow.map(|v| v.into_iter().map(String::from).collect()),
            deny: deny.map(|v| v.into_iter().map(String::from).collect()),
            require_args: None,
            arg_constraints: None,
        },
        sequences: rules,
        aliases: HashMap::new(),
        on_error: ErrorPolicy::default(),
    }
}

fn trace(tools: &[&str]) -> Vec<ToolCall> {
    tools
        .iter()
        .map(|t| ToolCall {
            tool: t.to_string(),
            args: None,
        })
        .collect()
}

// ==================== BASIC TESTS ====================

#[test]
fn test_explain_empty_trace() {
    let policy = make_policy(vec![]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&[]);

    assert_eq!(explanation.total_steps, 0);
    assert_eq!(explanation.allowed_steps, 0);
    assert_eq!(explanation.blocked_steps, 0);
}

#[test]
fn test_explain_all_allowed() {
    let policy = make_policy(vec![]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["Search", "Create", "Update"]));

    assert_eq!(explanation.total_steps, 3);
    assert_eq!(explanation.allowed_steps, 3);
    assert_eq!(explanation.blocked_steps, 0);
    assert!(explanation.first_block_index.is_none());
}

// ==================== BEFORE RULE TESTS ====================

#[test]
fn test_explain_before_pass() {
    let policy = make_policy(vec![SequenceRule::Before {
        first: "Auth".to_string(),
        then: "Access".to_string(),
    }]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["Auth", "Access"]));

    assert_eq!(explanation.blocked_steps, 0);

    // Check that step 1 (Access) shows the rule passed
    let access_step = &explanation.steps[1];
    assert_eq!(access_step.verdict, StepVerdict::Allowed);

    let before_rule = access_step
        .rules_evaluated
        .iter()
        .find(|r| r.rule_type == "before")
        .expect("before rule should be evaluated");
    assert!(before_rule.passed);
    assert!(before_rule.explanation.contains("was called first"));
}

#[test]
fn test_explain_before_fail() {
    let policy = make_policy(vec![SequenceRule::Before {
        first: "Auth".to_string(),
        then: "Access".to_string(),
    }]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["Access"]));

    assert_eq!(explanation.blocked_steps, 1);
    assert_eq!(explanation.first_block_index, Some(0));

    let access_step = &explanation.steps[0];
    assert_eq!(access_step.verdict, StepVerdict::Blocked);

    let before_rule = access_step
        .rules_evaluated
        .iter()
        .find(|r| r.rule_type == "before")
        .expect("before rule should be evaluated");
    assert!(!before_rule.passed);
    assert!(before_rule.explanation.contains("requires"));
}

// ==================== MAX_CALLS TESTS ====================

#[test]
fn test_explain_max_calls_counting() {
    let policy = make_policy(vec![SequenceRule::MaxCalls {
        tool: "API".to_string(),
        max: 3,
    }]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["API", "API", "API", "API"]));

    // First 3 should pass, 4th should fail
    assert_eq!(explanation.allowed_steps, 3);
    assert_eq!(explanation.blocked_steps, 1);
    assert_eq!(explanation.first_block_index, Some(3));

    // Check progressive counting in explanations
    for (i, step) in explanation.steps.iter().take(3).enumerate() {
        let max_rule = step
            .rules_evaluated
            .iter()
            .find(|r| r.rule_type == "max_calls")
            .expect("max_calls rule should be evaluated");
        assert!(max_rule.passed);
        assert!(max_rule.explanation.contains(&format!("{}/3", i + 1)));
    }

    // Check 4th call shows exceeded
    let blocked_step = &explanation.steps[3];
    let max_rule = blocked_step
        .rules_evaluated
        .iter()
        .find(|r| r.rule_type == "max_calls")
        .expect("max_calls rule should be evaluated");
    assert!(!max_rule.passed);
    assert!(max_rule.explanation.contains("exceeded"));
}

// ==================== EVENTUALLY TESTS ====================

#[test]
fn test_explain_eventually_progress() {
    let policy = make_policy(vec![SequenceRule::Eventually {
        tool: "Validate".to_string(),
        within: 3,
    }]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["Search", "Create", "Validate"]));

    assert_eq!(explanation.blocked_steps, 0);

    // Validate appears at index 2, which is within 3 (indices 0, 1, 2)
    let validate_step = &explanation.steps[2];
    let ev_rule = validate_step
        .rules_evaluated
        .iter()
        .find(|r| r.rule_type == "eventually")
        .expect("eventually rule should be evaluated");
    assert!(ev_rule.passed);
    assert!(ev_rule.explanation.contains("already seen"));
}

#[test]
fn test_explain_eventually_fail() {
    let policy = make_policy(vec![SequenceRule::Eventually {
        tool: "Validate".to_string(),
        within: 2,
    }]);
    let explainer = TraceExplainer::new(policy);

    // Validate never appears, trace exceeds within
    let explanation = explainer.explain(&trace(&["Search", "Create", "Update"]));

    // Should fail at index 2 (third call, exceeds within:2)
    assert!(explanation.blocked_steps > 0);
}

// ==================== NEVER_AFTER TESTS ====================

#[test]
fn test_explain_never_after_triggered() {
    let policy = make_policy(vec![SequenceRule::NeverAfter {
        trigger: "Archive".to_string(),
        forbidden: "Delete".to_string(),
    }]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["Archive", "Delete"]));

    // Archive should be allowed
    assert_eq!(explanation.steps[0].verdict, StepVerdict::Allowed);

    // Delete should be blocked
    assert_eq!(explanation.steps[1].verdict, StepVerdict::Blocked);

    let never_rule = explanation.steps[1]
        .rules_evaluated
        .iter()
        .find(|r| r.rule_type == "never_after")
        .expect("never_after rule should be evaluated");
    assert!(!never_rule.passed);
    assert!(never_rule.explanation.contains("forbidden"));
}

#[test]
fn test_explain_never_after_before_trigger() {
    let policy = make_policy(vec![SequenceRule::NeverAfter {
        trigger: "Archive".to_string(),
        forbidden: "Delete".to_string(),
    }]);
    let explainer = TraceExplainer::new(policy);

    // Delete before Archive is OK
    let explanation = explainer.explain(&trace(&["Delete", "Archive"]));

    assert_eq!(explanation.blocked_steps, 0);
}

// ==================== SEQUENCE TESTS ====================

#[test]
fn test_explain_sequence_progress() {
    let policy = make_policy(vec![SequenceRule::Sequence {
        tools: vec!["A".to_string(), "B".to_string(), "C".to_string()],
        strict: false,
    }]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["A", "X", "B", "Y", "C"]));

    assert_eq!(explanation.blocked_steps, 0);

    // Check sequence progress in explanations
    let step_a = &explanation.steps[0];
    let seq_rule_a = step_a
        .rules_evaluated
        .iter()
        .find(|r| r.rule_type == "sequence")
        .unwrap();
    assert!(seq_rule_a.explanation.contains("1/3"));
}

#[test]
fn test_explain_sequence_strict_violation() {
    let policy = make_policy(vec![SequenceRule::Sequence {
        tools: vec!["A".to_string(), "B".to_string(), "C".to_string()],
        strict: true,
    }]);
    let explainer = TraceExplainer::new(policy);

    // X between A and B violates strict mode
    let explanation = explainer.explain(&trace(&["A", "X", "B", "C"]));

    assert!(explanation.blocked_steps > 0);

    let x_step = &explanation.steps[1];
    assert_eq!(x_step.verdict, StepVerdict::Blocked);
}

// ==================== DENY LIST TESTS ====================

#[test]
fn test_explain_deny_list() {
    let policy = make_policy_with_tools(vec![], None, Some(vec!["DeleteAccount", "DropDatabase"]));
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["Search", "DeleteAccount"]));

    assert_eq!(explanation.steps[0].verdict, StepVerdict::Allowed);
    assert_eq!(explanation.steps[1].verdict, StepVerdict::Blocked);

    let deny_rule = explanation.steps[1]
        .rules_evaluated
        .iter()
        .find(|r| r.rule_type == "deny")
        .expect("deny rule should be evaluated");
    assert!(!deny_rule.passed);
    assert!(deny_rule.explanation.contains("deny list"));
}

// ==================== OUTPUT FORMAT TESTS ====================

#[test]
fn test_terminal_output_format() {
    let policy = make_policy(vec![SequenceRule::MaxCalls {
        tool: "API".to_string(),
        max: 2,
    }]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["API", "API", "API"]));
    let output = explanation.to_terminal();

    assert!(output.contains("Timeline:"));
    assert!(output.contains("[0]"));
    assert!(output.contains("[1]"));
    assert!(output.contains("[2]"));
    assert!(output.contains("✅"));
    assert!(output.contains("❌"));
    assert!(output.contains("BLOCKED"));
}

#[test]
fn test_markdown_output_format() {
    let policy = make_policy(vec![]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["Search"]));
    let output = explanation.to_markdown();

    assert!(output.contains("## Trace Explanation"));
    assert!(output.contains("| # | Tool | Verdict |"));
    assert!(output.contains("| 0 | `Search` |"));
}

#[test]
fn test_html_output_format() {
    let policy = make_policy(vec![]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["Search"]));
    let output = explanation.to_html();

    assert!(output.contains("<!DOCTYPE html>"));
    assert!(output.contains("<title>Trace Explanation</title>"));
    assert!(output.contains("Search"));
}

// ==================== COMBINED RULES TESTS ====================

#[test]
fn test_explain_multiple_rules() {
    let policy = make_policy(vec![
        SequenceRule::Before {
            first: "Auth".to_string(),
            then: "Access".to_string(),
        },
        SequenceRule::MaxCalls {
            tool: "Access".to_string(),
            max: 2,
        },
        SequenceRule::Eventually {
            tool: "Logout".to_string(),
            within: 5,
        },
    ]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["Auth", "Access", "Access", "Logout"]));

    assert_eq!(explanation.blocked_steps, 0);

    // Each step should evaluate multiple rules
    for step in &explanation.steps {
        assert!(step.rules_evaluated.len() >= 2);
    }
}

#[test]
fn test_explain_first_failure_stops_not_evaluation() {
    // Even if first rule blocks, we should still see all rules evaluated
    let policy = make_policy(vec![
        SequenceRule::Before {
            first: "Auth".to_string(),
            then: "Access".to_string(),
        },
        SequenceRule::MaxCalls {
            tool: "Access".to_string(),
            max: 2,
        },
    ]);
    let explainer = TraceExplainer::new(policy);

    let explanation = explainer.explain(&trace(&["Access"])); // No Auth first

    // Should be blocked
    assert_eq!(explanation.blocked_steps, 1);

    // But both rules should be evaluated
    let step = &explanation.steps[0];
    assert!(step.rules_evaluated.iter().any(|r| r.rule_type == "before"));
    assert!(step
        .rules_evaluated
        .iter()
        .any(|r| r.rule_type == "max_calls"));
}

#[test]
fn test_require_end_of_trace_violation() {
    let policy = make_policy(vec![SequenceRule::Require {
        tool: "Audit".to_string(),
    }]);
    let explainer = TraceExplainer::new(policy);

    // Trace ends without Audit ever being called
    let explanation = explainer.explain(&trace(&["Search", "Create"]));

    // Should have violation in blocking_rules
    assert!(explanation
        .blocking_rules
        .iter()
        .any(|r| r.contains("require_audit")));

    // And last step should have the violation appended
    let last_step = explanation.steps.last().unwrap();
    let req_rule = last_step
        .rules_evaluated
        .iter()
        .find(|r| r.rule_type == "require" && !r.passed)
        .expect("require rule failure should be reported at end");
    assert!(req_rule.explanation.contains("never called"));
}

#[test]
fn test_after_end_of_trace_violation() {
    let policy = make_policy(vec![SequenceRule::After {
        trigger: "Create".to_string(),
        then: "Notify".to_string(),
        within: 2,
    }]);
    let explainer = TraceExplainer::new(policy);

    // Create triggered, but trace ends without Notify (and before 2 steps pass)
    // Create(0), Update(1). Trace ends.
    // Deadly sin: trace ended while pending constraints existed.
    let explanation = explainer.explain(&trace(&["Create", "Update"]));

    // Should have violation in blocking_rules
    assert!(explanation
        .blocking_rules
        .iter()
        .any(|r| r.contains("after_create")));

    // Last step (Update) should contain the violation
    let last_step = explanation.steps.last().unwrap();
    let after_rule = last_step
        .rules_evaluated
        .iter()
        .find(|r| r.rule_type == "after" && !r.passed)
        .expect("after rule failure should be reported at end");
    assert!(after_rule.explanation.contains("trace ended"));
}
