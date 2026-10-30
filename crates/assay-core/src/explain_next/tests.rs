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
        first: "Search".into(),
        then: "Create".into(),
    }]);

    let explainer = TraceExplainer::new(policy);
    let trace = vec![
        ToolCall {
            tool: "Search".into(),
            args: None,
        },
        ToolCall {
            tool: "Create".into(),
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
        first: "Search".into(),
        then: "Create".into(),
    }]);

    let explainer = TraceExplainer::new(policy);
    let trace = vec![ToolCall {
        tool: "Create".into(),
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
        tool: "API".into(),
        max: 2,
    }]);

    let explainer = TraceExplainer::new(policy);
    let trace = vec![
        ToolCall {
            tool: "API".into(),
            args: None,
        },
        ToolCall {
            tool: "API".into(),
            args: None,
        },
        ToolCall {
            tool: "API".into(),
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
        tool: "Search".into(),
        args: None,
    }];

    let explanation = explainer.explain(&trace);
    let output = explanation.to_terminal();

    assert!(output.contains("Timeline:"));
    assert!(output.contains("[0]"));
    assert!(output.contains("Search"));
}

#[test]
fn terminal_args_truncate_at_utf8_byte_boundary() {
    for (value, summary) in [
        ("aaaaaaaaaaaaaaaaaaaé", "aaaaaaaaaaaaaaaaaaa..."),
        ("aaaaaaaaaaaaaaaaaa€", "aaaaaaaaaaaaaaaaaa..."),
        ("aaaaaaaaaaaaaaaaa🦀", "aaaaaaaaaaaaaaaaa..."),
        ("aaaaaaaaaaaaaaaaaa🦀", "aaaaaaaaaaaaaaaaaa..."),
        ("aaaaaaaaaaaaaaaaaaa🦀", "aaaaaaaaaaaaaaaaaaa..."),
    ] {
        let args = serde_json::json!({"query": value});
        let explanation = TraceExplainer::new(make_policy(vec![])).explain(&[ToolCall {
            tool: "Search".into(),
            args: Some(args.clone()),
        }]);

        assert!(
            explanation
                .to_terminal()
                .contains(&format!("Search(query: \"{summary}\")")),
            "incorrect summary for {value:?}"
        );
        assert_eq!(explanation.steps[0].args, Some(args));
    }
}

#[test]
fn terminal_args_preserve_byte_budget_and_summary_shape() {
    for (value, summary) in [
        ("", ""),
        ("short", "short"),
        ("abcdefghijklmnopqrst", "abcdefghijklmnopqrst"),
        ("abcdefghijklmnopqrstu", "abcdefghijklmnopqrst..."),
        ("éééééééééé", "éééééééééé"),
        ("ééééééééééé", "éééééééééé..."),
        ("aaaaaaaaaaaaaaaaaaéz", "aaaaaaaaaaaaaaaaaaé..."),
    ] {
        let explanation = TraceExplainer::new(make_policy(vec![])).explain(&[ToolCall {
            tool: "Search".into(),
            args: Some(serde_json::json!({"query": value})),
        }]);
        assert!(
            explanation
                .to_terminal()
                .contains(&format!("Search(query: \"{summary}\")")),
            "incorrect summary for {value:?}"
        );
    }

    let explanation = TraceExplainer::new(make_policy(vec![])).explain(&[ToolCall {
        tool: "Search".into(),
        args: Some(serde_json::json!({"a": 1, "b": true, "c": "omitted"})),
    }]);
    assert!(explanation.to_terminal().contains("Search(a: 1, b: true)"));
}
