use super::{ToolCall, TraceExplainer};
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
    let trace = vec![ToolCall {
        tool: "Create".to_string(),
        args: None,
    }];

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
        },
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
