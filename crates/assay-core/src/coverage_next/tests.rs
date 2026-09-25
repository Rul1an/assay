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
                first: "SearchKnowledgeBase".into(),
                then: "CreateTicket".into(),
            },
            SequenceRule::MaxCalls {
                tool: "GetCustomerInfo".into(),
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
            unseen_tools: vec!["CreateTicket".into()],
            unexpected_tools: vec![],
        },
        rule_coverage: RuleCoverage {
            total_rules: 2,
            rules_triggered: 1,
            coverage_pct: 50.0,
            untriggered_rules: vec!["max_calls_api_3".to_string()],
        },
        high_risk_gaps: vec![HighRiskGap {
            tool: "DeleteAccount".into(),
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

fn make_tools_only_policy() -> Policy {
    Policy {
        version: "1.1".to_string(),
        name: "tools-only".to_string(),
        metadata: None,
        tools: ToolsPolicy {
            allow: Some(vec!["ToolA".to_string(), "ToolB".to_string()]),
            deny: None,
            require_args: None,
            arg_constraints: None,
        },
        sequences: vec![],
        aliases: HashMap::new(),
        on_error: ErrorPolicy::default(),
    }
}

fn make_rules_only_policy() -> Policy {
    // Blocklist sequences declare rules without declaring tools, so the tool
    // dimension stays not applicable while the rule dimension is populated.
    Policy {
        version: "1.1".to_string(),
        name: "rules-only".to_string(),
        metadata: None,
        tools: ToolsPolicy {
            allow: None,
            deny: None,
            require_args: None,
            arg_constraints: None,
        },
        sequences: vec![
            SequenceRule::Blocklist {
                pattern: "drop_".to_string(),
            },
            SequenceRule::Blocklist {
                pattern: "wipe_".to_string(),
            },
        ],
        aliases: HashMap::new(),
        on_error: ErrorPolicy::default(),
    }
}

fn make_empty_policy() -> Policy {
    Policy {
        version: "1.1".to_string(),
        name: "empty".to_string(),
        metadata: None,
        tools: ToolsPolicy {
            allow: None,
            deny: None,
            require_args: None,
            arg_constraints: None,
        },
        sequences: vec![],
        aliases: HashMap::new(),
        on_error: ErrorPolicy::default(),
    }
}

/// Matrix row 1: a tools-only report with every declared tool observed keeps an
/// overall 100, while the empty rule dimension reads 0 and stays out of the mean.
#[test]
fn tools_only_all_seen_keeps_overall_100_with_rule_not_applicable() {
    let analyzer = CoverageAnalyzer::from_policy(&make_tools_only_policy());
    let traces = vec![TraceRecord {
        trace_id: "t1".to_string(),
        tools_called: vec!["ToolA".to_string(), "ToolB".to_string()],
        rules_triggered: HashSet::new(),
    }];

    let report = analyzer.analyze(&traces, 80.0);

    assert_eq!(report.tool_coverage.coverage_pct, 100.0);
    assert!(report.tool_coverage.is_applicable());
    assert!(!report.rule_coverage.is_applicable());
    assert_eq!(report.rule_coverage.coverage_pct, 0.0);
    assert_eq!(report.applicable_dimensions(), 1);
    assert_eq!(report.not_applicable_reason(), None);
    assert_eq!(report.overall_coverage_pct, 100.0);
    assert!(report.meets_threshold);
}

/// Matrix row 3: a rules-only report means rule coverage over the rule dimension.
#[test]
fn rules_only_report_means_rule_coverage_with_tool_not_applicable() {
    let analyzer = CoverageAnalyzer::from_policy(&make_rules_only_policy());
    let traces = vec![TraceRecord {
        trace_id: "t1".to_string(),
        tools_called: vec![],
        rules_triggered: HashSet::from(["blocklist_drop_".to_string()]),
    }];

    let report = analyzer.analyze(&traces, 80.0);

    assert!(!report.tool_coverage.is_applicable());
    assert_eq!(report.tool_coverage.coverage_pct, 0.0);
    assert!(report.rule_coverage.is_applicable());
    assert_eq!(report.rule_coverage.coverage_pct, 50.0);
    assert_eq!(report.applicable_dimensions(), 1);
    assert_eq!(report.overall_coverage_pct, 50.0);
    assert!(!report.meets_threshold);
}

/// Matrix row 5: with no applicable dimension the threshold is refused at every
/// threshold, including 0, with a stated reason.
#[test]
fn empty_policy_refuses_threshold_zero_with_reason() {
    let analyzer = CoverageAnalyzer::from_policy(&make_empty_policy());
    let traces = vec![TraceRecord {
        trace_id: "t1".to_string(),
        tools_called: vec![],
        rules_triggered: HashSet::new(),
    }];

    let report = analyzer.analyze(&traces, 0.0);

    assert_eq!(report.tool_coverage.coverage_pct, 0.0);
    assert_eq!(report.rule_coverage.coverage_pct, 0.0);
    assert_eq!(report.overall_coverage_pct, 0.0);
    assert_eq!(report.applicable_dimensions(), 0);
    assert_eq!(
        report.not_applicable_reason(),
        Some("Coverage not applicable: policy declares no tools and no rules")
    );
    assert!(!report.meets_threshold);
}

/// The single applicability predicate: a dimension applies iff anything is declared.
#[test]
fn applicability_derives_from_declared_totals() {
    assert!(CoverageReport::dimension_is_applicable(1));
    assert!(!CoverageReport::dimension_is_applicable(0));

    let report = CoverageAnalyzer::from_policy(&make_tools_only_policy()).analyze(&[], 0.0);
    assert!(report.tool_coverage.is_applicable());
    assert!(!report.rule_coverage.is_applicable());
}

#[test]
fn rule_ids_outside_the_policy_do_not_count_as_triggered_rules() {
    let policy = make_policy();
    let analyzer = CoverageAnalyzer::from_policy(&policy);

    // Every policy tool is seen, but no policy rule is triggered: the trace only
    // names rule ids the policy does not contain.
    let traces = vec![TraceRecord {
        trace_id: "t1".to_string(),
        tools_called: vec![
            "SearchKnowledgeBase".to_string(),
            "GetCustomerInfo".to_string(),
            "CreateTicket".to_string(),
            "DeleteAccount".to_string(),
        ],
        rules_triggered: HashSet::from([
            "not_a_policy_rule_1".to_string(),
            "not_a_policy_rule_2".to_string(),
            "not_a_policy_rule_3".to_string(),
        ]),
    }];

    let report = analyzer.analyze(&traces, 80.0);

    assert_eq!(report.rule_coverage.total_rules, 2);
    assert_eq!(report.rule_coverage.rules_triggered, 0);
    assert_eq!(report.rule_coverage.coverage_pct, 0.0);
    assert_eq!(report.rule_coverage.untriggered_rules.len(), 2);
    assert_eq!(report.overall_coverage_pct, 50.0);
    assert!(!report.meets_threshold);
}

#[test]
fn triggered_and_untriggered_rules_partition_the_policy_rules() {
    let policy = make_policy();
    let analyzer = CoverageAnalyzer::from_policy(&policy);

    let traces = vec![TraceRecord {
        trace_id: "t1".to_string(),
        tools_called: vec!["SearchKnowledgeBase".to_string()],
        rules_triggered: HashSet::from([
            "max_calls_getcustomerinfo_3".to_string(),
            "not_a_policy_rule".to_string(),
        ]),
    }];

    let report = analyzer.analyze(&traces, 80.0);

    assert_eq!(report.rule_coverage.rules_triggered, 1);
    assert_eq!(
        report.rule_coverage.rules_triggered + report.rule_coverage.untriggered_rules.len(),
        report.rule_coverage.total_rules
    );
    assert_eq!(report.rule_coverage.coverage_pct, 50.0);
}
