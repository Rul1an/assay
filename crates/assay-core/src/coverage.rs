//! Coverage metrics for Assay policies
//!
//! Analyzes traces to determine:
//! - Tool coverage: which tools from policy were exercised
//! - Rule coverage: which rules were triggered
//! - Gap detection: high-risk tools never seen in traces

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Coverage analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    /// Tool coverage metrics
    pub tool_coverage: ToolCoverage,

    /// Rule coverage metrics
    pub rule_coverage: RuleCoverage,

    /// High-risk gaps (blocklisted tools never seen)
    pub high_risk_gaps: Vec<HighRiskGap>,

    /// Overall coverage percentage
    pub overall_coverage_pct: f64,

    /// Whether coverage meets threshold
    pub meets_threshold: bool,

    /// Threshold that was checked
    pub threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCoverage {
    /// Total unique tools referenced in policy
    pub total_tools_in_policy: usize,

    /// Tools that appeared in at least one trace
    pub tools_seen_in_traces: usize,

    /// Coverage percentage
    pub coverage_pct: f64,

    /// Tools in policy but never seen
    pub unseen_tools: Vec<String>,

    /// Tools seen in traces but not in policy (potential gaps)
    pub unexpected_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCoverage {
    /// Total rules in policy
    pub total_rules: usize,

    /// Rules that were triggered (evaluated to allow or deny)
    pub rules_triggered: usize,

    /// Coverage percentage
    pub coverage_pct: f64,

    /// Rules that were never triggered
    pub untriggered_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighRiskGap {
    /// Tool name
    pub tool: String,

    /// Why it's high risk
    pub reason: String,

    /// Severity: "critical", "high", "medium"
    pub severity: String,
}

/// Trace data for coverage analysis
#[derive(Debug, Clone)]
pub struct TraceRecord {
    pub trace_id: String,
    pub tools_called: Vec<String>,
    pub rules_triggered: HashSet<String>,
}

/// Coverage analyzer
pub struct CoverageAnalyzer {
    /// Tools referenced in policy (from allow, deny, sequences)
    policy_tools: HashSet<String>,

    /// High-risk tools (from deny list, blocklist patterns)
    high_risk_tools: HashSet<String>,

    /// Rule IDs in policy
    rule_ids: Vec<String>,

    /// Resolved aliases (alias -> members)
    aliases: HashMap<String, Vec<String>>,
}

impl CoverageAnalyzer {
    /// Create analyzer from a v1.1 policy
    pub fn from_policy(policy: &crate::model::Policy) -> Self {
        let mut policy_tools = HashSet::new();
        let mut high_risk_tools = HashSet::new();
        let mut rule_ids = Vec::new();

        // Extract tools from policy.tools
        if let Some(allow) = &policy.tools.allow {
            for tool in allow {
                policy_tools.insert(tool.clone());
            }
        }

        if let Some(deny) = &policy.tools.deny {
            for tool in deny {
                policy_tools.insert(tool.clone());
                high_risk_tools.insert(tool.clone()); // Denied = high risk
            }
        }

        if let Some(require_args) = &policy.tools.require_args {
            for tool in require_args.keys() {
                policy_tools.insert(tool.clone());
            }
        }

        // Extract tools from sequences
        for (idx, rule) in policy.sequences.iter().enumerate() {
            let rule_id = Self::rule_id(rule, idx);
            rule_ids.push(rule_id);

            match rule {
                crate::model::SequenceRule::Require { tool } => {
                    policy_tools.insert(tool.clone());
                }
                crate::model::SequenceRule::Eventually { tool, .. } => {
                    policy_tools.insert(tool.clone());
                }
                crate::model::SequenceRule::MaxCalls { tool, .. } => {
                    policy_tools.insert(tool.clone());
                }
                crate::model::SequenceRule::Before { first, then } => {
                    policy_tools.insert(first.clone());
                    policy_tools.insert(then.clone());
                }
                crate::model::SequenceRule::After { trigger, then, .. } => {
                    policy_tools.insert(trigger.clone());
                    policy_tools.insert(then.clone());
                }
                crate::model::SequenceRule::NeverAfter { trigger, forbidden } => {
                    policy_tools.insert(trigger.clone());
                    policy_tools.insert(forbidden.clone());
                    high_risk_tools.insert(forbidden.clone()); // Forbidden = high risk
                }
                crate::model::SequenceRule::Sequence { tools, .. } => {
                    for tool in tools {
                        policy_tools.insert(tool.clone());
                    }
                }
                crate::model::SequenceRule::Blocklist { pattern } => {
                    // Pattern-based, mark as high risk indicator
                    high_risk_tools.insert(format!("*{}*", pattern));
                }
            }
        }

        // Resolve aliases - add alias members to policy_tools
        for (alias, members) in &policy.aliases {
            policy_tools.insert(alias.clone());
            for member in members {
                policy_tools.insert(member.clone());
            }
        }

        Self {
            policy_tools,
            high_risk_tools,
            rule_ids,
            aliases: policy.aliases.clone(),
        }
    }

    /// Generate a rule ID from rule type and index
    fn rule_id(rule: &crate::model::SequenceRule, _idx: usize) -> String {
        match rule {
            crate::model::SequenceRule::Require { tool } => {
                format!("require_{}", tool.to_lowercase())
            }
            crate::model::SequenceRule::Eventually { tool, within } => {
                format!("eventually_{}_{}", tool.to_lowercase(), within)
            }
            crate::model::SequenceRule::MaxCalls { tool, max } => {
                format!("max_calls_{}_{}", tool.to_lowercase(), max)
            }
            crate::model::SequenceRule::Before { first, then } => {
                format!(
                    "before_{}_then_{}",
                    first.to_lowercase(),
                    then.to_lowercase()
                )
            }
            crate::model::SequenceRule::After { trigger, then, .. } => {
                format!(
                    "after_{}_then_{}",
                    trigger.to_lowercase(),
                    then.to_lowercase()
                )
            }
            crate::model::SequenceRule::NeverAfter { trigger, forbidden } => {
                format!(
                    "never_after_{}_forbidden_{}",
                    trigger.to_lowercase(),
                    forbidden.to_lowercase()
                )
            }
            crate::model::SequenceRule::Sequence { tools, strict } => {
                let mode = if *strict { "strict" } else { "seq" };
                format!("{}_{}", mode, tools.join("_").to_lowercase())
            }
            crate::model::SequenceRule::Blocklist { pattern } => {
                format!("blocklist_{}", pattern.to_lowercase())
            }
        }
    }

    /// Analyze coverage from a set of traces
    pub fn analyze(&self, traces: &[TraceRecord], threshold: f64) -> CoverageReport {
        let mut tools_seen: HashSet<String> = HashSet::new();
        let mut rules_triggered: HashSet<String> = HashSet::new();
        let mut unexpected_tools: HashSet<String> = HashSet::new();

        // Collect all tools and triggered rules from traces
        for trace in traces {
            for tool in &trace.tools_called {
                tools_seen.insert(tool.clone());

                // Check if tool is in policy (including alias resolution)
                if !self.is_policy_tool(tool) {
                    unexpected_tools.insert(tool.clone());
                }
            }

            for rule_id in &trace.rules_triggered {
                rules_triggered.insert(rule_id.clone());
            }
        }

        // Calculate tool coverage
        let policy_tool_count = self.policy_tools.len();
        let seen_policy_tools: HashSet<_> = tools_seen
            .iter()
            .filter(|t| self.is_policy_tool(t))
            .cloned()
            .collect();
        let tools_seen_count = seen_policy_tools.len();

        let unseen_tools: Vec<String> = self
            .policy_tools
            .iter()
            .filter(|t| !self.is_tool_seen(t, &tools_seen))
            .cloned()
            .collect();

        let tool_coverage_pct = if policy_tool_count > 0 {
            (tools_seen_count as f64 / policy_tool_count as f64) * 100.0
        } else {
            100.0
        };

        // Calculate rule coverage
        let total_rules = self.rule_ids.len();
        let triggered_count = rules_triggered.len();

        let untriggered_rules: Vec<String> = self
            .rule_ids
            .iter()
            .filter(|r| !rules_triggered.contains(*r))
            .cloned()
            .collect();

        let rule_coverage_pct = if total_rules > 0 {
            (triggered_count as f64 / total_rules as f64) * 100.0
        } else {
            100.0
        };

        // Identify high-risk gaps
        let high_risk_gaps: Vec<HighRiskGap> = self
            .high_risk_tools
            .iter()
            .filter(|t| !t.starts_with('*')) // Skip patterns
            .filter(|t| !self.is_tool_seen(t, &tools_seen))
            .map(|t| HighRiskGap {
                tool: t.clone(),
                reason: "Tool is in deny list but never appeared in test traces".to_string(),
                severity: "high".to_string(),
            })
            .collect();

        // Overall coverage (average of tool and rule coverage)
        let overall_coverage_pct = (tool_coverage_pct + rule_coverage_pct) / 2.0;
        let meets_threshold = overall_coverage_pct >= threshold;

        CoverageReport {
            tool_coverage: ToolCoverage {
                total_tools_in_policy: policy_tool_count,
                tools_seen_in_traces: tools_seen_count,
                coverage_pct: tool_coverage_pct,
                unseen_tools,
                unexpected_tools: unexpected_tools.into_iter().collect(),
            },
            rule_coverage: RuleCoverage {
                total_rules,
                rules_triggered: triggered_count,
                coverage_pct: rule_coverage_pct,
                untriggered_rules,
            },
            high_risk_gaps,
            overall_coverage_pct,
            meets_threshold,
            threshold,
        }
    }

    /// Check if a tool is in the policy (including alias resolution)
    fn is_policy_tool(&self, tool: &str) -> bool {
        if self.policy_tools.contains(tool) {
            return true;
        }

        // Check if tool is a member of any alias
        for members in self.aliases.values() {
            if members.contains(&tool.to_string()) {
                return true;
            }
        }

        false
    }

    /// Check if a tool (or any of its alias members) was seen
    fn is_tool_seen(&self, tool: &str, seen: &HashSet<String>) -> bool {
        if seen.contains(tool) {
            return true;
        }

        // Check if this tool is an alias and any member was seen
        if let Some(members) = self.aliases.get(tool) {
            return members.iter().any(|m| seen.contains(m));
        }

        // Check if tool is a member of an alias that was seen
        for (alias, members) in &self.aliases {
            if members.contains(&tool.to_string()) && seen.contains(alias) {
                return true;
            }
        }

        false
    }
}

impl CoverageReport {
    /// Format as GitHub Actions annotation
    pub fn to_github_annotation(&self) -> String {
        let mut lines = Vec::new();

        if !self.meets_threshold {
            lines.push(format!(
                "::error::Coverage {:.1}% is below threshold {:.1}%",
                self.overall_coverage_pct, self.threshold
            ));
        }

        for gap in &self.high_risk_gaps {
            lines.push(format!(
                "::warning::High-risk tool '{}' never tested: {}",
                gap.tool, gap.reason
            ));
        }

        for tool in &self.tool_coverage.unseen_tools {
            lines.push(format!(
                "::notice::Tool '{}' in policy but not covered by tests",
                tool
            ));
        }

        lines.join("\n")
    }

    /// Format as markdown summary
    pub fn to_markdown(&self) -> String {
        let status = if self.meets_threshold { "✅" } else { "❌" };

        let mut md = format!(
            "## Coverage Report {}\n\n\
            | Metric | Value |\n\
            |--------|-------|\n\
            | Overall Coverage | {:.1}% |\n\
            | Tool Coverage | {:.1}% ({}/{}) |\n\
            | Rule Coverage | {:.1}% ({}/{}) |\n\
            | Threshold | {:.1}% |\n\n",
            status,
            self.overall_coverage_pct,
            self.tool_coverage.coverage_pct,
            self.tool_coverage.tools_seen_in_traces,
            self.tool_coverage.total_tools_in_policy,
            self.rule_coverage.coverage_pct,
            self.rule_coverage.rules_triggered,
            self.rule_coverage.total_rules,
            self.threshold,
        );

        if !self.high_risk_gaps.is_empty() {
            md.push_str("### ⚠️ High-Risk Gaps\n\n");
            for gap in &self.high_risk_gaps {
                md.push_str(&format!("- **{}**: {}\n", gap.tool, gap.reason));
            }
            md.push('\n');
        }

        if !self.tool_coverage.unseen_tools.is_empty() {
            md.push_str("### Uncovered Tools\n\n");
            for tool in &self.tool_coverage.unseen_tools {
                md.push_str(&format!("- `{}`\n", tool));
            }
            md.push('\n');
        }

        if !self.rule_coverage.untriggered_rules.is_empty() {
            md.push_str("### Untriggered Rules\n\n");
            for rule in &self.rule_coverage.untriggered_rules {
                md.push_str(&format!("- `{}`\n", rule));
            }
            md.push('\n');
        }

        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Policy, SequenceRule, ToolsPolicy};
    use crate::on_error::ErrorPolicy;

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
            overall_coverage_pct: 50.0,
            meets_threshold: false,
            threshold: 80.0,
        };

        let annotation = report.to_github_annotation();

        assert!(annotation.contains("::error::Coverage 50.0% is below threshold 80.0%"));
        assert!(annotation.contains("::warning::High-risk tool 'DeleteAccount'"));
        assert!(annotation.contains("::notice::Tool 'CreateTicket'"));
    }
}
