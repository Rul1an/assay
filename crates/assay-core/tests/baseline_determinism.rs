//! Determinism tests for baseline management
//!
//! Verifies that baseline operations are deterministic:
//! - Same traces → identical baseline
//! - Same baseline → identical diff results

use assay_core::baseline::{Baseline, BaselineEntry, GitInfo};
use assay_core::coverage::{CoverageAnalyzer, TraceRecord};
use assay_core::model::{Policy, ToolsPolicy, SequenceRule};
use std::collections::{HashMap, HashSet};
use tempfile::TempDir;

fn make_policy() -> Policy {
    Policy {
        version: "1.1".to_string(),
        name: "determinism-test".to_string(),
        metadata: None,
        tools: ToolsPolicy {
            allow: Some(vec![
                "Alpha".to_string(),
                "Beta".to_string(),
                "Gamma".to_string(),
            ]),
            deny: Some(vec!["Danger".to_string()]),
            require_args: None,
            arg_constraints: None,
        },
        sequences: vec![
            SequenceRule::MaxCalls {
                tool: "Alpha".to_string(),
                max: 5,
            },
        ],
        aliases: HashMap::new(),
        on_error: assay_core::on_error::ErrorPolicy::default(),
    }
}

fn make_traces() -> Vec<TraceRecord> {
    vec![
        TraceRecord {
            trace_id: "trace_1".to_string(),
            tools_called: vec!["Alpha".to_string(), "Beta".to_string()],
            rules_triggered: HashSet::new(),
        },
        TraceRecord {
            trace_id: "trace_2".to_string(),
            tools_called: vec!["Alpha".to_string(), "Gamma".to_string()],
            rules_triggered: HashSet::new(),
        },
    ]
}

// ==================== DETERMINISM TESTS ====================

#[test]
fn test_baseline_save_twice_identical() {
    let dir = TempDir::new().unwrap();
    let policy = make_policy();
    let traces = make_traces();

    let analyzer = CoverageAnalyzer::from_policy(&policy);
    let report = analyzer.analyze(&traces, 0.0);

    let fixed_time = "2026-01-01T12:00:00Z".to_string();

    let entries = vec![
        BaselineEntry {
            test_id: "coverage".to_string(),
            metric: "overall".to_string(),
            score: report.overall_coverage_pct,
            meta: None,
        },
        BaselineEntry {
            test_id: "coverage".to_string(),
            metric: "tool".to_string(),
            score: report.tool_coverage.coverage_pct,
            meta: None,
        },
    ];

    let baseline1 = Baseline {
        schema_version: 1,
        created_at: fixed_time.clone(),
        git_info: Some(GitInfo {
            commit: "abc123".to_string(),
            branch: Some("main".to_string()),
            dirty: false,
            author: None,
            timestamp: None,
        }),
        suite: policy.name.clone(),
        assay_version: "1.0.0".to_string(),
        config_fingerprint: "fixed_hash".to_string(),
        entries: entries.clone(),
    };

    let baseline2 = baseline1.clone();

    let path1 = dir.path().join("baseline1.yaml");
    let path2 = dir.path().join("baseline2.yaml");

    baseline1.save(&path1).unwrap();
    baseline2.save(&path2).unwrap();

    let content1 = std::fs::read_to_string(&path1).unwrap();
    let content2 = std::fs::read_to_string(&path2).unwrap();

    assert_eq!(content1, content2, "Baseline files must be byte-identical");
}

#[test]
fn test_baseline_roundtrip_determinism() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("baseline.yaml");

    let baseline = Baseline {
        schema_version: 1,
        created_at: "2026-01-01T12:00:00Z".to_string(),
        git_info: Some(GitInfo {
            commit: "def456".to_string(),
            branch: Some("feature".to_string()),
            dirty: true,
            author: None,
            timestamp: None,
        }),
        suite: "roundtrip-test".to_string(),
        assay_version: "1.0.0".to_string(),
        config_fingerprint: "abc123def456".to_string(),
        entries: vec![
             BaselineEntry {
                test_id: "coverage".to_string(),
                metric: "overall".to_string(),
                score: 75.5,
                meta: None,
            },
        ],
    };

    baseline.save(&path).unwrap();
    let content_before = std::fs::read_to_string(&path).unwrap();

    let loaded = Baseline::load(&path).unwrap();
    loaded.save(&path).unwrap();
    let content_after = std::fs::read_to_string(&path).unwrap();

    assert_eq!(content_before, content_after, "Roundtrip must preserve exact content");
}

#[test]
fn test_diff_determinism() {
    let baseline = Baseline {
        schema_version: 1,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        git_info: None,
        suite: "test".to_string(),
        assay_version: "1.0.0".to_string(),
        config_fingerprint: "hash".to_string(),
        entries: vec![
             BaselineEntry {
                test_id: "coverage".to_string(),
                metric: "overall".to_string(),
                score: 80.0,
                meta: None,
            },
        ],
    };

    let policy = make_policy();
    let traces = make_traces();

    let analyzer = CoverageAnalyzer::from_policy(&policy);
    let report = analyzer.analyze(&traces, 0.0);

    let candidate = Baseline::from_coverage_report(
        &report,
        "test".to_string(),
        "hash".to_string(),
        None,
    );

    let diff1 = baseline.diff(&candidate);
    let diff2 = baseline.diff(&candidate);
    let diff3 = baseline.diff(&candidate);

    let json1 = serde_json::to_string(&diff1).unwrap();
    let json2 = serde_json::to_string(&diff2).unwrap();
    let json3 = serde_json::to_string(&diff3).unwrap();

    assert_eq!(json1, json2);
    assert_eq!(json2, json3);
}

#[test]
fn test_coverage_analysis_determinism() {
    let policy = make_policy();
    let traces = make_traces();

    let analyzer = CoverageAnalyzer::from_policy(&policy);

    let report1 = analyzer.analyze(&traces, 80.0);
    let report2 = analyzer.analyze(&traces, 80.0);
    let report3 = analyzer.analyze(&traces, 80.0);

    assert_eq!(report1.overall_coverage_pct, report2.overall_coverage_pct);
    assert_eq!(report1.meets_threshold, report2.meets_threshold);
    assert_eq!(
        report1.tool_coverage.tools_seen_in_traces,
        report2.tool_coverage.tools_seen_in_traces
    );
}

#[test]
fn test_trace_order_independence() {
    let policy = make_policy();

    let traces_order1 = vec![
        TraceRecord {
            trace_id: "a".to_string(),
            tools_called: vec!["Alpha".to_string()],
            rules_triggered: HashSet::new(),
        },
        TraceRecord {
            trace_id: "b".to_string(),
            tools_called: vec!["Beta".to_string()],
            rules_triggered: HashSet::new(),
        },
    ];

    let traces_order2 = vec![
        TraceRecord {
            trace_id: "b".to_string(),
            tools_called: vec!["Beta".to_string()],
            rules_triggered: HashSet::new(),
        },
        TraceRecord {
            trace_id: "a".to_string(),
            tools_called: vec!["Alpha".to_string()],
            rules_triggered: HashSet::new(),
        },
    ];

    let analyzer = CoverageAnalyzer::from_policy(&policy);

    let report1 = analyzer.analyze(&traces_order1, 0.0);
    let report2 = analyzer.analyze(&traces_order2, 0.0);

    assert_eq!(
        report1.overall_coverage_pct,
        report2.overall_coverage_pct,
    );

    assert_eq!(
        report1.tool_coverage.tools_seen_in_traces,
        report2.tool_coverage.tools_seen_in_traces,
    );
}

#[test]
fn test_removing_trace_triggers_regression() {
    let policy = make_policy();

    let full_traces = vec![
        TraceRecord {
            trace_id: "a".to_string(),
            tools_called: vec!["Alpha".to_string(), "Beta".to_string()],
            rules_triggered: HashSet::new(),
        },
        TraceRecord {
            trace_id: "b".to_string(),
            tools_called: vec!["Gamma".to_string()],
            rules_triggered: HashSet::new(),
        },
    ];

    let reduced_traces = vec![
        TraceRecord {
            trace_id: "a".to_string(),
            tools_called: vec!["Alpha".to_string(), "Beta".to_string()],
            rules_triggered: HashSet::new(),
        },
    ];

    let analyzer = CoverageAnalyzer::from_policy(&policy);

    let baseline_report = analyzer.analyze(&full_traces, 0.0);
    let baseline = Baseline::from_coverage_report(
        &baseline_report,
        "test".to_string(),
        "fingerprint".to_string(),
        None,
    );

    let current_report = analyzer.analyze(&reduced_traces, 0.0);

     let candidate = Baseline::from_coverage_report(
        &current_report,
        "test".to_string(),
        "fingerprint".to_string(),
        None,
    );

    let diff = baseline.diff(&candidate);

    assert!(!diff.regressions.is_empty(), "Removing a trace should cause a regression");
    let reg = diff.regressions.iter().find(|r| r.metric == "overall").unwrap();
    assert!(reg.delta < 0.0);
}

#[test]
fn test_adding_trace_no_regression() {
    let policy = make_policy();

    let baseline_traces = vec![
        TraceRecord {
            trace_id: "a".to_string(),
            tools_called: vec!["Alpha".to_string()],
            rules_triggered: HashSet::new(),
        },
    ];

    let extended_traces = vec![
        TraceRecord {
            trace_id: "a".to_string(),
            tools_called: vec!["Alpha".to_string()],
            rules_triggered: HashSet::new(),
        },
        TraceRecord {
            trace_id: "b".to_string(),
            tools_called: vec!["Beta".to_string(), "Gamma".to_string()],
            rules_triggered: HashSet::new(),
        },
    ];

    let analyzer = CoverageAnalyzer::from_policy(&policy);

    let baseline_report = analyzer.analyze(&baseline_traces, 0.0);
    let baseline = Baseline::from_coverage_report(
        &baseline_report,
        "test".to_string(),
        "fingerprint".to_string(),
        None,
    );

    let current_report = analyzer.analyze(&extended_traces, 0.0);
    let candidate = Baseline::from_coverage_report(
        &current_report,
        "test".to_string(),
        "fingerprint".to_string(),
        None,
    );

    let diff = baseline.diff(&candidate);

    assert!(
        diff.regressions.is_empty(),
        "Adding traces should not cause regression"
    );
}

#[test]
fn test_yaml_format_stable() {
    let baseline = Baseline {
        schema_version: 1,
        created_at: "2026-01-15T10:30:00Z".to_string(),
        git_info: Some(GitInfo {
            commit: "abc123".to_string(),
            branch: Some("main".to_string()),
            dirty: false,
            author: None,
            timestamp: None,
        }),
        suite: "stable-test".to_string(),
        assay_version: "1.0.0".to_string(),
        config_fingerprint: "deadbeef".to_string(),
        entries: vec![
            BaselineEntry {
                test_id: "coverage".to_string(),
                metric: "overall".to_string(),
                score: 85.0,
                meta: None,
            },
        ],
    };

    let yaml = serde_json::to_string_pretty(&baseline).unwrap();

    assert!(yaml.contains("\"schema_version\": 1"));
    assert!(yaml.contains("\"suite\": \"stable-test\""));
    assert!(yaml.contains("\"config_fingerprint\": \"deadbeef\""));
    assert!(yaml.contains("\"score\": 85.0"));
}

#[test]
fn test_json_export_deterministic() {
    let baseline = Baseline {
        schema_version: 1,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        git_info: None,
        suite: "json-test".to_string(),
        assay_version: "1.0.0".to_string(),
        config_fingerprint: "hash".to_string(),
        entries: vec![
            BaselineEntry {
                test_id: "coverage".to_string(),
                metric: "overall".to_string(),
                score: 50.0,
                meta: None,
            },
        ],
    };

    let json1 = serde_json::to_string_pretty(&baseline).unwrap();
    let json2 = serde_json::to_string_pretty(&baseline).unwrap();

    assert_eq!(json1, json2, "JSON serialization must be deterministic");
}
