use super::*;
use crate::model::{Policy, SequenceRule, ToolsPolicy};
use crate::on_error::ErrorPolicy;
use std::collections::{HashMap, HashSet};

fn make_policy() -> Policy {
    Policy {
        version: "1.1".to_string(),
        name: "test".to_string(),
        metadata: None,
        tools: ToolsPolicy {
            allow: Some(vec![
                "SearchKnowledgeBase".to_string(),
                "GetCustomerInfo".to_string(),
                "CreateTicket".to_string(),
            ]),
            deny: Some(vec!["DeleteAccount".to_string()]),
            require_args: None,
            arg_constraints: None,
        },
        sequences: vec![
            SequenceRule::Before {
                first: "SearchKnowledgeBase".to_string(),
                then: "CreateTicket".to_string(),
            },
            SequenceRule::MaxCalls {
                tool: "GetCustomerInfo".to_string(),
                max: 3,
            },
        ],
        aliases: HashMap::new(),
        on_error: ErrorPolicy::default(),
    }
}

#[test]
fn test_full_coverage() {
    let policy = make_policy();
    let analyzer = CoverageAnalyzer::from_policy(&policy);

    let traces = vec![TraceRecord {
        trace_id: "t1".to_string(),
        tools_called: vec![
            "SearchKnowledgeBase".to_string(),
            "GetCustomerInfo".to_string(),
            "CreateTicket".to_string(),
            "DeleteAccount".to_string(), // High-risk, but tested
        ],
        rules_triggered: HashSet::from([
            "before_searchknowledgebase_then_createticket".to_string(),
            "max_calls_getcustomerinfo_3".to_string(),
        ]),
    }];

    let report = analyzer.analyze(&traces, 80.0);

    assert_eq!(report.tool_coverage.tools_seen_in_traces, 4);
    assert!(report.tool_coverage.unseen_tools.is_empty());
    assert!(report.high_risk_gaps.is_empty()); // DeleteAccount was seen
    assert!(report.meets_threshold);
}

#[test]
fn test_partial_coverage() {
    let policy = make_policy();
    let analyzer = CoverageAnalyzer::from_policy(&policy);

    let traces = vec![TraceRecord {
        trace_id: "t1".to_string(),
        tools_called: vec!["SearchKnowledgeBase".to_string()],
        rules_triggered: HashSet::new(),
    }];

    let report = analyzer.analyze(&traces, 80.0);

    assert_eq!(report.tool_coverage.tools_seen_in_traces, 1);
    assert!(report
        .tool_coverage
        .unseen_tools
        .contains(&"CreateTicket".to_string()));
    assert!(report
        .tool_coverage
        .unseen_tools
        .contains(&"GetCustomerInfo".to_string()));
    assert!(!report.high_risk_gaps.is_empty()); // DeleteAccount not seen
    assert!(!report.meets_threshold);
}

#[test]
fn test_unexpected_tools() {
    let policy = make_policy();
    let analyzer = CoverageAnalyzer::from_policy(&policy);

    let traces = vec![TraceRecord {
        trace_id: "t1".to_string(),
        tools_called: vec![
            "SearchKnowledgeBase".to_string(),
            "UnknownTool".to_string(), // Not in policy
        ],
        rules_triggered: HashSet::new(),
    }];

    let report = analyzer.analyze(&traces, 50.0);

    assert!(report
        .tool_coverage
        .unexpected_tools
        .contains(&"UnknownTool".to_string()));
}

#[test]
fn test_github_annotation_format() {
    let report = CoverageReport {
        tool_coverage: ToolCoverage {
            total_tools_in_policy: 4,
            tools_seen_in_traces: 2,
            coverage_pct: 50.0,
            unseen_tools: vec!["CreateTicket".to_string()],
            unexpected_tools: vec![],
        },
        rule_coverage: RuleCoverage {
            total_rules: 2,
            rules_triggered: 1,
            coverage_pct: 50.0,
            untriggered_rules: vec!["max_calls_api_3".to_string()],
        },
        high_risk_gaps: vec![HighRiskGap {
            tool: "DeleteAccount".to_string(),
            reason: "Never tested".to_string(),
            severity: "high".to_string(),
        }],
        policy_violations: vec![],
        policy_warnings: vec![],
        overall_coverage_pct: 50.0,
        meets_threshold: false,
        threshold: 80.0,
    };

    let annotation = report.to_github_annotation();

    assert!(annotation.contains("::error::Coverage 50.0% is below threshold 80.0%"));
    assert!(annotation.contains("::warning::High-risk tool 'DeleteAccount'"));
    assert!(annotation.contains("::notice::Tool 'CreateTicket'"));
}
